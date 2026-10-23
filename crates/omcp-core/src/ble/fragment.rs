//! BLE fragment codec + reassembly (PROTOCOL §8.1, §10.2).

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::ble::link::LinkSession;
use crate::error::{OmcpError, Result};

/// Default application payload size per fragment **before** link AEAD (PROTOCOL §8.1).
pub const DEFAULT_FRAGMENT_PAYLOAD: usize = 512;

/// Reassembly timeout for incomplete messages (PROTOCOL §10.2): 30 seconds.
pub const REASSEMBLY_TIMEOUT: Duration = Duration::from_secs(30);

/// Size of the cleartext fragment header (`frag_id` + index + count).
pub const FRAGMENT_HEADER_LEN: usize = 8 + 2 + 2;

/// Fragment header fields (PROTOCOL §10.2) — also used as link-AEAD AAD.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FragmentHeader {
    /// Random id shared by all fragments of one logical message.
    pub frag_id: u64,
    /// Zero-based index of this fragment.
    pub frag_index: u16,
    /// Total number of fragments in the message (`>= 1`).
    pub frag_count: u16,
}

impl FragmentHeader {
    /// Encode header as AAD / wire prefix (big-endian).
    pub fn encode(&self) -> [u8; FRAGMENT_HEADER_LEN] {
        let mut out = [0u8; FRAGMENT_HEADER_LEN];
        out[0..8].copy_from_slice(&self.frag_id.to_be_bytes());
        out[8..10].copy_from_slice(&self.frag_index.to_be_bytes());
        out[10..12].copy_from_slice(&self.frag_count.to_be_bytes());
        out
    }

    /// Decode a 12-byte header.
    pub fn decode(buf: &[u8]) -> Result<Self> {
        if buf.len() < FRAGMENT_HEADER_LEN {
            return Err(OmcpError::Truncated {
                need: FRAGMENT_HEADER_LEN,
                have: buf.len(),
            });
        }
        let frag_id = u64::from_be_bytes(buf[0..8].try_into().unwrap());
        let frag_index = u16::from_be_bytes([buf[8], buf[9]]);
        let frag_count = u16::from_be_bytes([buf[10], buf[11]]);
        if frag_count == 0 || frag_index >= frag_count {
            return Err(OmcpError::FragmentMalformed("index/count"));
        }
        Ok(Self {
            frag_id,
            frag_index,
            frag_count,
        })
    }
}

/// Encode `plaintext` into length-prefixed BLE frames with link AEAD.
///
/// Each frame: `frame_len(u16 BE) ‖ header(12) ‖ ciphertext(+tag)`.
pub fn fragment_and_seal(
    session: &mut LinkSession,
    plaintext: &[u8],
    frag_id: u64,
    max_payload: usize,
) -> Result<Vec<Vec<u8>>> {
    if max_payload == 0 {
        return Err(OmcpError::FragmentMalformed("max_payload"));
    }
    let chunks: Vec<&[u8]> = if plaintext.is_empty() {
        vec![plaintext]
    } else {
        plaintext.chunks(max_payload).collect()
    };
    let frag_count: u16 = chunks
        .len()
        .try_into()
        .map_err(|_| OmcpError::FragmentMalformed("too many fragments"))?;
    let mut frames = Vec::with_capacity(chunks.len());
    for (i, chunk) in chunks.into_iter().enumerate() {
        let header = FragmentHeader {
            frag_id,
            frag_index: i as u16,
            frag_count,
        };
        let aad = header.encode();
        let ct = session.seal_fragment(&aad, chunk)?;
        let body_len = FRAGMENT_HEADER_LEN + ct.len();
        if body_len > u16::MAX as usize {
            return Err(OmcpError::FragmentMalformed("frame too large"));
        }
        let mut frame = Vec::with_capacity(2 + body_len);
        frame.extend_from_slice(&(body_len as u16).to_be_bytes());
        frame.extend_from_slice(&aad);
        frame.extend_from_slice(&ct);
        frames.push(frame);
    }
    Ok(frames)
}

/// Parse one length-prefixed frame into `(header, ciphertext)`.
pub fn parse_frame(frame: &[u8]) -> Result<(FragmentHeader, &[u8])> {
    if frame.len() < 2 + FRAGMENT_HEADER_LEN {
        return Err(OmcpError::Truncated {
            need: 2 + FRAGMENT_HEADER_LEN,
            have: frame.len(),
        });
    }
    let body_len = u16::from_be_bytes([frame[0], frame[1]]) as usize;
    if frame.len() != 2 + body_len {
        return Err(OmcpError::Truncated {
            need: 2 + body_len,
            have: frame.len(),
        });
    }
    let header = FragmentHeader::decode(&frame[2..2 + FRAGMENT_HEADER_LEN])?;
    let ct = &frame[2 + FRAGMENT_HEADER_LEN..];
    if ct.is_empty() {
        return Err(OmcpError::FragmentMalformed("empty ciphertext"));
    }
    Ok((header, ct))
}

