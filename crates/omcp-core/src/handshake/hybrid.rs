//! Noise_IK + ML-KEM-768 hybrid handshake for suite `OMCP-2026-A` (PROTOCOL §5.1).
//!
//! # Deviation from unfinished IETF Noise Hybrid draft
//!
//! PROTOCOL §17 open question 2: exact Noise hybrid construction is not pinned.
//! Until Writey pins an IETF draft, this module uses the following **provisional**
//! construction (documented for interop review):
//!
//! 1. Classical pattern `Noise_IK_25519_ChaChaPoly_BLAKE2b` (initiator knows
//!    responder's static X25519 public key).
//! 2. Responder also has a static ML-KEM-768 key pair; the encapsulation key is
//!    known to the initiator out-of-band (same trust channel as the IK static key).
//! 3. Message 1 appends an ML-KEM ciphertext (encapsulation to the responder).
//!    The ciphertext is `mix_hash`'d as cleartext (not AEAD-sealed).
//! 4. After both Noise messages, each side sets
//!    `x25519_ss := Noise chaining key (HASHLEN)` (all classical DH outputs already
//!    mixed into CK via Noise `MixKey`), then
//!    `shared = HKDF-SHA-384(ikm = x25519_ss || mlkem_ss)` per §5.1, with
//!    `salt = handshake_hash` and `info = "OMCP-2026-A/hybrid/v1"`.
//!    Two 32-byte transport keys are expanded (initiator→responder, responder→initiator).
//!
//! This is **not** wire-compatible with snow HFS / PQNoise / Reishi hybrid suites.
//! Revisit when §17 Q2 is resolved.

use hkdf::Hkdf;
use ml_kem::kem::{Ciphertext, Decapsulate, Encapsulate, Kem, KeyExport};
use ml_kem::{EncapsulationKey, MlKem768};
use sha2::Sha384;
use x25519_dalek::{PublicKey as X25519Public, StaticSecret};
use zeroize::{Zeroize, ZeroizeOnDrop};

use super::symmetric::{sealed_dh_len, SymmetricState, DHLEN, HASHLEN};
use crate::error::{OmcpError, Result};

/// Protocol name mixed into the Noise prologue hash.
///
/// Not a registered Noise name (hybrid is outside stock Noise). Kept stable for M1 vectors.
pub const HYBRID_PROTOCOL_NAME: &[u8] = b"OMCP_IK_25519+MLKEM768_ChaChaPoly_BLAKE2b";

/// HKDF info label for the hybrid export (§5.1).
pub const HYBRID_HKDF_INFO: &[u8] = b"OMCP-2026-A/hybrid/v1";

/// ML-KEM-768 encapsulation key size (FIPS 203).
pub const MLKEM768_EK_LEN: usize = 1184;
/// ML-KEM-768 ciphertext size (FIPS 203).
pub const MLKEM768_CT_LEN: usize = 1088;
/// ML-KEM-768 shared secret size.
pub const MLKEM768_SS_LEN: usize = 32;

fn random_32() -> Result<[u8; 32]> {
    let mut b = [0u8; 32];
    getrandom::getrandom(&mut b).map_err(|_| OmcpError::Crypto("getrandom"))?;
    Ok(b)
}

/// Static identity keys for one peer (X25519 + ML-KEM-768).
pub struct StaticIdentity {
    dh: StaticSecret,
    kem_dk: <MlKem768 as Kem>::DecapsulationKey,
    kem_ek_bytes: [u8; MLKEM768_EK_LEN],
    dh_public: [u8; DHLEN],
}

impl StaticIdentity {
    /// Generate a fresh static identity.
    pub fn generate() -> Result<Self> {
        let dh = StaticSecret::from(random_32()?);
        let dh_public = X25519Public::from(&dh).to_bytes();
        let (kem_dk, kem_ek) = MlKem768::generate_keypair();
        let ek_encoded = kem_ek.to_bytes();
        let mut kem_ek_bytes = [0u8; MLKEM768_EK_LEN];
        kem_ek_bytes.copy_from_slice(ek_encoded.as_slice());
        Ok(Self {
            dh,
            kem_dk,
            kem_ek_bytes,
            dh_public,
        })
    }

