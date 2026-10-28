//! BLE fragment codec + reassembly (PROTOCOL §8.1, §10.2).

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::ble::link::LinkSession;
use crate::error::{OmcpError, Result};
use crate::framing::{ENVELOPE_V1_FIXED_LEN, MAX_CIPHERTEXT_LEN};

/// Default application payload size per fragment **before** link AEAD (PROTOCOL §8.1).
pub const DEFAULT_FRAGMENT_PAYLOAD: usize = 512;

/// Reassembly timeout for incomplete messages (PROTOCOL §10.2): 30 seconds.
pub const REASSEMBLY_TIMEOUT: Duration = Duration::from_secs(30);

/// Size of the cleartext fragment header (`frag_id` + index + count).
pub const FRAGMENT_HEADER_LEN: usize = 8 + 2 + 2;

/// Largest legal encoded V1 envelope, including the full u16 AAD allowance.
pub const MAX_REASSEMBLED_BYTES: usize =
    ENVELOPE_V1_FIXED_LEN + u16::MAX as usize + MAX_CIPHERTEXT_LEN as usize;
/// Local per-link admission budgets, not changes to the wire count range.
pub const MAX_PENDING_MESSAGES: usize = 8;
pub const MAX_PENDING_FRAGMENT_SLOTS: usize = u16::MAX as usize;
pub const MAX_PENDING_BYTES: usize = 4 * MAX_REASSEMBLED_BYTES;
const AEAD_TAG_LEN: usize = 16;

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
    // Check dimensions before collecting chunks or consuming a send nonce.
    if plaintext.len() > MAX_REASSEMBLED_BYTES {
        return Err(OmcpError::FragmentMalformed(
            "message exceeds V1 envelope budget",
        ));
    }
    if plaintext.len().min(max_payload) > u16::MAX as usize - FRAGMENT_HEADER_LEN - AEAD_TAG_LEN {
        return Err(OmcpError::FragmentMalformed("frame too large"));
    }
    let count = plaintext.len().div_ceil(max_payload).max(1);
    let frag_count: u16 = count
        .try_into()
        .map_err(|_| OmcpError::FragmentMalformed("too many fragments"))?;
    let mut frames = Vec::with_capacity(count);
    for i in 0..count {
        let start = i * max_payload;
        let chunk = &plaintext[start..start.saturating_add(max_payload).min(plaintext.len())];
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
    bytes: usize,
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

        if let Some(entry) = self.pending.get(&header.frag_id) {
            if entry.frag_count != header.frag_count {
                self.pending.remove(&header.frag_id);
                return Err(OmcpError::FragmentMalformed("frag_count mismatch"));
            }
            if let Some(previous) = &entry.parts[header.frag_index as usize] {
                if previous == &chunk {
                    // An authenticated retransmission must not consume a second budget.
                    return Ok(None);
                }
                self.pending.remove(&header.frag_id);
                return Err(OmcpError::FragmentMalformed(
                    "conflicting duplicate fragment",
                ));
            }
            if chunk.len() > MAX_REASSEMBLED_BYTES - entry.bytes {
                self.pending.remove(&header.frag_id);
                return Err(OmcpError::FragmentMalformed(
                    "message exceeds V1 envelope budget",
                ));
            }
        }
        let pending_bytes: usize = self.pending.values().map(|p| p.bytes).sum();
        if chunk.len() > MAX_PENDING_BYTES - pending_bytes {
            return Err(OmcpError::FragmentMalformed(
                "pending fragment byte budget exhausted",
            ));
        }
        if !self.pending.contains_key(&header.frag_id) {
            let slots: usize = self.pending.values().map(|p| p.parts.len()).sum();
            if self.pending.len() >= MAX_PENDING_MESSAGES {
                return Err(OmcpError::FragmentMalformed(
                    "pending message budget exhausted",
                ));
            }
            if header.frag_count as usize > MAX_PENDING_FRAGMENT_SLOTS - slots {
                return Err(OmcpError::FragmentMalformed(
                    "pending fragment slot budget exhausted",
                ));
            }
            let mut parts = Vec::new();
            parts
                .try_reserve_exact(header.frag_count as usize)
                .map_err(|_| OmcpError::FragmentMalformed("fragment allocation failed"))?;
            parts.resize_with(header.frag_count as usize, || None);
            self.pending.insert(
                header.frag_id,
                Pending {
                    frag_count: header.frag_count,
                    parts,
                    started: now,
                    received: 0,
                    bytes: 0,
                },
            );
        }
        let entry = self.pending.get_mut(&header.frag_id).unwrap();
        entry.bytes += chunk.len();
        entry.parts[header.frag_index as usize] = Some(chunk);
        entry.received += 1;

        if entry.received == entry.frag_count {
            let pending = self.pending.remove(&header.frag_id).unwrap();
            let mut out = Vec::new();
            out.try_reserve_exact(pending.bytes)
                .map_err(|_| OmcpError::FragmentMalformed("message allocation failed"))?;
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
            .retain(|_, p| now.saturating_duration_since(p.started) < timeout);
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

    fn frame(send: &mut LinkSession, id: u64, index: u16, count: u16, bytes: &[u8]) -> Vec<u8> {
        let header = FragmentHeader {
            frag_id: id,
            frag_index: index,
            frag_count: count,
        }
        .encode();
        let ct = send.seal_fragment(&header, bytes).unwrap();
        let mut out = ((header.len() + ct.len()) as u16).to_be_bytes().to_vec();
        out.extend_from_slice(&header);
        out.extend_from_slice(&ct);
        out
    }

    #[test]
    fn full_wire_count_is_supported_but_aggregate_slots_are_bounded_and_expire() {
        let (mut send, mut recv) = paired();
        let mut reasm = FragmentReassembler::default();
        let now = Instant::now();
        let first = frame(&mut send, 1, 0, u16::MAX, b"a");
        assert_eq!(reasm.push_frame(&mut recv, &first, now).unwrap(), None);
        assert_eq!(reasm.pending[&1].parts.len(), u16::MAX as usize);
        let second = frame(&mut send, 2, 0, u16::MAX, b"b");
        assert_eq!(
            reasm.push_frame(&mut recv, &second, now),
            Err(OmcpError::FragmentMalformed(
                "pending fragment slot budget exhausted"
            ))
        );
        assert_eq!(reasm.pending.len(), 1);
        let after_timeout = frame(&mut send, 2, 0, u16::MAX, b"b");
        reasm
            .push_frame(&mut recv, &after_timeout, now + REASSEMBLY_TIMEOUT)
            .unwrap();
        assert_eq!(reasm.pending.len(), 1);
        assert!(reasm.pending.contains_key(&2));
    }

    #[test]
    fn pending_ids_are_bounded_before_new_slot_allocation() {
        let (mut send, mut recv) = paired();
        let mut reasm = FragmentReassembler::default();
        let now = Instant::now();
        for id in 0..MAX_PENDING_MESSAGES as u64 {
            let pdu = frame(&mut send, id, 0, 2, b"x");
            reasm.push_frame(&mut recv, &pdu, now).unwrap();
        }
        let pdu = frame(&mut send, 99, 0, 2, b"x");
        assert_eq!(
            reasm.push_frame(&mut recv, &pdu, now),
            Err(OmcpError::FragmentMalformed(
                "pending message budget exhausted"
            ))
        );
        assert_eq!(reasm.pending.len(), MAX_PENDING_MESSAGES);
    }

    #[test]
    fn unauthenticated_count_does_not_allocate_and_duplicates_do_not_grow() {
        let (mut send, mut recv) = paired();
        let mut reasm = FragmentReassembler::default();
        let now = Instant::now();
        let valid = frame(&mut send, 1, 0, 2, b"x");
        let mut forged = valid.clone();
        forged[12..14].copy_from_slice(&u16::MAX.to_be_bytes());
        assert_eq!(
            reasm.push_frame(&mut recv, &forged, now),
            Err(OmcpError::LinkAead)
        );
        assert!(reasm.pending.is_empty());
        reasm.push_frame(&mut recv, &valid, now).unwrap();
        let duplicate = frame(&mut send, 1, 0, 2, b"x");
        reasm.push_frame(&mut recv, &duplicate, now).unwrap();
        assert_eq!(reasm.pending[&1].received, 1);
        assert_eq!(reasm.pending[&1].bytes, 1);
        let conflict = frame(&mut send, 1, 0, 2, b"y");
        assert!(reasm.push_frame(&mut recv, &conflict, now).is_err());
        assert!(reasm.pending.is_empty());
        let pdu = frame(&mut send, 1, 0, 2, b"x");
        reasm.push_frame(&mut recv, &pdu, now).unwrap();
        let mismatch = frame(&mut send, 1, 1, 3, b"y");
        assert!(reasm.push_frame(&mut recv, &mismatch, now).is_err());
        assert!(reasm.pending.is_empty());
    }

    #[test]
    fn full_v1_envelope_limit_round_trips_without_reducing_aad_allowance() {
        let (mut send, mut recv) = paired();
        let mut reasm = FragmentReassembler::default();
        let bytes = vec![7; MAX_REASSEMBLED_BYTES];
        let frames = fragment_and_seal(&mut send, &bytes, 9, 512).unwrap();
        let now = Instant::now();
        let mut result = None;
        for frame in frames {
            result = reasm.push_frame(&mut recv, &frame, now).unwrap();
        }
        assert_eq!(result.unwrap(), bytes);
        assert!(reasm.pending.is_empty());
    }

    #[test]
    fn message_and_aggregate_byte_budgets_fail_before_retaining_excess() {
        let (mut send, mut recv) = paired();
        let mut reasm = FragmentReassembler::default();
        let now = Instant::now();
        let bytes = vec![1; u16::MAX as usize - FRAGMENT_HEADER_LEN - AEAD_TAG_LEN];
        for id in 0..4 {
            let mut remaining = MAX_REASSEMBLED_BYTES;
            let mut index = 0;
            while remaining > 0 {
                let len = remaining.min(bytes.len());
                let pdu = frame(&mut send, id, index, 100, &bytes[..len]);
                reasm.push_frame(&mut recv, &pdu, now).unwrap();
                remaining -= len;
                index += 1;
            }
        }
        assert_eq!(
            reasm.pending.values().map(|p| p.bytes).sum::<usize>(),
            MAX_PENDING_BYTES
        );
        let excess = frame(&mut send, 5, 0, 2, b"x");
        assert_eq!(
            reasm.push_frame(&mut recv, &excess, now),
            Err(OmcpError::FragmentMalformed(
                "pending fragment byte budget exhausted"
            ))
        );
        assert_eq!(reasm.pending.len(), 4);
        let excess_message = frame(&mut send, 0, 99, 100, b"x");
        assert_eq!(
            reasm.push_frame(&mut recv, &excess_message, now),
            Err(OmcpError::FragmentMalformed(
                "message exceeds V1 envelope budget"
            ))
        );
        assert!(!reasm.pending.contains_key(&0));
    }

    #[test]
    fn sender_rejects_dimensions_before_consuming_a_nonce() {
        let (mut send, mut recv) = paired();
        assert!(fragment_and_seal(&mut send, &vec![0; u16::MAX as usize + 1], 1, 1).is_err());
        assert!(fragment_and_seal(&mut send, &vec![0; u16::MAX as usize], 1, usize::MAX).is_err());
        let good = fragment_and_seal(&mut send, b"still synchronized", 2, 512).unwrap();
        let mut reasm = FragmentReassembler::default();
        assert_eq!(
            reasm
                .push_frame(&mut recv, &good[0], Instant::now())
                .unwrap()
                .unwrap(),
            b"still synchronized"
        );
    }
}
