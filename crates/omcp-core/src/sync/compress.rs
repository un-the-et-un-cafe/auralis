//! Padding buckets and (optional) compression before encryption
//! (PROTOCOL §22.3).
//!
//! # Side-channel warning (CRIME/BREACH)
//! Compressing before encrypting makes ciphertext length depend on content.
//! If an attacker can inject text (a quote, a forward, a crafted link) into
//! the same compression context as a secret, observed lengths leak the
//! secret byte-by-byte. This module therefore enforces the §22.3 MUST rules
//! in code, not by convention:
//! 1. **Padding is mandatory**: every ciphertext produced by [`seal_padded`]
//!    is exactly a bucket length (256 B / 1 KiB / 4 KiB / 16 KiB, then the
//!    next multiple of 16 KiB); [`check_bucket_len`] rejects anything else.
//! 2. **Never compress secrets or control data**: [`FrameClass::Secret`] and
//!    [`FrameClass::Control`] frames are always stored raw.
//! 3. **Attacker-influenced text is isolated**: each frame is compressed in
//!    its own zstd context, and quoted/forwarded text must be its own
//!    [`FrameClass::Quoted`] frame (callers MAY disable it via
//!    [`SealOptions::compress_quoted`]).
//! 4. **Static public dictionary only**: [`OMCP_DICT_V1`] is a compile-time
//!    constant; the API takes no dictionary argument and the decoder rejects
//!    any other dictionary id.
//! 5. **Burn and Nearby messages are never compressed**.
//!
//! Inner plaintext layout (before AEAD):
//! `inner_len u32-be | frame* | zero padding`, where a frame is
//! `class u8 | flags u8 | [dict_id u8 | raw_len varint]? | len varint | bytes`.

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};

use super::{get_varint, put_varint, take};
use crate::error::{OmcpError, Result};
use crate::framing::MAX_CIPHERTEXT_LEN;

/// Padding buckets below 16 KiB.
pub const BUCKETS: [usize; 4] = [256, 1024, 4096, 16 * 1024];
/// Above the last bucket, round up to a multiple of this.
pub const BUCKET_STEP: usize = 16 * 1024;
/// ChaCha20-Poly1305 tag length.
pub const AEAD_TAG_LEN: usize = 16;
/// Dictionary id of [`OMCP_DICT_V1`].
pub const DICT_ID_V1: u8 = 1;
/// zstd level.
const LEVEL: i32 = 3;
/// Frames shorter than this are never compressed (no gain, more leakage).
const MIN_COMPRESS_LEN: usize = 64;

/// The static, public OMCP v1 dictionary (raw-content zstd dictionary).
/// It is part of the spec; changing it requires a new [`DICT_ID_V1`].
pub const OMCP_DICT_V1: &[u8] = b"\
the and you that have for not with this but what are was they your can just \
will all there about would when like out know get one think time yes okay ok \
thanks thank you please sorry good great morning night today tomorrow tonight \
where are you on my way see you soon love you haha lol :) :( <3 \
https://www. .com .org http:// @ # \
le la les de des et est pas pour que qui une un je tu il elle nous vous merci \
bonjour bonsoir salut oui non der die das und ist nicht ich du wir ihr danke hallo \
el los las que y es no por para gracias hola si \
image photo video voice message call missed reply forwarded quoted location \
";

/// Bucket length for a ciphertext that would otherwise be `n` bytes.
pub fn bucket_for(n: usize) -> usize {
    for b in BUCKETS {
        if n <= b {
            return b;
        }
    }
    n.div_ceil(BUCKET_STEP) * BUCKET_STEP
}

/// Is `len` a valid padded ciphertext length?
pub fn check_bucket_len(len: usize) -> Result<()> {
    if bucket_for(len) == len && len > 0 {
        Ok(())
    } else {
        Err(OmcpError::Sync("ciphertext not padded to a bucket"))
    }
}

