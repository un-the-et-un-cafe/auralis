//! Device keys, PeerID and the restore indicator (REQUIREMENTS "Restore indicator
//! (security)" RS1, "Re-verification after restore" RS2, "Implementation
//! clarifications (v0.1.4a)"; PROTOCOL §3.1, §3.4).
//!
//! * **Identity signing key** (Ed25519) is derived from the 24-word master seed:
//!   `HKDF-SHA-384(salt = "OMCP-Identity-Seed-v1", ikm = seed, info = "ed25519")`.
//!   It survives a restore.
//! * **PeerID** = BLAKE2b-256(identity signing public key) ([`peer_id_from_identity`]).
//!   Contacts are keyed by PeerID.
//! * Every installation holds a random Ed25519 **device key** and its own
//!   **X25519 Noise static key** (per device). Both are published in a
//!   [`DeviceCert`] signed by the identity key. A restore always mints new ones
//!   with `restored_at`.
//! * **Device binding (c):** in every handshake the device key signs the Noise
//!   handshake hash ([`OwnDevice::handshake_payload`]). The peer checks the
//!   certificate, that its X25519 key is the Noise remote static, and the
//!   signature ([`verify_handshake_payload`]). A missing certificate or a
//!   superseded device key fails the handshake, so an old certificate can't be
//!   replayed to suppress the restore notice.

use blake2::digest::consts::U32;
use blake2::{Blake2b, Digest};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use hkdf::Hkdf;
use serde::{Deserialize, Serialize};
use sha2::Sha384;
use zeroize::Zeroizing;

use crate::error::{OmcpError, Result};
use crate::handshake::StaticIdentity;

/// Wire version of [`DeviceCert`] (v2 adds the X25519 key and PeerID = BLAKE2b).
pub const DEVICE_CERT_VERSION: u8 = 2;
/// `ver | peer_id 32 | identity_sign 32 | device 32 | dh 32 | created_ms u64 BE | restored_at_ms u64 BE (0 = none) | sig 64`.
pub const DEVICE_CERT_LEN: usize = 1 + 32 + 32 + 32 + 32 + 8 + 8 + 64;
/// The "Restored" badge stays on the contact for 7 days.
pub const RESTORED_BADGE_MS: u64 = 7 * 24 * 3600 * 1000;
/// Prefix of the Noise handshake payload (unchanged from v0.1).
pub const HANDSHAKE_PAYLOAD_PREFIX: &[u8] = b"omcp-ffi-v1";
/// Handshake payload extension tag: a [`DeviceCert`] follows.
pub const HANDSHAKE_EXT_DEVICE_CERT: u8 = 0x01;
/// Handshake payload extension tag: device-key signature over the handshake hash.
pub const HANDSHAKE_EXT_DEVICE_BIND: u8 = 0x02;

const SIG_CONTEXT: &[u8] = b"OMCP-DeviceCert-v2";
const BIND_CONTEXT: &[u8] = b"OMCP-DeviceBind-v1";

/// The identity signing key (Ed25519), deterministic from the master seed.
pub fn identity_signing_key(seed: &[u8; 32]) -> Result<SigningKey> {
    let hk = Hkdf::<Sha384>::new(Some(b"OMCP-Identity-Seed-v1"), seed);
    let mut sk = Zeroizing::new([0u8; 32]);
    hk.expand(b"ed25519", sk.as_mut())
        .map_err(|_| OmcpError::Crypto("hkdf ed25519"))?;
    Ok(SigningKey::from_bytes(&sk))
}

/// PeerID = BLAKE2b-256 of the Ed25519 identity public key (PROTOCOL §3.1).
pub fn peer_id_from_identity(identity_sign: &[u8; 32]) -> [u8; 32] {
    let mut h = Blake2b::<U32>::new();
    h.update(identity_sign);
    h.finalize().into()
}

/// PeerID of the identity behind a master seed.
pub fn peer_id_from_seed(seed: &[u8; 32]) -> Result<[u8; 32]> {
    Ok(peer_id_from_identity(
        &identity_signing_key(seed)?.verifying_key().to_bytes(),
    ))
}

/// The pre-v0.1.4a X25519 static key, derived from the master seed. Installs that
/// existed before per-device keys keep it as their device X25519 key so contacts
/// that pinned it can still reach them (legacy contact ids were this key).
pub fn legacy_dh_public(seed: &[u8; 32]) -> Result<[u8; 32]> {
    Ok(StaticIdentity::from_seed(seed)?.dh_public())
}

