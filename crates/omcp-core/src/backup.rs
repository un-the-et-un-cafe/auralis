//! Encrypted recovery-phrase backup blob (M5, experimental).
//!
//! Spec: apps/REQUIREMENTS.md "Onboarding & recovery backup (v0.1.1)",
//! PROTOCOL §11 item 9. The plaintext phrase never leaves the device; the
//! platform layer (Google Drive `appDataFolder`, iCloud) only stores the
//! opaque blob produced here.
//!
//! - KDF: Argon2id (m = 64 MiB, t = 3, p = 1, 16-byte random salt) over a
//!   user backup password (≥ 8 chars, zxcvbn score ≥ 3), **or** a 256-bit
//!   platform key (iCloud Keychain "no password" mode, iOS only).
//! - Cipher: XChaCha20-Poly1305, AAD = `"OMCP-Backup-v1" ‖ suite_id(u16 BE) ‖
//!   created_at(u64 BE, ms)`.
//! - Blob: CBOR map `{v:1, kdf, salt, nonce, ct, created_at, device_hint}`
//!   where `kdf` is `{alg:"argon2id", m, t, p}` or `{alg:"platform-key"}`.
//! - Plaintext (inside `ct`): CBOR `{entropy: bytes(32), contacts: [...]}` so
//!   contacts and verification state restore together with the identity.

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, Zeroizing};

use crate::error::{OmcpError, Result};
use crate::suite::SUITE_OMCP_2026_A;

pub const BACKUP_VERSION: u8 = 1;
pub const BACKUP_AAD_PREFIX: &[u8] = b"OMCP-Backup-v1";
pub const ARGON2_M_KIB: u32 = 64 * 1024;
pub const ARGON2_T: u32 = 3;
pub const ARGON2_P: u32 = 1;
pub const MIN_PASSWORD_CHARS: usize = 8;
pub const MIN_ZXCVBN_SCORE: u8 = 3;

/// One contact carried in the backup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupContact {
    #[serde(with = "serde_bytes_32")]
    pub peer_id: [u8; 32],
    pub name: String,
    pub verified: bool,
}

/// Decrypted backup contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupContents {
    pub entropy: Zeroizing<[u8; 32]>,
    pub contacts: Vec<BackupContact>,
}

/// How the backup key is derived.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "alg")]
pub enum Kdf {
    #[serde(rename = "argon2id")]
    Argon2id { m: u32, t: u32, p: u32 },
    #[serde(rename = "platform-key")]
    PlatformKey,
}

/// Secret used to unlock a backup.
pub enum BackupSecret<'a> {
    Password(&'a str),
    /// 256-bit key from a synced platform keychain (iCloud only).
    PlatformKey(&'a [u8; 32]),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Blob {
    v: u8,
    kdf: Kdf,
    #[serde(with = "serde_bytes_vec")]
    salt: Vec<u8>,
    #[serde(with = "serde_bytes_vec")]
    nonce: Vec<u8>,
    #[serde(with = "serde_bytes_vec")]
    ct: Vec<u8>,
    created_at: u64,
    device_hint: String,
}

#[derive(Serialize, Deserialize)]
struct Inner {
    #[serde(with = "serde_bytes_vec")]
    entropy: Vec<u8>,
    contacts: Vec<BackupContact>,
}

/// Password policy verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasswordCheck {
    pub ok: bool,
    /// zxcvbn score 0..=4.
    pub score: u8,
    pub too_short: bool,
}

/// Check a backup password: ≥ 8 chars and zxcvbn score ≥ 3.
pub fn check_password(password: &str) -> PasswordCheck {
    let too_short = password.chars().count() < MIN_PASSWORD_CHARS;
    let score = if password.is_empty() {
        0
    } else {
        u8::from(zxcvbn::zxcvbn(password, &[]).score())
    };
    PasswordCheck {
        ok: !too_short && score >= MIN_ZXCVBN_SCORE,
        score,
        too_short,
    }
}

fn aad(created_at: u64) -> Vec<u8> {
    let mut a = BACKUP_AAD_PREFIX.to_vec();
    a.extend_from_slice(&SUITE_OMCP_2026_A.to_be_bytes());
    a.extend_from_slice(&created_at.to_be_bytes());
    a
}