/// What a frame contains — decides whether it may be compressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FrameClass {
    /// The sender's own text.
    OwnText = 1,
    /// Quoted or forwarded third-party (attacker-influenced) text.
    Quoted = 2,
    /// Keys, tokens, recovery data. Never compressed.
    Secret = 3,
    /// Control messages (receipts, MLS handshake, routing). Never compressed.
    Control = 4,
    /// Already-compressed / opaque binary (media refs, files). Not compressed.
    Binary = 5,
}

impl FrameClass {
    fn from_u8(b: u8) -> Result<Self> {
        Ok(match b {
            1 => Self::OwnText,
            2 => Self::Quoted,
            3 => Self::Secret,
            4 => Self::Control,
            5 => Self::Binary,
            _ => return Err(OmcpError::Sync("frame class")),
        })
    }

    /// Classes for which compression is ever allowed.
    pub fn compressible(self) -> bool {
        matches!(self, Self::OwnText | Self::Quoted)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub class: FrameClass,
    pub data: Vec<u8>,
}

/// Per-message options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SealOptions {
    /// Compression is a MAY; off disables it entirely.
    pub compress: bool,
    /// Also compress (in its own context) quoted/forwarded frames.
    pub compress_quoted: bool,
    /// Burn-after-read message: compression MUST be off.
    pub burn: bool,
    /// Nearby (§20) message: compression MUST be off.
    pub nearby: bool,
}

impl Default for SealOptions {
    fn default() -> Self {
        Self {
            compress: true,
            compress_quoted: false,
            burn: false,
            nearby: false,
        }
    }
}

impl SealOptions {
    fn may_compress(&self, class: FrameClass) -> bool {
        self.compress
            && !self.burn
            && !self.nearby
            && class.compressible()
            && (class != FrameClass::Quoted || self.compress_quoted)
    }
}

const FLAG_ZSTD: u8 = 1;

fn zstd_compress(data: &[u8]) -> Result<Vec<u8>> {
    // Fresh context per frame: no cross-frame history.
    let mut c = zstd::bulk::Compressor::with_dictionary(LEVEL, OMCP_DICT_V1)
        .map_err(|_| OmcpError::Sync("zstd init"))?;
    c.compress(data).map_err(|_| OmcpError::Sync("zstd"))
}

fn zstd_decompress(data: &[u8], raw_len: usize) -> Result<Vec<u8>> {
    let mut d = zstd::bulk::Decompressor::with_dictionary(OMCP_DICT_V1)
        .map_err(|_| OmcpError::Sync("zstd init"))?;
    let out = d
        .decompress(data, raw_len)
        .map_err(|_| OmcpError::Sync("zstd decode"))?;
    if out.len() != raw_len {
        return Err(OmcpError::Sync("zstd length"));
    }
    Ok(out)
}

/// Encode frames into an inner plaintext padded so that the AEAD ciphertext
/// is exactly a bucket length.
pub fn encode_padded(frames: &[Frame], opts: SealOptions) -> Result<Vec<u8>> {
    let mut body = Vec::new();
    for f in frames {
        body.push(f.class as u8);
        let packed = if opts.may_compress(f.class) && f.data.len() >= MIN_COMPRESS_LEN {
            let z = zstd_compress(&f.data)?;
            (z.len() < f.data.len()).then_some(z)
        } else {
            None
        };
        match packed {
            Some(z) => {
                body.push(FLAG_ZSTD);
                body.push(DICT_ID_V1);
                put_varint(&mut body, f.data.len() as u64);
                put_varint(&mut body, z.len() as u64);
                body.extend_from_slice(&z);
            }
            None => {
                body.push(0);
                put_varint(&mut body, f.data.len() as u64);
                body.extend_from_slice(&f.data);
            }
        }
    }
    let ct_len = bucket_for(4 + body.len() + AEAD_TAG_LEN);
    if ct_len > MAX_CIPHERTEXT_LEN as usize {
        return Err(OmcpError::Sync("payload exceeds envelope max"));
    }
    let mut out = Vec::with_capacity(ct_len - AEAD_TAG_LEN);
    out.extend_from_slice(&(body.len() as u32).to_be_bytes());
    out.extend_from_slice(&body);
    out.resize(ct_len - AEAD_TAG_LEN, 0);
    Ok(out)
}