    /// X25519 static public key bytes.
    pub fn dh_public(&self) -> [u8; DHLEN] {
        self.dh_public
    }

    /// ML-KEM-768 encapsulation key bytes.
    pub fn kem_public(&self) -> &[u8; MLKEM768_EK_LEN] {
        &self.kem_ek_bytes
    }
}

/// Completed hybrid session secrets (PROTOCOL §5.1 / §11 FS).
#[derive(Clone, Zeroize, ZeroizeOnDrop, PartialEq, Eq)]
pub struct HybridSessionKeys {
    /// Initiator → responder transport key (32 bytes).
    pub k_i2r: [u8; 32],
    /// Responder → initiator transport key (32 bytes).
    pub k_r2i: [u8; 32],
    /// Noise handshake hash (public transcript commitment).
    #[zeroize(skip)]
    pub handshake_hash: [u8; HASHLEN],
}

fn derive_session(
    ck: &[u8; HASHLEN],
    mlkem_ss: &[u8; MLKEM768_SS_LEN],
    handshake_hash: &[u8; HASHLEN],
) -> Result<HybridSessionKeys> {
    // PROTOCOL §5.1: shared secret = HKDF(X25519_ss || ML-KEM_ss)
    // Provisional: X25519_ss := Noise CK after IK (all DH MixKeys applied).
    let mut ikm = [0u8; HASHLEN + MLKEM768_SS_LEN];
    ikm[..HASHLEN].copy_from_slice(ck);
    ikm[HASHLEN..].copy_from_slice(mlkem_ss);

    let hk = Hkdf::<Sha384>::new(Some(handshake_hash.as_slice()), &ikm);
    let mut okm = [0u8; 64];
    hk.expand(HYBRID_HKDF_INFO, &mut okm)
        .map_err(|_| OmcpError::Crypto("hkdf expand"))?;

    let mut k_i2r = [0u8; 32];
    let mut k_r2i = [0u8; 32];
    k_i2r.copy_from_slice(&okm[..32]);
    k_r2i.copy_from_slice(&okm[32..]);
    okm.zeroize();
    ikm.zeroize();

    Ok(HybridSessionKeys {
        k_i2r,
        k_r2i,
        handshake_hash: *handshake_hash,
    })
}

fn parse_kem_ek(bytes: &[u8; MLKEM768_EK_LEN]) -> Result<EncapsulationKey<MlKem768>> {
    use ml_kem::kem::TryKeyInit;
    EncapsulationKey::<MlKem768>::new_from_slice(bytes.as_slice())
        .map_err(|_| OmcpError::HandshakeMalformed("kem ek"))
}

fn dh(secret: &StaticSecret, public: &X25519Public) -> [u8; DHLEN] {
    secret.diffie_hellman(public).to_bytes()
}

/// Initiator state (knows responder static DH + KEM publics).
pub struct HybridInitiator {
    sym: SymmetricState,
    e: StaticSecret,
    s: StaticIdentity,
    rs: X25519Public,
    responder_kem: [u8; MLKEM768_EK_LEN],
    mlkem_ss: Option<[u8; MLKEM768_SS_LEN]>,
    msg1_sent: bool,
}

/// Responder state.
pub struct HybridResponder {
    sym: SymmetricState,
    s: StaticIdentity,
    e: Option<StaticSecret>,
    re: Option<X25519Public>,
    rs: Option<X25519Public>,
    mlkem_ss: Option<[u8; MLKEM768_SS_LEN]>,
    msg1_ok: bool,
}

impl HybridInitiator {
    /// Create initiator; `responder_dh` / `responder_kem` are the peer's static publics.
    pub fn new(
        static_id: StaticIdentity,
        responder_dh: &[u8; DHLEN],
        responder_kem: &[u8; MLKEM768_EK_LEN],
    ) -> Result<Self> {
        let _ = parse_kem_ek(responder_kem)?;
        let mut sym = SymmetricState::initialize(HYBRID_PROTOCOL_NAME);
        sym.mix_hash(responder_dh);
        Ok(Self {
            sym,
            e: StaticSecret::from(random_32()?),
            s: static_id,
            rs: X25519Public::from(*responder_dh),
            responder_kem: *responder_kem,
            mlkem_ss: None,
            msg1_sent: false,
        })
    }