fn derive(kdf: &Kdf, salt: &[u8], secret: &BackupSecret<'_>) -> Result<Zeroizing<[u8; 32]>> {
    let mut key = Zeroizing::new([0u8; 32]);
    match (kdf, secret) {
        (Kdf::Argon2id { m, t, p }, BackupSecret::Password(pw)) => {
            // Bound attacker-chosen params on open (DoS guard).
            if *m > 1024 * 1024 || *t > 16 || *p > 8 {
                return Err(OmcpError::Backup("kdf params out of range"));
            }
            let params =
                Params::new(*m, *t, *p, Some(32)).map_err(|_| OmcpError::Backup("kdf params"))?;
            Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
                .hash_password_into(pw.as_bytes(), salt, key.as_mut())
                .map_err(|_| OmcpError::Backup("argon2"))?;
        }
        (Kdf::PlatformKey, BackupSecret::PlatformKey(k)) => key.copy_from_slice(&k[..]),
        _ => return Err(OmcpError::Backup("secret does not match kdf")),
    }
    Ok(key)
}

/// Encrypt `contents` into a CBOR backup blob.
pub fn seal_backup(
    contents: &BackupContents,
    secret: BackupSecret<'_>,
    created_at: u64,
    device_hint: &str,
) -> Result<Vec<u8>> {
    let kdf = match secret {
        BackupSecret::Password(pw) => {
            if !check_password(pw).ok {
                return Err(OmcpError::Backup("backup password too weak"));
            }
            Kdf::Argon2id {
                m: ARGON2_M_KIB,
                t: ARGON2_T,
                p: ARGON2_P,
            }
        }
        BackupSecret::PlatformKey(_) => Kdf::PlatformKey,
    };
    let mut salt = vec![0u8; 16];
    let mut nonce = vec![0u8; 24];
    getrandom::getrandom(&mut salt).map_err(|_| OmcpError::Crypto("getrandom"))?;
    getrandom::getrandom(&mut nonce).map_err(|_| OmcpError::Crypto("getrandom"))?;
    let key = derive(&kdf, &salt, &secret)?;

    let mut inner = Vec::new();
    ciborium::into_writer(
        &Inner {
            entropy: contents.entropy.to_vec(),
            contacts: contents.contacts.clone(),
        },
        &mut inner,
    )
    .map_err(|_| OmcpError::Backup("cbor encode"))?;
    let cipher = XChaCha20Poly1305::new_from_slice(key.as_ref())
        .map_err(|_| OmcpError::Crypto("xchacha key"))?;
    let ct = cipher
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: &inner,
                aad: &aad(created_at),
            },
        )
        .map_err(|_| OmcpError::Backup("encrypt"))?;
    inner.zeroize();

    let mut out = Vec::new();
    ciborium::into_writer(
        &Blob {
            v: BACKUP_VERSION,
            kdf,
            salt,
            nonce,
            ct,
            created_at,
            device_hint: device_hint.chars().take(64).collect(),
        },
        &mut out,
    )
    .map_err(|_| OmcpError::Backup("cbor encode"))?;
    Ok(out)
}

/// Metadata readable without the secret (for "Welcome back" prompts).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupInfo {
    pub created_at: u64,
    pub device_hint: String,
    pub needs_password: bool,
}

fn parse(blob: &[u8]) -> Result<Blob> {
    let b: Blob = ciborium::from_reader(blob).map_err(|_| OmcpError::Backup("cbor decode"))?;
    if b.v != BACKUP_VERSION {
        return Err(OmcpError::Backup("unsupported backup version"));
    }
    if b.nonce.len() != 24 || b.salt.len() != 16 {
        return Err(OmcpError::Backup("malformed backup"));
    }
    Ok(b)
}

pub fn backup_info(blob: &[u8]) -> Result<BackupInfo> {
    let b = parse(blob)?;
    Ok(BackupInfo {
        created_at: b.created_at,
        device_hint: b.device_hint,
        needs_password: matches!(b.kdf, Kdf::Argon2id { .. }),
    })
}