/// Inverse of [`encode_padded`]; checks padding is all-zero.
pub fn decode_padded(inner: &[u8]) -> Result<Vec<Frame>> {
    check_bucket_len(inner.len() + AEAD_TAG_LEN)?;
    let mut buf = inner;
    let n = u32::from_be_bytes(take(&mut buf, 4)?.try_into().expect("4")) as usize;
    let mut body = take(&mut buf, n)?;
    if buf.iter().any(|b| *b != 0) {
        return Err(OmcpError::Sync("nonzero padding"));
    }
    let mut frames = Vec::new();
    while !body.is_empty() {
        let class = FrameClass::from_u8(take(&mut body, 1)?[0])?;
        let flags = take(&mut body, 1)?[0];
        let data = match flags {
            0 => {
                let l = get_varint(&mut body)? as usize;
                take(&mut body, l)?.to_vec()
            }
            FLAG_ZSTD => {
                if !class.compressible() {
                    return Err(OmcpError::Sync("compressed frame of forbidden class"));
                }
                if take(&mut body, 1)?[0] != DICT_ID_V1 {
                    return Err(OmcpError::Sync("unknown dictionary"));
                }
                let raw = get_varint(&mut body)? as usize;
                if raw > MAX_CIPHERTEXT_LEN as usize {
                    return Err(OmcpError::Sync("decompressed size too large"));
                }
                let l = get_varint(&mut body)? as usize;
                zstd_decompress(take(&mut body, l)?, raw)?
            }
            _ => return Err(OmcpError::Sync("frame flags")),
        };
        frames.push(Frame { class, data });
    }
    Ok(frames)
}

/// Encode, pad and AEAD-seal. The result is always a bucket length.
pub fn seal_padded(
    key: &[u8; 32],
    nonce: &[u8; 12],
    aad: &[u8],
    frames: &[Frame],
    opts: SealOptions,
) -> Result<Vec<u8>> {
    let pt = encode_padded(frames, opts)?;
    let c = ChaCha20Poly1305::new_from_slice(key).map_err(|_| OmcpError::Crypto("key"))?;
    let ct = c
        .encrypt(Nonce::from_slice(nonce), Payload { msg: &pt, aad })
        .map_err(|_| OmcpError::Crypto("seal"))?;
    check_bucket_len(ct.len())?;
    Ok(ct)
}

