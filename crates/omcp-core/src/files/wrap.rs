//! Content Key generation and HPKE-style hybrid wrap (PROTOCOL §5.1 sealed / §7).
//!
//! Provisional M3 construction (documented, not RFC 9180 wire-compatible):
//! 1. Ephemeral X25519 × recipient static X25519 → `dh_ss`
//! 2. Encapsulate ML-KEM-768 to recipient EK → `(ct, kem_ss)`
//! 3. `aead_key = HKDF-SHA-384(salt=dh_ss, ikm=kem_ss, info="OMCP-File-HPKE-v1", L=32)`
//! 4. Seal Content Key with ChaCha20-Poly1305 (random 96-bit nonce; AAD = recipient id)

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use hkdf::Hkdf;
use ml_kem::kem::{Ciphertext, Decapsulate, Encapsulate, Kem, KeyExport, TryKeyInit};
use ml_kem::{EncapsulationKey, MlKem768};
use sha2::Sha384;
use x25519_dalek::{PublicKey as X25519Public, StaticSecret};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::error::{OmcpError, Result};
use crate::handshake::{MLKEM768_CT_LEN, MLKEM768_EK_LEN, MLKEM768_SS_LEN};

/// HKDF info for file Content Key wrap (M3 provisional).
pub const FILE_HPKE_HKDF_INFO: &[u8] = b"OMCP-File-HPKE-v1";

/// 256-bit Content Key (PROTOCOL §7).
#[derive(Clone, Zeroize, ZeroizeOnDrop, PartialEq, Eq)]
pub struct ContentKey(pub [u8; 32]);

impl ContentKey {
    /// Borrow the raw key bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Public half used by senders to wrap a Content Key.
#[derive(Clone)]
pub struct RecipientFileKeys {
    /// Stable recipient identifier (e.g. PeerID).
    pub recipient_id: [u8; 32],
    pub x25519_public: [u8; 32],
    pub mlkem_ek: [u8; MLKEM768_EK_LEN],
}

/// Recipient secret material for unwrapping Content Keys.
pub struct FileRecipientSecrets {
    pub recipient_id: [u8; 32],
    x25519: StaticSecret,
    x25519_public: [u8; 32],
    kem_dk: <MlKem768 as Kem>::DecapsulationKey,
    kem_ek: [u8; MLKEM768_EK_LEN],
}

impl FileRecipientSecrets {
    /// Generate a fresh recipient key set; `recipient_id` is caller-chosen.
    pub fn generate(recipient_id: [u8; 32]) -> Result<Self> {
        let x25519 = StaticSecret::from(random_32()?);
        let x25519_public = X25519Public::from(&x25519).to_bytes();
        let (kem_dk, kem_ek) = MlKem768::generate_keypair();
        let ek_encoded = kem_ek.to_bytes();
        let mut kem_ek_bytes = [0u8; MLKEM768_EK_LEN];
        kem_ek_bytes.copy_from_slice(ek_encoded.as_slice());
        Ok(Self {
            recipient_id,
            x25519,
            x25519_public,
            kem_dk,
            kem_ek: kem_ek_bytes,
        })
    }

    /// Public keys for senders.
    pub fn public_keys(&self) -> RecipientFileKeys {
        RecipientFileKeys {
            recipient_id: self.recipient_id,
            x25519_public: self.x25519_public,
            mlkem_ek: self.kem_ek,
        }
    }

    /// Unwrap a Content Key sealed to this recipient.
    pub fn open_content_key(&self, wrapped: &WrappedContentKey) -> Result<ContentKey> {
        open_content_key(wrapped, self)
    }
}

/// Wrapped Content Key for one recipient.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WrappedContentKey {
    pub recipient_id: [u8; 32],
    pub eph_x25519_pk: [u8; 32],
    pub mlkem_ct: [u8; MLKEM768_CT_LEN],
    pub nonce: [u8; 12],
    /// AEAD ciphertext of the 32-byte Content Key (32 + 16 tag).
    pub ciphertext: Vec<u8>,
}

impl WrappedContentKey {
    /// Encode to a compact binary blob (M3 provisional).
    pub fn encode(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(32 + 32 + MLKEM768_CT_LEN + 12 + 2 + self.ciphertext.len());
        v.extend_from_slice(&self.recipient_id);
        v.extend_from_slice(&self.eph_x25519_pk);
        v.extend_from_slice(&self.mlkem_ct);
        v.extend_from_slice(&self.nonce);
        v.extend_from_slice(&(self.ciphertext.len() as u16).to_be_bytes());
        v.extend_from_slice(&self.ciphertext);
        v
    }

    /// Decode from [`Self::encode`].
    pub fn decode(mut buf: &[u8]) -> Result<Self> {
        let need = 32 + 32 + MLKEM768_CT_LEN + 12 + 2;
        if buf.len() < need {
            return Err(OmcpError::Truncated {
                need,
                have: buf.len(),
            });
        }
        let mut recipient_id = [0u8; 32];
        recipient_id.copy_from_slice(&buf[..32]);
        buf = &buf[32..];
        let mut eph_x25519_pk = [0u8; 32];
        eph_x25519_pk.copy_from_slice(&buf[..32]);
        buf = &buf[32..];
        let mut mlkem_ct = [0u8; MLKEM768_CT_LEN];
        mlkem_ct.copy_from_slice(&buf[..MLKEM768_CT_LEN]);
        buf = &buf[MLKEM768_CT_LEN..];
        let mut nonce = [0u8; 12];
        nonce.copy_from_slice(&buf[..12]);
        buf = &buf[12..];
        let ct_len = u16::from_be_bytes([buf[0], buf[1]]) as usize;
        buf = &buf[2..];
        if buf.len() < ct_len {
            return Err(OmcpError::Truncated {
                need: ct_len,
                have: buf.len(),
            });
        }
        Ok(Self {
            recipient_id,
            eph_x25519_pk,
            mlkem_ct,
            nonce,
            ciphertext: buf[..ct_len].to_vec(),
        })
    }
}

