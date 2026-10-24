//! OMCP-MLS-PQ hybrid mix helper (PROTOCOL §6.2, `extensions/mls-pq.md`).
//!
//! Wire tag magic: ASCII `OM01`. Provisional MLS ExtensionType: `0xF001`.
//! Full OpenMLS extension registration is deferred; this module proves the hybrid
//! mix changes exporter output and provides encode/decode + ML-KEM encaps helpers.

use hkdf::Hkdf;
use ml_kem::kem::{Ciphertext, Decapsulate, Encapsulate, Kem, KeyExport, TryKeyInit};
use ml_kem::{EncapsulationKey, MlKem768};
use sha2::Sha384;
use zeroize::Zeroize;

use crate::error::{OmcpError, Result};
use crate::handshake::{MLKEM768_CT_LEN, MLKEM768_EK_LEN, MLKEM768_SS_LEN};

/// Payload magic / wire tag (`0xOM01` in PROTOCOL stub notation).
pub const MLS_PQ_MAGIC: &[u8; 4] = b"OM01";
/// Provisional format version.
pub const MLS_PQ_VERSION: u8 = 1;
/// Provisional MLS `ExtensionType::Unknown` value (not IANA).
pub const MLS_PQ_EXTENSION_TYPE: u16 = 0xF001;
/// HKDF info label for the hybrid mix.
pub const MLS_PQ_HKDF_INFO: &[u8] = b"OMCP-MLS-PQ-v1";
/// MLS exporter label used before the PQ mix.
pub const MLS_PQ_EXPORTER_LABEL: &str = "OMCP-MLS-PQ";

/// One ML-KEM encapsulation to a group member (see `extensions/mls-pq.md` §2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EncapsEntry {
    pub recipient_leaf: u32,
    pub mlkem_ct: [u8; MLKEM768_CT_LEN],
}

/// OMCP-MLS-PQ extension payload v1.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OmcpMlsPqExtensionV1 {
    pub epoch: u64,
    pub entries: Vec<EncapsEntry>,
}

impl OmcpMlsPqExtensionV1 {
    pub fn encode(&self) -> Result<Vec<u8>> {
        if self.entries.len() > u16::MAX as usize {
            return Err(OmcpError::MlsPqMalformed("too many entries"));
        }
        let mut v = Vec::with_capacity(4 + 1 + 8 + 2 + self.entries.len() * (4 + MLKEM768_CT_LEN));
        v.extend_from_slice(MLS_PQ_MAGIC);
        v.push(MLS_PQ_VERSION);
        v.extend_from_slice(&self.epoch.to_be_bytes());
        v.extend_from_slice(&(self.entries.len() as u16).to_be_bytes());
        for e in &self.entries {
            v.extend_from_slice(&e.recipient_leaf.to_be_bytes());
            v.extend_from_slice(&e.mlkem_ct);
        }
        Ok(v)
    }

    pub fn decode(buf: &[u8]) -> Result<Self> {
        const HDR: usize = 4 + 1 + 8 + 2;
        if buf.len() < HDR {
            return Err(OmcpError::Truncated {
                need: HDR,
                have: buf.len(),
            });
        }
        if &buf[0..4] != MLS_PQ_MAGIC {
            return Err(OmcpError::MlsPqMalformed("bad magic"));
        }
        if buf[4] != MLS_PQ_VERSION {
            return Err(OmcpError::MlsPqMalformed("version"));
        }
        let epoch = u64::from_be_bytes(buf[5..13].try_into().unwrap());
        let n = u16::from_be_bytes([buf[13], buf[14]]) as usize;
        let need = HDR + n * (4 + MLKEM768_CT_LEN);
        if buf.len() < need {
            return Err(OmcpError::Truncated {
                need,
                have: buf.len(),
            });
        }
        let mut entries = Vec::with_capacity(n);
        let mut off = HDR;
        for _ in 0..n {
            let leaf = u32::from_be_bytes(buf[off..off + 4].try_into().unwrap());
            off += 4;
            let mut ct = [0u8; MLKEM768_CT_LEN];
            ct.copy_from_slice(&buf[off..off + MLKEM768_CT_LEN]);
            off += MLKEM768_CT_LEN;
            entries.push(EncapsEntry {
                recipient_leaf: leaf,
                mlkem_ct: ct,
            });
        }
        Ok(Self { epoch, entries })
    }
}

/// ML-KEM-768 keypair for OMCP-MLS-PQ (per member, out-of-band with credential).
pub struct MlsPqKemKeyPair {
    dk: <MlKem768 as Kem>::DecapsulationKey,
    ek: [u8; MLKEM768_EK_LEN],
}

impl MlsPqKemKeyPair {
    pub fn generate() -> Result<Self> {
        let (dk, ek) = MlKem768::generate_keypair();
        let encoded = ek.to_bytes();
        let mut ek_bytes = [0u8; MLKEM768_EK_LEN];
        ek_bytes.copy_from_slice(encoded.as_slice());
        Ok(Self { dk, ek: ek_bytes })
    }

    pub fn public_bytes(&self) -> &[u8; MLKEM768_EK_LEN] {
        &self.ek
    }

    pub fn encapsulate_to(
        ek_bytes: &[u8; MLKEM768_EK_LEN],
    ) -> Result<([u8; MLKEM768_CT_LEN], [u8; MLKEM768_SS_LEN])> {
        let ek = EncapsulationKey::<MlKem768>::new_from_slice(ek_bytes.as_slice())
            .map_err(|_| OmcpError::MlsPqMalformed("ek"))?;
        let (ct, ss) = ek.encapsulate();
        let mut ct_arr = [0u8; MLKEM768_CT_LEN];
        ct_arr.copy_from_slice(ct.as_slice());
        let mut ss_arr = [0u8; MLKEM768_SS_LEN];
        ss_arr.copy_from_slice(ss.as_slice());
        Ok((ct_arr, ss_arr))
    }