struct Pending {
    frag_count: u16,
    parts: Vec<Option<Vec<u8>>>,
    started: Instant,
    received: u16,
}

/// Reassembles link-AEAD fragments into a full plaintext message.
pub struct FragmentReassembler {
    pending: HashMap<u64, Pending>,
    timeout: Duration,
}

impl Default for FragmentReassembler {
    fn default() -> Self {
        Self::new(REASSEMBLY_TIMEOUT)
    }
}

impl FragmentReassembler {
    /// Create a reassembler with the given incomplete-message timeout.
    pub fn new(timeout: Duration) -> Self {
        Self {
            pending: HashMap::new(),
            timeout,
        }
    }

    /// Open one frame and return a completed plaintext when all fragments arrive.
    ///
    /// Incomplete messages older than the timeout are dropped (PROTOCOL §10.2).
    pub fn push_frame(
        &mut self,
        session: &mut LinkSession,
        frame: &[u8],
        now: Instant,
    ) -> Result<Option<Vec<u8>>> {
        self.evict_timed_out(now);
        let (header, ct) = parse_frame(frame)?;
        let aad = header.encode();
        let chunk = session.open_fragment(&aad, ct)?;

        let entry = self
            .pending
            .entry(header.frag_id)
            .or_insert_with(|| Pending {
                frag_count: header.frag_count,
                parts: vec![None; header.frag_count as usize],
                started: now,
                received: 0,
            });

        if entry.frag_count != header.frag_count {
            self.pending.remove(&header.frag_id);
            return Err(OmcpError::FragmentMalformed("frag_count mismatch"));
        }
        let idx = header.frag_index as usize;
        if entry.parts[idx].is_some() {
            // Duplicate fragment — ignore (idempotent).
            return Ok(None);
        }
        entry.parts[idx] = Some(chunk);
        entry.received += 1;

        if entry.received == entry.frag_count {
            let pending = self.pending.remove(&header.frag_id).unwrap();
            let mut out = Vec::new();
            for part in pending.parts {
                out.extend_from_slice(&part.expect("all parts present"));
            }
            return Ok(Some(out));
        }
        Ok(None)
    }

    fn evict_timed_out(&mut self, now: Instant) {
        let timeout = self.timeout;
        self.pending
            .retain(|_, p| now.duration_since(p.started) < timeout);
    }
}

/// Convenience: random `frag_id` from the platform CSPRNG.
pub fn random_frag_id() -> Result<u64> {
    let mut b = [0u8; 8];
    getrandom::getrandom(&mut b).map_err(|_| OmcpError::Crypto("getrandom"))?;
    Ok(u64::from_le_bytes(b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ble::link::LinkSession;
    use crate::handshake::{HybridInitiator, HybridResponder, StaticIdentity};

    fn paired() -> (LinkSession, LinkSession) {
        let responder_id = StaticIdentity::generate().unwrap();
        let initiator_id = StaticIdentity::generate().unwrap();
        let mut init = HybridInitiator::new(initiator_id, &responder_id.dh_public()).unwrap();
        let mut resp = HybridResponder::new(responder_id);
        let msg1 = init.write_message_1(b"").unwrap();
        resp.read_message_1(&msg1).unwrap();
        let (msg2, keys_r) = resp.write_message_2(b"").unwrap();
        let (_, keys_i) = init.read_message_2(&msg2).unwrap();
        (
            LinkSession::initiator(&keys_i),
            LinkSession::responder(&keys_r),
        )
    }

    #[test]
    fn multi_fragment_round_trip() {
        let (mut send, mut recv) = paired();
        let mut reasm = FragmentReassembler::default();
        let payload = vec![0x5Au8; 1200]; // > 512 → multiple fragments
        let frames = fragment_and_seal(&mut send, &payload, 0xDEAD_BEEF, 512).unwrap();
        assert!(frames.len() > 1);
        let mut got = None;
        let now = Instant::now();
        for f in &frames {
            if let Some(pt) = reasm.push_frame(&mut recv, f, now).unwrap() {
                got = Some(pt);
            }
        }
        assert_eq!(got.unwrap(), payload);
    }

    #[test]
    fn reassembly_timeout_drops() {
        let (mut send, mut recv) = paired();
        let mut reasm = FragmentReassembler::new(Duration::from_millis(5));
        let payload = vec![1u8; 600];
        let frames = fragment_and_seal(&mut send, &payload, 99, 512).unwrap();
        let t0 = Instant::now();
        // Deliver only the first fragment.
        assert!(reasm
            .push_frame(&mut recv, &frames[0], t0)
            .unwrap()
            .is_none());
        let t1 = t0 + Duration::from_millis(20);
        // After timeout the pending entry is gone; second fragment alone cannot complete.
        let r = reasm.push_frame(&mut recv, &frames[1], t1).unwrap();
        assert!(r.is_none());
    }
}