    /// Write Noise IK message 1 + ML-KEM ciphertext.
    ///
    /// Layout: `e (32) || enc(s) (48) || enc(payload) || mlkem_ct (1088)`
    pub fn write_message_1(&mut self, payload: &[u8]) -> Result<Vec<u8>> {
        if self.msg1_sent {
            return Err(OmcpError::HandshakeState);
        }

        let e_pub = X25519Public::from(&self.e).to_bytes();
        self.sym.mix_hash(&e_pub);
        self.sym.mix_key(&dh(&self.e, &self.rs));

        let mut msg = Vec::new();
        msg.extend_from_slice(&e_pub);

        let s_pub = self.s.dh_public;
        let enc_s = self.sym.encrypt_and_hash(&s_pub)?;
        msg.extend_from_slice(&enc_s);

        self.sym.mix_key(&dh(&self.s.dh, &self.rs));

        let enc_payload = self.sym.encrypt_and_hash(payload)?;
        msg.extend_from_slice(&enc_payload);

        let ek = parse_kem_ek(&self.responder_kem)?;
        let (ct, ss) = ek.encapsulate();
        let mut mlkem_ss = [0u8; MLKEM768_SS_LEN];
        mlkem_ss.copy_from_slice(ss.as_slice());
        self.mlkem_ss = Some(mlkem_ss);

        let ct_bytes = ct.as_slice();
        debug_assert_eq!(ct_bytes.len(), MLKEM768_CT_LEN);
        self.sym.mix_hash(ct_bytes);
        msg.extend_from_slice(ct_bytes);

        self.msg1_sent = true;
        Ok(msg)
    }

    /// Read Noise IK message 2 and derive session keys.
    pub fn read_message_2(&mut self, message: &[u8]) -> Result<(Vec<u8>, HybridSessionKeys)> {
        if !self.msg1_sent || self.mlkem_ss.is_none() {
            return Err(OmcpError::HandshakeState);
        }
        // msg2: e (32) || enc(payload); empty payload => 32 + 16 (tag) = 48.
        const MSG2_MIN: usize = DHLEN + 16;
        if message.len() < MSG2_MIN {
            return Err(OmcpError::HandshakeMalformed("msg2 short"));
        }

        let (e_bytes, rest) = message.split_at(DHLEN);
        let mut e_arr = [0u8; DHLEN];
        e_arr.copy_from_slice(e_bytes);
        let re = X25519Public::from(e_arr);
        self.sym.mix_hash(&e_arr);
        self.sym.mix_key(&dh(&self.e, &re));
        self.sym.mix_key(&dh(&self.s.dh, &re));

        let payload = self.sym.decrypt_and_hash(rest)?;

        let mlkem_ss = self.mlkem_ss.take().ok_or(OmcpError::HandshakeState)?;
        let h = self.sym.handshake_hash();
        // take ownership of sym via split
        let sym = std::mem::replace(&mut self.sym, SymmetricState::initialize(b"consumed"));
        let (_k1, _k2, ck) = sym.split();
        let keys = derive_session(&ck, &mlkem_ss, &h)?;
        Ok((payload, keys))
    }
}

impl HybridResponder {
    /// Create responder with its static identity (DH public already known to initiator).
    pub fn new(static_id: StaticIdentity) -> Self {
        let mut sym = SymmetricState::initialize(HYBRID_PROTOCOL_NAME);
        sym.mix_hash(&static_id.dh_public);
        Self {
            sym,
            s: static_id,
            e: None,
            re: None,
            rs: None,
            mlkem_ss: None,
            msg1_ok: false,
        }
    }

