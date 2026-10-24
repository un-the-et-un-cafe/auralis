//! Device keys and the restore indicator (REQUIREMENTS "Restore indicator
//! (security)" RS1, "Re-verification after restore" RS2; PROTOCOL §3.2, §3.4).
//!
//! Every installation holds a random Ed25519 **device key**. Its public half is
//! published in a [`DeviceCert`] signed by the **identity signing key**, which is
//! derived from the 24-word master seed (so it survives a restore):
//! `HKDF-SHA-384(salt = "OMCP-Identity-Seed-v1", ikm = seed, info = "ed25519")`.
//! A restore from the 24 words always mints a *new* device key whose certificate
//! carries `restored_at`.
//!
//! The certificate travels in the Noise handshake payload of every link
//! ([`handshake_payload`]), so a contact learns about the new device on the very
//! first connection after the restore. The contact's own device decides whether
//! to show the notice ([`on_peer_cert`]); nothing the restored device sends (or
//! omits) can suppress it: a different device key, a `restored_at`, or a missing
//! certificate after one was pinned all raise it, clear "Verified" and mark the
//! old device key superseded.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use hkdf::Hkdf;
use serde::{Deserialize, Serialize};
use sha2::Sha384;
use zeroize::Zeroizing;

use crate::error::{OmcpError, Result};

/// Wire version of [`DeviceCert`].
pub const DEVICE_CERT_VERSION: u8 = 1;
/// `ver | peer_id 32 | identity_sign 32 | device 32 | created_ms u64 BE | restored_at_ms u64 BE (0 = none) | sig 64`.
pub const DEVICE_CERT_LEN: usize = 1 + 32 + 32 + 32 + 8 + 8 + 64;
/// The "Restored" badge stays on the contact for 7 days.
pub const RESTORED_BADGE_MS: u64 = 7 * 24 * 3600 * 1000;
/// Prefix of the Noise handshake payload (unchanged from v0.1).
pub const HANDSHAKE_PAYLOAD_PREFIX: &[u8] = b"omcp-ffi-v1";
/// Handshake payload extension tag: a [`DeviceCert`] follows.
pub const HANDSHAKE_EXT_DEVICE_CERT: u8 = 0x01;

const SIG_CONTEXT: &[u8] = b"OMCP-DeviceCert-v1";

/// The identity signing key (Ed25519), deterministic from the master seed.
pub fn identity_signing_key(seed: &[u8; 32]) -> Result<SigningKey> {
    let hk = Hkdf::<Sha384>::new(Some(b"OMCP-Identity-Seed-v1"), seed);
    let mut sk = Zeroizing::new([0u8; 32]);
    hk.expand(b"ed25519", sk.as_mut())
        .map_err(|_| OmcpError::Crypto("hkdf ed25519"))?;
    Ok(SigningKey::from_bytes(&sk))
}

/// A device certificate: "this device key belongs to this identity".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceCert {
    /// The X25519 static key the contact knows us by (PeerID in the reference).
    pub peer_id: [u8; 32],
    pub identity_sign: [u8; 32],
    pub device: [u8; 32],
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
        v.extend_from_slice(&self.created_ms.to_be_bytes());
        v.extend_from_slice(&self.restored_at_ms.unwrap_or(0).to_be_bytes());
        v
    }

    /// Issue a certificate for `device` signed by the identity key from `seed`.
    pub fn issue(
        seed: &[u8; 32],
        peer_id: [u8; 32],
        device: [u8; 32],
        created_ms: u64,
        restored_at_ms: Option<u64>,
    ) -> Result<Self> {
        let id = identity_signing_key(seed)?;
        let mut c = Self {
            peer_id,
            identity_sign: id.verifying_key().to_bytes(),
            device,
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

    /// Decode and verify the identity signature.
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
            created_ms: u64at(97),
            restored_at_ms: Some(u64at(105)).filter(|t| *t != 0),
            sig: b[113..177].try_into().expect("len"),
        };
        let vk = VerifyingKey::from_bytes(&c.identity_sign)
            .map_err(|_| OmcpError::Crypto("device cert identity key"))?;
        vk.verify(&c.tbs(), &Signature::from_bytes(&c.sig))
            .map_err(|_| OmcpError::Crypto("device cert signature"))?;
        Ok(c)
    }
}

/// This installation's device key (kept sealed in the local store).
#[derive(Clone, Serialize, Deserialize)]
pub struct OwnDevice {
    secret: Vec<u8>,
    cert: Vec<u8>,
}

impl Drop for OwnDevice {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.secret.zeroize();
    }
}

impl OwnDevice {
    /// Mint a fresh device key (and certificate). `restored_at_ms` marks a restore.
    pub fn generate(
        seed: &[u8; 32],
        peer_id: [u8; 32],
        now_ms: u64,
        restored_at_ms: Option<u64>,
    ) -> Result<Self> {
        let mut sk = Zeroizing::new([0u8; 32]);
        getrandom::getrandom(sk.as_mut()).map_err(|_| OmcpError::Crypto("getrandom"))?;
        let dev = SigningKey::from_bytes(&sk);
        let cert = DeviceCert::issue(
            seed,
            peer_id,
            dev.verifying_key().to_bytes(),
            now_ms,
            restored_at_ms,
        )?;
        Ok(Self {
            secret: sk.to_vec(),
            cert: cert.encode(),
        })
    }

    pub fn cert(&self) -> Result<DeviceCert> {
        DeviceCert::decode(&self.cert)
    }

    pub fn public(&self) -> Result<[u8; 32]> {
        Ok(self.cert()?.device)
    }
}