/// A device certificate: "this device key and X25519 key belong to this identity".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceCert {
    /// BLAKE2b-256(identity_sign); checked on decode.
    pub peer_id: [u8; 32],
    pub identity_sign: [u8; 32],
    /// Ed25519 device key (signs the handshake hash).
    pub device: [u8; 32],
    /// X25519 Noise static key of this device.
    pub dh: [u8; 32],
    pub created_ms: u64,
    /// Set when the device was created by a restore from the 24 words.
    pub restored_at_ms: Option<u64>,
    pub sig: [u8; 64],
}

impl DeviceCert {
    fn tbs(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(SIG_CONTEXT.len() + DEVICE_CERT_LEN - 64);
        v.extend_from_slice(SIG_CONTEXT);
        v.push(DEVICE_CERT_VERSION);
        v.extend_from_slice(&self.peer_id);
        v.extend_from_slice(&self.identity_sign);
        v.extend_from_slice(&self.device);
        v.extend_from_slice(&self.dh);
        v.extend_from_slice(&self.created_ms.to_be_bytes());
        v.extend_from_slice(&self.restored_at_ms.unwrap_or(0).to_be_bytes());
        v
    }

    /// Issue a certificate for `device` + `dh` signed by the identity key from `seed`.
    pub fn issue(
        seed: &[u8; 32],
        device: [u8; 32],
        dh: [u8; 32],
        created_ms: u64,
        restored_at_ms: Option<u64>,
    ) -> Result<Self> {
        let id = identity_signing_key(seed)?;
        let identity_sign = id.verifying_key().to_bytes();
        let mut c = Self {
            peer_id: peer_id_from_identity(&identity_sign),
            identity_sign,
            device,
            dh,
            created_ms,
            restored_at_ms: restored_at_ms.filter(|t| *t != 0),
            sig: [0; 64],
        };
        c.sig = id.sign(&c.tbs()).to_bytes();
        Ok(c)
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut v = self.tbs()[SIG_CONTEXT.len()..].to_vec();
        v.extend_from_slice(&self.sig);
        v
    }

    /// Decode and verify the PeerID and the identity signature.
    pub fn decode(b: &[u8]) -> Result<Self> {
        if b.len() != DEVICE_CERT_LEN || b[0] != DEVICE_CERT_VERSION {
            return Err(OmcpError::Crypto("device cert format"));
        }
        let a32 = |o: usize| -> [u8; 32] { b[o..o + 32].try_into().expect("len") };
        let u64at = |o: usize| u64::from_be_bytes(b[o..o + 8].try_into().expect("len"));
        let c = Self {
            peer_id: a32(1),
            identity_sign: a32(33),
            device: a32(65),
            dh: a32(97),
            created_ms: u64at(129),
            restored_at_ms: Some(u64at(137)).filter(|t| *t != 0),
            sig: b[145..209].try_into().expect("len"),
        };
        if peer_id_from_identity(&c.identity_sign) != c.peer_id {
            return Err(OmcpError::Crypto("device cert peer id"));
        }
        let vk = VerifyingKey::from_bytes(&c.identity_sign)
            .map_err(|_| OmcpError::Crypto("device cert identity key"))?;
        vk.verify(&c.tbs(), &Signature::from_bytes(&c.sig))
            .map_err(|_| OmcpError::Crypto("device cert signature"))?;
        Ok(c)
    }
}

/// This installation's device keys (kept sealed in the local store).
#[derive(Clone, Serialize, Deserialize)]
pub struct OwnDevice {
    secret: Vec<u8>,
    cert: Vec<u8>,
    /// Seed of this device's Noise static identity (X25519 + ML-KEM).
    #[serde(default)]
    dh_seed: Vec<u8>,
}

impl Drop for OwnDevice {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.secret.zeroize();
        self.dh_seed.zeroize();
    }
}

impl OwnDevice {
    /// Mint fresh device keys (random device key and X25519). `restored_at_ms` marks a restore.
    pub fn generate(seed: &[u8; 32], now_ms: u64, restored_at_ms: Option<u64>) -> Result<Self> {
        let mut dh_seed = Zeroizing::new([0u8; 32]);
        getrandom::getrandom(dh_seed.as_mut()).map_err(|_| OmcpError::Crypto("getrandom"))?;
        Self::generate_with_dh_seed(seed, &dh_seed, now_ms, restored_at_ms)
    }

