//! 1:1 application text messages inside [`EnvelopeV1`] (PROTOCOL §6.1, §6.3).

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use hkdf::Hkdf;
use sha2::Sha384;

use crate::error::{OmcpError, Result};
use crate::framing::{flags, EnvelopeV1};
use crate::suite::{ENVELOPE_VERSION_V1, SUITE_OMCP_2026_A};

/// HKDF info for deriving application AEAD keys from Noise transport keys.
///
/// Separates app and link nonce spaces (see `ble` module docs).
pub const APP_AEAD_HKDF_INFO: &[u8] = b"OMCP-App-AEAD-v1";

/// Content type `0x01` — `text/plain; charset=utf-8` (PROTOCOL §6.3).
pub const CONTENT_TYPE_TEXT_PLAIN: u8 = 0x01;

/// Derive a 32-byte application AEAD key from a directional transport key.
pub fn derive_app_key(transport_key: &[u8; 32]) -> Result<[u8; 32]> {
    let hk = Hkdf::<Sha384>::new(Some(transport_key.as_slice()), &[]);
    let mut out = [0u8; 32];
    hk.expand(APP_AEAD_HKDF_INFO, &mut out)
        .map_err(|_| OmcpError::Crypto("app hkdf"))?;
    Ok(out)
}

/// Build §6.1 associated data for an application message.
pub fn application_aad(
    suite_id: u16,
    sender_peer_id: &[u8; 32],
    msg_id: &[u8; 16],
    timestamp_ms: u64,
    content_type: u8,
) -> Vec<u8> {
    let mut aad = Vec::with_capacity(2 + 32 + 16 + 8 + 1);
    aad.extend_from_slice(&suite_id.to_be_bytes());
    aad.extend_from_slice(sender_peer_id);
    aad.extend_from_slice(msg_id);
    aad.extend_from_slice(&timestamp_ms.to_be_bytes());
    aad.push(content_type);
    aad
}

fn app_nonce(counter: u64) -> Nonce {
    // Prefer counter‖random (§5.3); M2 uses Noise-style counter with zero prefix,
    // random entropy supplied by a distinct key derivation (APP_AEAD_HKDF_INFO).
    let mut n = [0u8; 12];
    n[4..].copy_from_slice(&counter.to_le_bytes());
    Nonce::from(n)
}

/// Seal a UTF-8 text chat message into an [`EnvelopeV1`].
///
/// `send_transport_key` is the directional Noise key this peer uses to send
/// (`k_i2r` for initiator, `k_r2i` for responder). `app_nonce_counter` is
/// incremented on success.
pub fn seal_text_envelope(
    send_transport_key: &[u8; 32],
    sender_peer_id: &[u8; 32],
    plaintext_utf8: &str,
    created_ms: u64,
    app_nonce_counter: &mut u64,
    ttl_hops: u8,
) -> Result<EnvelopeV1> {
    let mut envelope_id = [0u8; 16];
    getrandom::getrandom(&mut envelope_id).map_err(|_| OmcpError::Crypto("getrandom"))?;

    let content_type = CONTENT_TYPE_TEXT_PLAIN;
    let aad = application_aad(
        SUITE_OMCP_2026_A,
        sender_peer_id,
        &envelope_id,
        created_ms,
        content_type,
    );
    let app_key = derive_app_key(send_transport_key)?;
    let cipher =
        ChaCha20Poly1305::new_from_slice(&app_key).map_err(|_| OmcpError::Crypto("chacha key"))?;
    let ciphertext = cipher
        .encrypt(
            &app_nonce(*app_nonce_counter),
            Payload {
                msg: plaintext_utf8.as_bytes(),
                aad: &aad,
            },
        )
        .map_err(|_| OmcpError::AppAead)?;
    *app_nonce_counter = app_nonce_counter
        .checked_add(1)
        .ok_or(OmcpError::Crypto("nonce overflow"))?;

    // header_aad: content_type hint only (no secrets) — PROTOCOL §10.1.
    Ok(EnvelopeV1 {
        version: ENVELOPE_VERSION_V1,
        suite_id: SUITE_OMCP_2026_A,
        envelope_id,
        flags: flags::RELAY_OK | flags::STORE_OK,
        ttl_hops,
        created_ms,
        header_aad: vec![content_type],
        ciphertext,
    })
}

/// Open a text envelope sealed by [`seal_text_envelope`].
///
/// `recv_transport_key` is the directional key the **sender** used (i.e. the
/// receiver's recv direction key).
pub fn open_text_envelope(
    recv_transport_key: &[u8; 32],
    sender_peer_id: &[u8; 32],
    envelope: &EnvelopeV1,
    app_nonce_counter: &mut u64,
) -> Result<String> {
    if envelope.header_aad.first().copied() != Some(CONTENT_TYPE_TEXT_PLAIN) {
        return Err(OmcpError::AppAead);
    }
    let aad = application_aad(
        envelope.suite_id,
        sender_peer_id,
        &envelope.envelope_id,
        envelope.created_ms,
        CONTENT_TYPE_TEXT_PLAIN,
    );
    let app_key = derive_app_key(recv_transport_key)?;
    let cipher =
        ChaCha20Poly1305::new_from_slice(&app_key).map_err(|_| OmcpError::Crypto("chacha key"))?;
    let plaintext = cipher
        .decrypt(
            &app_nonce(*app_nonce_counter),
            Payload {
                msg: &envelope.ciphertext,
                aad: &aad,
            },
        )
        .map_err(|_| OmcpError::AppAead)?;
    *app_nonce_counter = app_nonce_counter
        .checked_add(1)
        .ok_or(OmcpError::Crypto("nonce overflow"))?;
    String::from_utf8(plaintext).map_err(|_| OmcpError::AppAead)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handshake::{HybridInitiator, HybridResponder, StaticIdentity};

    #[test]
    fn text_envelope_round_trip() {
        let responder_id = StaticIdentity::generate().unwrap();
        let initiator_id = StaticIdentity::generate().unwrap();
        let sender_peer = initiator_id.dh_public();
        let mut init = HybridInitiator::new(initiator_id, &responder_id.dh_public()).unwrap();
        let mut resp = HybridResponder::new(responder_id);
        let msg1 = init.write_message_1(b"").unwrap();
        resp.read_message_1(&msg1).unwrap();
        let (msg2, keys_r) = resp.write_message_2(b"").unwrap();
        let (_, keys_i) = init.read_message_2(&msg2).unwrap();

        let mut send_ctr = 0u64;
        let mut recv_ctr = 0u64;
        let env = seal_text_envelope(
            &keys_i.k_i2r,
            &sender_peer,
            "hello OMCP",
            1_700_000_000_000,
            &mut send_ctr,
            8,
        )
        .unwrap();
        assert_eq!(env.header_aad, vec![CONTENT_TYPE_TEXT_PLAIN]);
        let text = open_text_envelope(&keys_r.k_i2r, &sender_peer, &env, &mut recv_ctr).unwrap();
        assert_eq!(text, "hello OMCP");
        assert_eq!(CONTENT_TYPE_TEXT_PLAIN, 0x01);
        assert_eq!(APP_AEAD_HKDF_INFO, b"OMCP-App-AEAD-v1");
    }
}