/// Generate a fresh random Content Key.
pub fn generate_content_key() -> Result<ContentKey> {
    let mut k = [0u8; 32];
    getrandom::getrandom(&mut k).map_err(|_| OmcpError::Crypto("getrandom"))?;
    Ok(ContentKey(k))
}

fn random_32() -> Result<[u8; 32]> {
    let mut b = [0u8; 32];
    getrandom::getrandom(&mut b).map_err(|_| OmcpError::Crypto("getrandom"))?;
    Ok(b)
}

fn derive_wrap_key(dh_ss: &[u8; 32], kem_ss: &[u8; MLKEM768_SS_LEN]) -> Result<[u8; 32]> {
    let hk = Hkdf::<Sha384>::new(Some(dh_ss.as_slice()), kem_ss);
    let mut out = [0u8; 32];
    hk.expand(FILE_HPKE_HKDF_INFO, &mut out)
        .map_err(|_| OmcpError::Crypto("file hpke hkdf"))?;
    Ok(out)
}

/// Seal `content_key` to `recipient`.
pub fn seal_content_key(
    content_key: &ContentKey,
    recipient: &RecipientFileKeys,
) -> Result<WrappedContentKey> {
    let eph = StaticSecret::from(random_32()?);
    let eph_pk = X25519Public::from(&eph).to_bytes();
    let their_pk = X25519Public::from(recipient.x25519_public);
    let mut dh_ss = eph.diffie_hellman(&their_pk).to_bytes();

    let ek = EncapsulationKey::<MlKem768>::new_from_slice(recipient.mlkem_ek.as_slice())
        .map_err(|_| OmcpError::File("mlkem ek"))?;
    let (ct, ss) = ek.encapsulate();
    let mut kem_ss = [0u8; MLKEM768_SS_LEN];
    kem_ss.copy_from_slice(ss.as_slice());
    let mut mlkem_ct = [0u8; MLKEM768_CT_LEN];
    mlkem_ct.copy_from_slice(ct.as_slice());

    let aead_key = derive_wrap_key(&dh_ss, &kem_ss)?;
    dh_ss.zeroize();
    kem_ss.zeroize();

    let mut nonce_bytes = [0u8; 12];
    getrandom::getrandom(&mut nonce_bytes).map_err(|_| OmcpError::Crypto("getrandom"))?;
    let nonce = Nonce::from(nonce_bytes);
    let cipher = ChaCha20Poly1305::new_from_slice(&aead_key)
        .map_err(|_| OmcpError::Crypto("chacha key"))?;
    let ciphertext = cipher
        .encrypt(
            &nonce,
            Payload {
                msg: content_key.as_bytes(),
                aad: &recipient.recipient_id,
            },
        )
        .map_err(|_| OmcpError::Crypto("wrap seal"))?;

    Ok(WrappedContentKey {
        recipient_id: recipient.recipient_id,
        eph_x25519_pk: eph_pk,
        mlkem_ct,
        nonce: nonce_bytes,
        ciphertext,
    })
}

fn open_content_key(
    wrapped: &WrappedContentKey,
    secrets: &FileRecipientSecrets,
) -> Result<ContentKey> {
    if wrapped.recipient_id != secrets.recipient_id {
        return Err(OmcpError::File("recipient id mismatch"));
    }
    let eph_pk = X25519Public::from(wrapped.eph_x25519_pk);
    let mut dh_ss = secrets.x25519.diffie_hellman(&eph_pk).to_bytes();

    let ct: Ciphertext<MlKem768> = wrapped
        .mlkem_ct
        .as_slice()
        .try_into()
        .map_err(|_| OmcpError::File("mlkem ct"))?;
    let ss = secrets.kem_dk.decapsulate(&ct);
    let mut kem_ss = [0u8; MLKEM768_SS_LEN];
    kem_ss.copy_from_slice(ss.as_slice());

    let aead_key = derive_wrap_key(&dh_ss, &kem_ss)?;
    dh_ss.zeroize();
    kem_ss.zeroize();

    let cipher = ChaCha20Poly1305::new_from_slice(&aead_key)
        .map_err(|_| OmcpError::Crypto("chacha key"))?;
    let nonce = Nonce::from(wrapped.nonce);
    let plain = cipher
        .decrypt(
            &nonce,
            Payload {
                msg: &wrapped.ciphertext,
                aad: &secrets.recipient_id,
            },
        )
        .map_err(|_| OmcpError::File("wrap open"))?;
    if plain.len() != 32 {
        return Err(OmcpError::File("content key length"));
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&plain);
    Ok(ContentKey(key))
}

/// Public alias used by `FileRecipientSecrets::open_content_key` callers that
/// already hold secrets — prefer the method on [`FileRecipientSecrets`].

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_key_wrap_roundtrip() {
        let mut rid = [0u8; 32];
        rid[0] = 7;
        let secrets = FileRecipientSecrets::generate(rid).unwrap();
        let ck = generate_content_key().unwrap();
        let wrapped = seal_content_key(&ck, &secrets.public_keys()).unwrap();
        let opened = secrets.open_content_key(&wrapped).unwrap();
        assert_eq!(opened.as_bytes(), ck.as_bytes());
        let enc = wrapped.encode();
        let dec = WrappedContentKey::decode(&enc).unwrap();
        assert_eq!(dec, wrapped);
    }
}