/// Noise handshake payload carrying our device certificate.
pub fn handshake_payload(cert: &DeviceCert) -> Vec<u8> {
    let mut v = HANDSHAKE_PAYLOAD_PREFIX.to_vec();
    v.push(HANDSHAKE_EXT_DEVICE_CERT);
    v.extend_from_slice(&cert.encode());
    v
}

/// Parse a peer's handshake payload. `Ok(None)` = an older peer without a cert.
pub fn parse_handshake_payload(p: &[u8]) -> Result<Option<DeviceCert>> {
    let Some(rest) = p.strip_prefix(HANDSHAKE_PAYLOAD_PREFIX) else {
        return Ok(None);
    };
    match rest.split_first() {
        None => Ok(None),
        Some((&HANDSHAKE_EXT_DEVICE_CERT, c)) => Ok(Some(DeviceCert::decode(c)?)),
        Some(_) => Ok(None), // unknown extension: ignore (forward compatible)
    }
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

/// Apply a peer's handshake certificate (or its absence) to their state.
pub fn on_peer_cert(
    st: &mut ContactDevice,
    peer_id: &[u8; 32],
    cert: Option<&DeviceCert>,
    now_ms: u64,
) -> Result<DeviceEvent> {
    let Some(c) = cert else {
        // A contact that used to send a certificate stopped: treat as a device change.
        if let Some(old) = st.current.take() {
            st.superseded.push(old.clone());
            st.restored_at_ms = None;
            st.badge_until_ms = Some(now_ms + RESTORED_BADGE_MS);
            st.restored_unverified = true;
            return Ok(DeviceEvent::Changed {
                restored_at_ms: None,
                old_device: old.try_into().ok(),
            });
        }
        return Ok(DeviceEvent::Unchanged);
    };
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

/// Short display fingerprint of a device key (QR `omcp:2` field).
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
    const PEER: [u8; 32] = [9; 32];

    #[test]
    fn cert_roundtrip_and_tamper() {
        let d = OwnDevice::generate(&SEED, PEER, 10, Some(10)).unwrap();
        let c = d.cert().unwrap();
        assert_eq!(c.restored_at_ms, Some(10));
        let mut b = c.encode();
        assert_eq!(b.len(), DEVICE_CERT_LEN);
        assert_eq!(DeviceCert::decode(&b).unwrap(), c);
        b[70] ^= 1;
        assert!(DeviceCert::decode(&b).is_err());
        let p = handshake_payload(&c);
        assert_eq!(parse_handshake_payload(&p).unwrap(), Some(c));
        assert_eq!(parse_handshake_payload(b"omcp-ffi-v1").unwrap(), None);
    }

    #[test]
    fn identity_key_survives_restore_device_key_does_not() {
        let a = OwnDevice::generate(&SEED, PEER, 1, None).unwrap();
        let b = OwnDevice::generate(&SEED, PEER, 2, Some(2)).unwrap();
        assert_eq!(
            a.cert().unwrap().identity_sign,
            b.cert().unwrap().identity_sign
        );
        assert_ne!(a.public().unwrap(), b.public().unwrap());
    }

    #[test]
    fn rs1_restore_raises_notice_and_supersedes_old_key() {
        let mut st = ContactDevice::default();
        let a = OwnDevice::generate(&SEED, PEER, 1, None)
            .unwrap()
            .cert()
            .unwrap();
        assert_eq!(
            on_peer_cert(&mut st, &PEER, Some(&a), 5).unwrap(),
            DeviceEvent::Pinned
        );
        assert_eq!(
            on_peer_cert(&mut st, &PEER, Some(&a), 6).unwrap(),
            DeviceEvent::Unchanged
        );
        let b = OwnDevice::generate(&SEED, PEER, 100, Some(100))
            .unwrap()
            .cert()
            .unwrap();
        let ev = on_peer_cert(&mut st, &PEER, Some(&b), 200).unwrap();
        assert!(ev.is_notice());
        assert!(st.is_superseded(&a.device));
        assert!(st.badge_active(200) && !st.badge_active(200 + RESTORED_BADGE_MS));
        // The old device key is never accepted again.
        assert!(on_peer_cert(&mut st, &PEER, Some(&a), 300).is_err());
        // Another identity can't impersonate.
        let evil = OwnDevice::generate(&[4; 32], PEER, 1, None)
            .unwrap()
            .cert()
            .unwrap();
        assert!(on_peer_cert(&mut st, &PEER, Some(&evil), 300).is_err());
    }

    #[test]
    fn rs1_notice_cannot_be_suppressed() {
        // Omitting the certificate after one was pinned still raises the notice.
        let mut st = ContactDevice::default();
        let a = OwnDevice::generate(&SEED, PEER, 1, None)
            .unwrap()
            .cert()
            .unwrap();
        on_peer_cert(&mut st, &PEER, Some(&a), 1).unwrap();
        assert!(on_peer_cert(&mut st, &PEER, None, 2).unwrap().is_notice());
        // A new device without `restored_at` still counts as a change.
        let mut st = ContactDevice::default();
        on_peer_cert(&mut st, &PEER, Some(&a), 1).unwrap();
        let b = OwnDevice::generate(&SEED, PEER, 3, None)
            .unwrap()
            .cert()
            .unwrap();
        assert!(on_peer_cert(&mut st, &PEER, Some(&b), 3)
            .unwrap()
            .is_notice());
        // A restored device seen for the first time also shows it.
        let mut st = ContactDevice::default();
        let r = OwnDevice::generate(&SEED, PEER, 3, Some(3))
            .unwrap()
            .cert()
            .unwrap();
        assert!(on_peer_cert(&mut st, &PEER, Some(&r), 3)
            .unwrap()
            .is_notice());
    }
}