    /// Read message 1; returns initiator payload (cleartext after AEAD).
    pub fn read_message_1(&mut self, message: &[u8]) -> Result<Vec<u8>> {
        if self.msg1_ok {
            return Err(OmcpError::HandshakeState);
        }

        let min = DHLEN + sealed_dh_len() + 16 + MLKEM768_CT_LEN; // e + enc(s) + enc(payload>=0) + ct
        if message.len() < min {
            return Err(OmcpError::HandshakeMalformed("msg1 short"));
        }

        let ct_start = message
            .len()
            .checked_sub(MLKEM768_CT_LEN)
            .ok_or(OmcpError::HandshakeMalformed("msg1 ct"))?;
        let (noise_part, ct_bytes) = message.split_at(ct_start);

        let (e_bytes, rest) = noise_part.split_at(DHLEN);
        let mut e_arr = [0u8; DHLEN];
        e_arr.copy_from_slice(e_bytes);
        let re = X25519Public::from(e_arr);
        self.sym.mix_hash(&e_arr);
        self.sym.mix_key(&dh(&self.s.dh, &re));

        if rest.len() < sealed_dh_len() {
            return Err(OmcpError::HandshakeMalformed("msg1 enc(s)"));
        }
        let (enc_s, enc_payload) = rest.split_at(sealed_dh_len());
        let s_bytes = self.sym.decrypt_and_hash(enc_s)?;
        if s_bytes.len() != DHLEN {
            return Err(OmcpError::HandshakeMalformed("initiator static len"));
        }
        let mut s_arr = [0u8; DHLEN];
        s_arr.copy_from_slice(&s_bytes);
        let rs = X25519Public::from(s_arr);
        self.sym.mix_key(&dh(&self.s.dh, &rs));

        let payload = self.sym.decrypt_and_hash(enc_payload)?;

        let ct: Ciphertext<MlKem768> = ct_bytes
            .try_into()
            .map_err(|_| OmcpError::HandshakeMalformed("kem ct length"))?;
        let ss = self.s.kem_dk.decapsulate(&ct);
        let mut mlkem_ss = [0u8; MLKEM768_SS_LEN];
        mlkem_ss.copy_from_slice(ss.as_slice());
        self.mlkem_ss = Some(mlkem_ss);
        self.sym.mix_hash(ct_bytes);

        self.re = Some(re);
        self.rs = Some(rs);
        self.e = Some(StaticSecret::from(random_32()?));
        self.msg1_ok = true;
        Ok(payload)
    }