    pub fn decapsulate(&self, ct_bytes: &[u8; MLKEM768_CT_LEN]) -> Result<[u8; MLKEM768_SS_LEN]> {
        let ct: Ciphertext<MlKem768> = ct_bytes
            .as_slice()
            .try_into()
            .map_err(|_| OmcpError::MlsPqMalformed("ct"))?;
        let ss = self.dk.decapsulate(&ct);
        let mut ss_arr = [0u8; MLKEM768_SS_LEN];
        ss_arr.copy_from_slice(ss.as_slice());
        Ok(ss_arr)
    }
}

/// Build extension entries by encapsulating to every other member.
///
/// `others` is `(leaf_index, mlkem_ek)` sorted arbitrarily; returned entries are
/// sorted by ascending `recipient_leaf`. Also returns the ordered shared secrets
/// matching that sort (for the hybrid mix on the sender side).
pub fn encapsulate_to_members(
    epoch: u64,
    others: &[(u32, [u8; MLKEM768_EK_LEN])],
) -> Result<(OmcpMlsPqExtensionV1, Vec<[u8; MLKEM768_SS_LEN]>)> {
    let mut pairs: Vec<(EncapsEntry, [u8; MLKEM768_SS_LEN])> = Vec::with_capacity(others.len());
    for (leaf, ek) in others {
        let (ct, ss) = MlsPqKemKeyPair::encapsulate_to(ek)?;
        pairs.push((
            EncapsEntry {
                recipient_leaf: *leaf,
                mlkem_ct: ct,
            },
            ss,
        ));
    }
    pairs.sort_by_key(|(e, _)| e.recipient_leaf);
    let mut entries = Vec::with_capacity(pairs.len());
    let mut secrets = Vec::with_capacity(pairs.len());
    for (e, ss) in pairs {
        entries.push(e);
        secrets.push(ss);
    }
    Ok((OmcpMlsPqExtensionV1 { epoch, entries }, secrets))
}

/// `pq_ikm = BLAKE3(ss[0] || … || ss[n-1])` (empty → BLAKE3("")) then
/// `HKDF-SHA-384(salt=mls_exporter, ikm=pq_ikm, info=OMCP-MLS-PQ-v1, L=32)`.
pub fn mix_exporter(
    mls_exporter: &[u8],
    kem_shared_secrets: &[[u8; MLKEM768_SS_LEN]],
) -> Result<[u8; 32]> {
    let mut concat = Vec::with_capacity(kem_shared_secrets.len() * MLKEM768_SS_LEN);
    for ss in kem_shared_secrets {
        concat.extend_from_slice(ss);
    }
    let pq_ikm = blake3::hash(&concat);
    concat.zeroize();
    let hk = Hkdf::<Sha384>::new(Some(mls_exporter), pq_ikm.as_bytes());
    let mut out = [0u8; 32];
    hk.expand(MLS_PQ_HKDF_INFO, &mut out)
        .map_err(|_| OmcpError::Crypto("mls-pq hkdf"))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mls_pq_extension_roundtrip() {
        let mut ct = [0u8; MLKEM768_CT_LEN];
        ct[0] = 0xAB;
        let ext = OmcpMlsPqExtensionV1 {
            epoch: 7,
            entries: vec![EncapsEntry {
                recipient_leaf: 1,
                mlkem_ct: ct,
            }],
        };
        let enc = ext.encode().unwrap();
        assert_eq!(&enc[0..4], b"OM01");
        let dec = OmcpMlsPqExtensionV1::decode(&enc).unwrap();
        assert_eq!(dec, ext);
    }

    #[test]
    fn mls_pq_kat_two_ss() {
        let exporter = [0u8; 32];
        let ss0 = [0x01u8; 32];
        let ss1 = [0x02u8; 32];
        let hybrid = mix_exporter(&exporter, &[ss0, ss1]).unwrap();
        assert_eq!(
            hex::encode(blake3::hash(&[ss0, ss1].concat()).as_bytes()),
            "8d67bc7836d128b108be2c965538f37bbcee3e7503e35e58fbb0446432e05206"
        );
        assert_eq!(
            hex::encode(hybrid),
            "39dcdbba4ddd834fffc736e68d69e9a1b0442b76582264488edc676a8acc13a5"
        );
        // Changing any ss must change the hybrid output.
        let mut ss1b = ss1;
        ss1b[0] ^= 1;
        let hybrid2 = mix_exporter(&exporter, &[ss0, ss1b]).unwrap();
        assert_ne!(hybrid, hybrid2);
    }

    #[test]
    fn encapsulate_decapsulate_and_mix_changes_exporter() {
        let a = MlsPqKemKeyPair::generate().unwrap();
        let b = MlsPqKemKeyPair::generate().unwrap();
        let mls_exporter = [0x42u8; 32];
        let (ext, secrets) =
            encapsulate_to_members(1, &[(1, *b.public_bytes()), (0, *a.public_bytes())]).unwrap();
        assert_eq!(ext.entries[0].recipient_leaf, 0);
        assert_eq!(ext.entries[1].recipient_leaf, 1);
        // Recipient B decaps their entry
        let ss_b = b.decapsulate(&ext.entries[1].mlkem_ct).unwrap();
        assert_eq!(ss_b, secrets[1]);
        let hybrid = mix_exporter(&mls_exporter, &secrets).unwrap();
        let classical_only = mix_exporter(&mls_exporter, &[]).unwrap();
        assert_ne!(hybrid, classical_only);
    }
}