    /// Like [`Self::generate`] with a given X25519 seed. Migrated pre-v0.1.4a
    /// installs pass the master seed so their X25519 key stays the same.
    pub fn generate_with_dh_seed(
        seed: &[u8; 32],
        dh_seed: &[u8; 32],
        now_ms: u64,
        restored_at_ms: Option<u64>,
    ) -> Result<Self> {
        let mut sk = Zeroizing::new([0u8; 32]);
        getrandom::getrandom(sk.as_mut()).map_err(|_| OmcpError::Crypto("getrandom"))?;
        let dev = SigningKey::from_bytes(&sk);
        let dh = StaticIdentity::from_seed(dh_seed)?.dh_public();
        let cert = DeviceCert::issue(
            seed,
            dev.verifying_key().to_bytes(),
            dh,
            now_ms,
            restored_at_ms,
        )?;
        Ok(Self {
            secret: sk.to_vec(),
            cert: cert.encode(),
            dh_seed: dh_seed.to_vec(),
        })
    }

    pub fn cert(&self) -> Result<DeviceCert> {
        DeviceCert::decode(&self.cert)
    }

    /// Ed25519 device public key.
    pub fn public(&self) -> Result<[u8; 32]> {
        Ok(self.cert()?.device)
    }

    /// This device's Noise static identity (X25519 + ML-KEM).
    pub fn static_identity(&self) -> Result<StaticIdentity> {
        let s: [u8; 32] = self
            .dh_seed
            .as_slice()
            .try_into()
            .map_err(|_| OmcpError::Crypto("device dh seed"))?;
        let s = Zeroizing::new(s);
        StaticIdentity::from_seed(&s)
    }

    /// True when the stored keys are complete and belong to `seed`'s identity.
    pub fn valid_for(&self, peer_id: &[u8; 32]) -> bool {
        self.cert().is_ok_and(|c| &c.peer_id == peer_id)
            && self
                .static_identity()
                .is_ok_and(|s| self.cert().is_ok_and(|c| c.dh == s.dh_public()))
    }

    /// Noise handshake payload: certificate + device-key signature over `h`,
    /// the handshake hash at the point the payload is encrypted.
    pub fn handshake_payload(&self, h: &[u8]) -> Result<Vec<u8>> {
        let sk: [u8; 32] = self
            .secret
            .as_slice()
            .try_into()
            .map_err(|_| OmcpError::Crypto("device key"))?;
        let sk = SigningKey::from_bytes(&Zeroizing::new(sk));
        let mut v = HANDSHAKE_PAYLOAD_PREFIX.to_vec();
        v.push(HANDSHAKE_EXT_DEVICE_CERT);
        v.extend_from_slice(&self.cert);
        v.push(HANDSHAKE_EXT_DEVICE_BIND);
        v.extend_from_slice(&sk.sign(&bind_tbs(h)).to_bytes());
        Ok(v)
    }
}

fn bind_tbs(h: &[u8]) -> Vec<u8> {
    let mut m = BIND_CONTEXT.to_vec();
    m.extend_from_slice(h);
    m
}

/// Verify a peer's handshake payload against the handshake hash `h` and the
/// Noise remote static key. A missing certificate or signature, a certificate
/// for another X25519 key, or a bad signature fails.
pub fn verify_handshake_payload(
    p: &[u8],
    h: &[u8],
    remote_static: &[u8; 32],
) -> Result<DeviceCert> {
    let rest = p
        .strip_prefix(HANDSHAKE_PAYLOAD_PREFIX)
        .ok_or(OmcpError::Crypto("handshake payload"))?;
    let rest = match rest.split_first() {
        Some((&HANDSHAKE_EXT_DEVICE_CERT, r)) if r.len() >= DEVICE_CERT_LEN => r,
        _ => return Err(OmcpError::Crypto("missing device certificate")),
    };
    let (cert_b, rest) = rest.split_at(DEVICE_CERT_LEN);
    let cert = DeviceCert::decode(cert_b)?;
    if &cert.dh != remote_static {
        return Err(OmcpError::Crypto("device cert not for this session key"));
    }
    let sig = match rest.split_first() {
        Some((&HANDSHAKE_EXT_DEVICE_BIND, s)) if s.len() >= 64 => &s[..64],
        _ => return Err(OmcpError::Crypto("missing device binding")),
    };
    let vk = VerifyingKey::from_bytes(&cert.device).map_err(|_| OmcpError::Crypto("device key"))?;
    let sig: [u8; 64] = sig.try_into().expect("len");
    vk.verify(&bind_tbs(h), &Signature::from_bytes(&sig))
        .map_err(|_| OmcpError::Crypto("device binding signature"))?;
    Ok(cert)
}