/// Decrypt a backup blob. Wrong password → [`OmcpError::BackupAuth`].
pub fn open_backup(blob: &[u8], secret: BackupSecret<'_>) -> Result<BackupContents> {
    let b = parse(blob)?;
    let key = derive(&b.kdf, &b.salt, &secret)?;
    let cipher = XChaCha20Poly1305::new_from_slice(key.as_ref())
        .map_err(|_| OmcpError::Crypto("xchacha key"))?;
    let inner = Zeroizing::new(
        cipher
            .decrypt(
                XNonce::from_slice(&b.nonce),
                Payload {
                    msg: &b.ct,
                    aad: &aad(b.created_at),
                },
            )
            .map_err(|_| OmcpError::BackupAuth)?,
    );
    let i: Inner =
        ciborium::from_reader(inner.as_slice()).map_err(|_| OmcpError::Backup("inner"))?;
    let mut entropy = Zeroizing::new([0u8; 32]);
    if i.entropy.len() != 32 {
        return Err(OmcpError::Backup("entropy length"));
    }
    entropy.copy_from_slice(&i.entropy);
    let mut e = i.entropy;
    e.zeroize();
    Ok(BackupContents {
        entropy,
        contacts: i.contacts,
    })
}

mod serde_bytes_vec {
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(v: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_bytes(v)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let v: ciborium::Value = Deserialize::deserialize(d)?;
        v.into_bytes()
            .map_err(|_| serde::de::Error::custom("expected bytes"))
    }
}

mod serde_bytes_32 {
    use serde::{Deserializer, Serializer};
    pub fn serialize<S: Serializer>(v: &[u8; 32], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_bytes(v)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 32], D::Error> {
        let v = super::serde_bytes_vec::deserialize(d)?;
        v.try_into()
            .map_err(|_| serde::de::Error::custom("expected 32 bytes"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recovery;

    const PW: &str = "correct horse battery staple 42";

    fn contents() -> BackupContents {
        BackupContents {
            entropy: Zeroizing::new([0x5au8; 32]),
            contacts: vec![BackupContact {
                peer_id: [7u8; 32],
                name: "Alex".into(),
                verified: true,
            }],
        }
    }

    fn contains(hay: &[u8], needle: &[u8]) -> bool {
        hay.windows(needle.len()).any(|w| w == needle)
    }

    #[test]
    fn roundtrip_password_restores_contacts_and_verification() {
        let blob = seal_backup(
            &contents(),
            BackupSecret::Password(PW),
            1_700_000_000_000,
            "Pixel",
        )
        .unwrap();
        let info = backup_info(&blob).unwrap();
        assert!(info.needs_password);
        assert_eq!(info.device_hint, "Pixel");
        let back = open_backup(&blob, BackupSecret::Password(PW)).unwrap();
        assert_eq!(back, contents());
    }

    #[test]
    fn o2_blob_never_contains_phrase_or_key_plaintext() {
        let c = contents();
        let phrase = recovery::seed_to_phrase(&c.entropy).unwrap();
        let blob = seal_backup(&c, BackupSecret::Password(PW), 1, "d").unwrap();
        assert!(!contains(&blob, &c.entropy[..]));
        assert!(!contains(&blob, phrase.as_bytes()));
        for w in phrase.split(' ').filter(|w| w.len() >= 5) {
            assert!(!contains(&blob, w.as_bytes()), "word {w} leaked");
        }
        assert!(!contains(&blob, PW.as_bytes()));
        assert!(!contains(&blob, b"Alex"));
        let key = [9u8; 32];
        let blob2 = seal_backup(&c, BackupSecret::PlatformKey(&key), 1, "d").unwrap();
        assert!(!contains(&blob2, &key));
        assert!(!contains(&blob2, &c.entropy[..]));
    }

    #[test]
    fn o4_wrong_password_fails_cleanly() {
        let blob = seal_backup(&contents(), BackupSecret::Password(PW), 1, "d").unwrap();
        assert_eq!(
            open_backup(&blob, BackupSecret::Password("wrong password 99!")),
            Err(OmcpError::BackupAuth)
        );
        assert!(open_backup(&blob, BackupSecret::PlatformKey(&[0; 32])).is_err());
    }

    #[test]
    fn tamper_and_aad_binding() {
        let key = [3u8; 32];
        let mut blob = seal_backup(&contents(), BackupSecret::PlatformKey(&key), 5, "d").unwrap();
        let n = blob.len();
        blob[n / 2] ^= 1;
        assert!(open_backup(&blob, BackupSecret::PlatformKey(&key)).is_err());
        assert!(open_backup(b"not cbor", BackupSecret::PlatformKey(&key)).is_err());
    }

    #[test]
    fn password_policy() {
        assert!(check_password("short").too_short);
        assert!(!check_password("password").ok);
        assert!(!check_password("12345678").ok);
        assert!(check_password(PW).ok);
        assert!(seal_backup(&contents(), BackupSecret::Password("password"), 1, "d").is_err());
    }
}