/// Open a padded ciphertext; rejects non-bucket lengths before decrypting.
pub fn open_padded(key: &[u8; 32], nonce: &[u8; 12], aad: &[u8], ct: &[u8]) -> Result<Vec<Frame>> {
    check_bucket_len(ct.len())?;
    let c = ChaCha20Poly1305::new_from_slice(key).map_err(|_| OmcpError::Crypto("key"))?;
    let pt = c
        .decrypt(Nonce::from_slice(nonce), Payload { msg: ct, aad })
        .map_err(|_| OmcpError::Crypto("open"))?;
    decode_padded(&pt)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn txt(class: FrameClass, s: &str) -> Frame {
        Frame {
            class,
            data: s.as_bytes().to_vec(),
        }
    }

    fn compressed_flags(inner: &[u8]) -> Vec<(u8, u8)> {
        // (class, flags) per frame, re-parsed from the raw layout.
        let n = u32::from_be_bytes(inner[..4].try_into().unwrap()) as usize;
        let mut b = &inner[4..4 + n];
        let mut out = vec![];
        while !b.is_empty() {
            let c = take(&mut b, 1).unwrap()[0];
            let f = take(&mut b, 1).unwrap()[0];
            if f == FLAG_ZSTD {
                take(&mut b, 1).unwrap();
                get_varint(&mut b).unwrap();
            }
            let l = get_varint(&mut b).unwrap() as usize;
            take(&mut b, l).unwrap();
            out.push((c, f));
        }
        out
    }

    const LONG: &str =
        "thanks see you soon, on my way, see you soon, on my way, see you soon love you haha";

    #[test]
    fn buckets() {
        assert_eq!(bucket_for(1), 256);
        assert_eq!(bucket_for(257), 1024);
        assert_eq!(bucket_for(4097), 16384);
        assert_eq!(bucket_for(16385), 32768);
        assert_eq!(bucket_for(40_000), 49152);
        assert!(check_bucket_len(1024).is_ok());
        assert!(check_bucket_len(1000).is_err());
        assert!(check_bucket_len(0).is_err());
    }

    #[test]
    fn every_ciphertext_is_bucketed_and_roundtrips() {
        let k = [1u8; 32];
        let n = [2u8; 12];
        for len in [0usize, 10, 200, 240, 241, 1000, 5000, 20_000, 100_000] {
            let fr = vec![Frame {
                class: FrameClass::Binary,
                data: vec![0xAB; len],
            }];
            let ct = seal_padded(&k, &n, b"aad", &fr, SealOptions::default()).unwrap();
            check_bucket_len(ct.len()).unwrap();
            assert_eq!(open_padded(&k, &n, b"aad", &ct).unwrap(), fr);
        }
        assert!(open_padded(&k, &n, b"aad", &[0u8; 300]).is_err());
    }

    #[test]
    fn own_text_is_compressed_secrets_and_control_never() {
        let fr = vec![
            txt(FrameClass::OwnText, LONG),
            txt(FrameClass::Secret, LONG),
            txt(FrameClass::Control, LONG),
            txt(FrameClass::Quoted, LONG),
        ];
        let inner = encode_padded(&fr, SealOptions::default()).unwrap();
        assert_eq!(
            compressed_flags(&inner),
            vec![(1, FLAG_ZSTD), (3, 0), (4, 0), (2, 0)]
        );
        assert_eq!(decode_padded(&inner).unwrap(), fr);
        // Quoted text may be compressed only in its own frame / context.
        let o = SealOptions {
            compress_quoted: true,
            ..Default::default()
        };
        let inner = encode_padded(&fr, o).unwrap();
        assert_eq!(compressed_flags(&inner)[3], (2, FLAG_ZSTD));
        assert_eq!(decode_padded(&inner).unwrap(), fr);
    }

    #[test]
    fn burn_and_nearby_disable_compression() {
        let fr = vec![txt(FrameClass::OwnText, LONG)];
        for o in [
            SealOptions {
                burn: true,
                ..Default::default()
            },
            SealOptions {
                nearby: true,
                ..Default::default()
            },
            SealOptions {
                compress: false,
                ..Default::default()
            },
        ] {
            let inner = encode_padded(&fr, o).unwrap();
            assert_eq!(compressed_flags(&inner), vec![(1, 0)]);
        }
    }

    #[test]
    fn decoder_rejects_forbidden_and_foreign_dict() {
        let fr = vec![txt(FrameClass::OwnText, LONG)];
        let mut inner = encode_padded(&fr, SealOptions::default()).unwrap();
        let mut bad = inner.clone();
        bad[4] = FrameClass::Secret as u8; // compressed secret frame
        assert!(decode_padded(&bad).is_err());
        inner[6] = 2; // dictionary id
        assert!(decode_padded(&inner).is_err());
        let mut pad = encode_padded(&fr, SealOptions::default()).unwrap();
        *pad.last_mut().unwrap() = 1;
        assert!(decode_padded(&pad).is_err());
    }

    #[test]
    fn padding_hides_compressibility_within_a_bucket() {
        // A highly compressible and an incompressible message of similar size
        // land in the same bucket.
        let a = vec![txt(FrameClass::OwnText, &"see you soon ".repeat(15))];
        let mut x = crate::sync::SplitMix64(1);
        let b = vec![Frame {
            class: FrameClass::OwnText,
            data: (0..195).map(|_| x.next_u64() as u8).collect(),
        }];
        let la = encode_padded(&a, SealOptions::default()).unwrap().len();
        let lb = encode_padded(&b, SealOptions::default()).unwrap().len();
        assert_eq!(la, lb);
    }
}