/// What we know about a contact's devices (stored with the contact).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContactDevice {
    /// Pinned identity signing key (first certificate seen over the
    /// authenticated Noise channel; it never changes, even after a restore).
    pub identity_sign: Option<Vec<u8>>,
    /// Current device key.
    pub current: Option<Vec<u8>>,
    /// Device keys replaced by a newer one (never accepted again).
    pub superseded: Vec<Vec<u8>>,
    pub restored_at_ms: Option<u64>,
    /// The "Restored" badge shows until this time.
    pub badge_until_ms: Option<u64>,
    /// Set until the user verifies the new safety number by QR.
    pub restored_unverified: bool,
}

/// Outcome of [`on_peer_cert`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceEvent {
    /// Nothing changed.
    Unchanged,
    /// First certificate from this contact: pinned silently.
    Pinned,
    /// The contact is on a new device (restore notice; verification cleared).
    Changed {
        restored_at_ms: Option<u64>,
        old_device: Option<[u8; 32]>,
    },
}

impl DeviceEvent {
    /// True when the in-chat notice, badge and verification reset apply.
    pub fn is_notice(&self) -> bool {
        matches!(self, DeviceEvent::Changed { .. })
    }
}

/// Apply a peer's verified handshake certificate to their state. A superseded
/// device key or a different identity is an error (the handshake fails).
pub fn on_peer_cert(
    st: &mut ContactDevice,
    peer_id: &[u8; 32],
    c: &DeviceCert,
    now_ms: u64,
) -> Result<DeviceEvent> {
    if &c.peer_id != peer_id {
        return Err(OmcpError::Crypto("device cert for another peer"));
    }
    match &st.identity_sign {
        Some(k) if k.as_slice() != c.identity_sign => {
            return Err(OmcpError::Crypto("device cert identity key changed"))
        }
        Some(_) => {}
        None => st.identity_sign = Some(c.identity_sign.to_vec()),
    }
    if st.current.as_deref() == Some(c.device.as_slice()) {
        return Ok(DeviceEvent::Unchanged);
    }
    if st.superseded.iter().any(|d| d.as_slice() == c.device) {
        return Err(OmcpError::Crypto("superseded device key"));
    }
    let first = st.current.is_none() && st.superseded.is_empty();
    let old = st.current.replace(c.device.to_vec());
    if first && c.restored_at_ms.is_none() {
        return Ok(DeviceEvent::Pinned);
    }
    if let Some(o) = &old {
        st.superseded.push(o.clone());
    }
    st.restored_at_ms = c.restored_at_ms;
    st.badge_until_ms = Some(c.restored_at_ms.unwrap_or(now_ms).min(now_ms) + RESTORED_BADGE_MS);
    st.restored_unverified = true;
    Ok(DeviceEvent::Changed {
        restored_at_ms: c.restored_at_ms,
        old_device: old.and_then(|o| o.try_into().ok()),
    })
}

impl ContactDevice {
    /// "Restored" badge visible at `now_ms`.
    pub fn badge_active(&self, now_ms: u64) -> bool {
        self.badge_until_ms.is_some_and(|t| now_ms < t)
    }

    pub fn current_key(&self) -> Option<[u8; 32]> {
        self.current
            .as_ref()
            .and_then(|d| d.as_slice().try_into().ok())
    }

    pub fn is_superseded(&self, device: &[u8; 32]) -> bool {
        self.superseded.iter().any(|d| d.as_slice() == device)
    }
}