    /// Write message 2 and derive session keys.
    pub fn write_message_2(&mut self, payload: &[u8]) -> Result<(Vec<u8>, HybridSessionKeys)> {
        if !self.msg1_ok {
            return Err(OmcpError::HandshakeState);
        }
        let e = self.e.as_ref().ok_or(OmcpError::HandshakeState)?;
        let re = self.re.ok_or(OmcpError::HandshakeState)?;
        let rs = self.rs.ok_or(OmcpError::HandshakeState)?;

        let e_pub = X25519Public::from(e).to_bytes();
        self.sym.mix_hash(&e_pub);
        self.sym.mix_key(&dh(e, &re));
        self.sym.mix_key(&dh(e, &rs));

        let enc_payload = self.sym.encrypt_and_hash(payload)?;
        let mut msg = Vec::with_capacity(DHLEN + enc_payload.len());
        msg.extend_from_slice(&e_pub);
        msg.extend_from_slice(&enc_payload);

        let mlkem_ss = self.mlkem_ss.take().ok_or(OmcpError::HandshakeState)?;
        let h = self.sym.handshake_hash();
        let sym = std::mem::replace(&mut self.sym, SymmetricState::initialize(b"consumed"));
        let (_k1, _k2, ck) = sym.split();
        let keys = derive_session(&ck, &mlkem_ss, &h)?;
        Ok((msg, keys))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hybrid_handshake_shared_secrets_match() {
        let responder_id = StaticIdentity::generate().unwrap();
        let initiator_id = StaticIdentity::generate().unwrap();

        let mut init = HybridInitiator::new(
            initiator_id,
            &responder_id.dh_public(),
            responder_id.kem_public(),
        )
        .unwrap();
        let mut resp = HybridResponder::new(responder_id);

        let msg1 = init.write_message_1(b"hello-responder").unwrap();
        let p1 = resp.read_message_1(&msg1).unwrap();
        assert_eq!(p1, b"hello-responder");

        let (msg2, keys_r) = resp.write_message_2(b"hello-initiator").unwrap();
        let (p2, keys_i) = init.read_message_2(&msg2).unwrap();
        assert_eq!(p2, b"hello-initiator");

        assert_eq!(keys_i.k_i2r, keys_r.k_i2r);
        assert_eq!(keys_i.k_r2i, keys_r.k_r2i);
        assert_eq!(keys_i.handshake_hash, keys_r.handshake_hash);
        assert_ne!(keys_i.k_i2r, [0u8; 32]);
        assert_ne!(keys_i.k_r2i, keys_i.k_i2r);
    }

    #[test]
    fn tampered_msg1_aead_fails() {
        let responder_id = StaticIdentity::generate().unwrap();
        let initiator_id = StaticIdentity::generate().unwrap();
        let mut init = HybridInitiator::new(
            initiator_id,
            &responder_id.dh_public(),
            responder_id.kem_public(),
        )
        .unwrap();
        let mut resp = HybridResponder::new(responder_id);

        let mut msg1 = init.write_message_1(b"x").unwrap();
        // Flip a byte inside encrypted static key region (after ephemeral pubkey).
        let idx = DHLEN + 3;
        msg1[idx] ^= 0x01;
        let err = resp.read_message_1(&msg1).unwrap_err();
        assert!(matches!(err, OmcpError::HandshakeAead));
    }

    #[test]
    fn tampered_msg2_aead_fails() {
        let responder_id = StaticIdentity::generate().unwrap();
        let initiator_id = StaticIdentity::generate().unwrap();
        let mut init = HybridInitiator::new(
            initiator_id,
            &responder_id.dh_public(),
            responder_id.kem_public(),
        )
        .unwrap();
        let mut resp = HybridResponder::new(responder_id);

        let msg1 = init.write_message_1(b"x").unwrap();
        resp.read_message_1(&msg1).unwrap();
        let (mut msg2, _) = resp.write_message_2(b"y").unwrap();
        let last = msg2.len() - 1;
        msg2[last] ^= 0xff;
        let err = match init.read_message_2(&msg2) {
            Err(e) => e,
            Ok(_) => panic!("expected AEAD failure"),
        };
        assert!(matches!(err, OmcpError::HandshakeAead));
    }

    #[test]
    fn tampered_mlkem_ct_breaks_transcript() {
        // ML-KEM ciphertext is mix_hash'd into the Noise transcript. Tampering it
        // lets msg1 classical AEAD succeed (CT is cleartext after the sealed
        // fields), but msg2 AEAD fails because handshake hashes diverge.
        let responder_id = StaticIdentity::generate().unwrap();
        let initiator_id = StaticIdentity::generate().unwrap();
        let mut init = HybridInitiator::new(
            initiator_id,
            &responder_id.dh_public(),
            responder_id.kem_public(),
        )
        .unwrap();
        let mut resp = HybridResponder::new(responder_id);

        let mut msg1 = init.write_message_1(b"x").unwrap();
        let last = msg1.len() - 1;
        msg1[last] ^= 0x5a;
        resp.read_message_1(&msg1).unwrap();
        let (msg2, _) = resp.write_message_2(b"y").unwrap();
        let err = match init.read_message_2(&msg2) {
            Err(e) => e,
            Ok(_) => panic!("expected AEAD failure from divergent transcript"),
        };
        assert!(matches!(err, OmcpError::HandshakeAead));
    }

    #[test]
    fn truncated_msg1_rejected() {
        let responder_id = StaticIdentity::generate().unwrap();
        let mut resp = HybridResponder::new(responder_id);
        let err = resp.read_message_1(&[0u8; 10]).unwrap_err();
        assert!(matches!(err, OmcpError::HandshakeMalformed(_)));
    }
}