/// Short display fingerprint of a device key.
pub fn device_fingerprint(device: &[u8; 32]) -> String {
    blake3::hash(device).as_bytes()[..8]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEED: [u8; 32] = [3; 32];

    fn peer() -> [u8; 32] {
        peer_id_from_seed(&SEED).unwrap()
    }

    #[test]
    fn peer_id_is_blake2b_of_identity_key() {
        let vk = identity_signing_key(&SEED)
            .unwrap()
            .verifying_key()
            .to_bytes();
        let mut h = Blake2b::<U32>::new();
        h.update(vk);
        let want: [u8; 32] = h.finalize().into();
        assert_eq!(peer(), want);
        assert_ne!(peer(), legacy_dh_public(&SEED).unwrap());
    }

    #[test]
    fn cert_roundtrip_and_tamper() {
        let d = OwnDevice::generate(&SEED, 10, Some(10)).unwrap();
        let c = d.cert().unwrap();
        assert_eq!(c.restored_at_ms, Some(10));
        assert_eq!(c.peer_id, peer());
        assert_eq!(c.dh, d.static_identity().unwrap().dh_public());
        let mut b = c.encode();
        assert_eq!(b.len(), DEVICE_CERT_LEN);
        assert_eq!(DeviceCert::decode(&b).unwrap(), c);
        b[70] ^= 1;
        assert!(DeviceCert::decode(&b).is_err());
        assert!(d.valid_for(&peer()));
    }

    #[test]
    fn binding_signature_over_handshake_hash() {
        let d = OwnDevice::generate(&SEED, 1, None).unwrap();
        let dh = d.cert().unwrap().dh;
        let p = d.handshake_payload(b"hash-1").unwrap();
        assert_eq!(
            verify_handshake_payload(&p, b"hash-1", &dh).unwrap(),
            d.cert().unwrap()
        );
        // Replayed into another session (other hash): rejected.
        assert!(verify_handshake_payload(&p, b"hash-2", &dh).is_err());
        // Wrong Noise static: rejected.
        assert!(verify_handshake_payload(&p, b"hash-1", &[7; 32]).is_err());
        // Missing certificate / binding: rejected.
        assert!(verify_handshake_payload(b"omcp-ffi-v1", b"hash-1", &dh).is_err());
        let cut = &p[..p.len() - 65];
        assert!(verify_handshake_payload(cut, b"hash-1", &dh).is_err());
    }

    #[test]
    fn legacy_install_keeps_its_x25519_key() {
        let d = OwnDevice::generate_with_dh_seed(&SEED, &SEED, 1, None).unwrap();
        assert_eq!(d.cert().unwrap().dh, legacy_dh_public(&SEED).unwrap());
    }

    #[test]
    fn identity_key_survives_restore_device_keys_do_not() {
        let a = OwnDevice::generate(&SEED, 1, None).unwrap().cert().unwrap();
        let b = OwnDevice::generate(&SEED, 2, Some(2))
            .unwrap()
            .cert()
            .unwrap();
        assert_eq!(a.identity_sign, b.identity_sign);
        assert_eq!(a.peer_id, b.peer_id);
        assert_ne!(a.device, b.device);
        assert_ne!(a.dh, b.dh);
    }

    #[test]
    fn rs1_restore_raises_notice_and_supersedes_old_key() {
        let p = peer();
        let mut st = ContactDevice::default();
        let a = OwnDevice::generate(&SEED, 1, None).unwrap().cert().unwrap();
        assert_eq!(
            on_peer_cert(&mut st, &p, &a, 5).unwrap(),
            DeviceEvent::Pinned
        );
        assert_eq!(
            on_peer_cert(&mut st, &p, &a, 6).unwrap(),
            DeviceEvent::Unchanged
        );
        let b = OwnDevice::generate(&SEED, 100, Some(100))
            .unwrap()
            .cert()
            .unwrap();
        let ev = on_peer_cert(&mut st, &p, &b, 200).unwrap();
        assert!(ev.is_notice());
        assert!(st.is_superseded(&a.device));
        assert!(st.badge_active(200) && !st.badge_active(200 + RESTORED_BADGE_MS));
        // The old device key is never accepted again.
        assert!(on_peer_cert(&mut st, &p, &a, 300).is_err());
        // Another identity can't impersonate.
        let evil = OwnDevice::generate(&[4; 32], 1, None)
            .unwrap()
            .cert()
            .unwrap();
        assert!(on_peer_cert(&mut st, &p, &evil, 300).is_err());
    }

    #[test]
    fn rs1_notice_cannot_be_suppressed() {
        let p = peer();
        let a = OwnDevice::generate(&SEED, 1, None).unwrap().cert().unwrap();
        // A new device without `restored_at` still counts as a change.
        let mut st = ContactDevice::default();
        on_peer_cert(&mut st, &p, &a, 1).unwrap();
        let b = OwnDevice::generate(&SEED, 3, None).unwrap().cert().unwrap();
        assert!(on_peer_cert(&mut st, &p, &b, 3).unwrap().is_notice());
        // A restored device seen for the first time also shows it.
        let mut st = ContactDevice::default();
        let r = OwnDevice::generate(&SEED, 3, Some(3))
            .unwrap()
            .cert()
            .unwrap();
        assert!(on_peer_cert(&mut st, &p, &r, 3).unwrap().is_notice());
    }
}
