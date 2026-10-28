//! Brand-independent OMCP compatibility guards for the Auralis client.
//!
//! Golden values are computed independently of the Rust encoders: envelope
//! bytes follow PROTOCOL §10/§21; hash vectors use Python hashlib/hmac with
//! explicit digest sizes and big-endian integers. These are regression guards,
//! not proof of full protocol compliance or cross-device interoperability.

use omcp_core::ble::{capabilities, DiscoveryAdvertisement};
use omcp_core::burn::{
    decode_app, encode_app, AppContent, BurnHeader, BurnMode, MediaControl, OpenedReceipt,
    CONTROL_MEDIA_REQUEST, CONTROL_MEDIA_RESEND_ASK,
};
use omcp_core::device::{peer_id_from_identity, DeviceCert, DEVICE_CERT_LEN};
use omcp_core::framing::{flags, EnvelopeV1, MAX_CIPHERTEXT_LEN};
use omcp_core::negotiate::{negotiate, Hello};
use omcp_core::sync::hello::{digest_id_of_bytes, LinkControl};
use omcp_core::sync::tag::{mailbox_tag, rendezvous_label, subtag};
use omcp_core::OmcpError;

const ENVELOPE_HEX: &str = concat!(
    "4f4d435000010001000102030405060708090a0b0c0d0e0f",
    "000708010203040506070800010200000003aabbcc"
);
const EPOCH: u64 = 0x0102_0304_0506_0708;

fn envelope() -> EnvelopeV1 {
    EnvelopeV1 {
        version: 1,
        suite_id: 1,
        envelope_id: std::array::from_fn(|i| i as u8),
        flags: flags::RELAY_OK | flags::STORE_OK | flags::PRIORITY,
        ttl_hops: 8,
        created_ms: EPOCH,
        header_aad: vec![0x02],
        ciphertext: vec![0xaa, 0xbb, 0xcc],
    }
}

#[test]
fn envelope_matches_independent_big_endian_golden_vector() {
    // Framing deliberately treats these three bytes as opaque; they are not
    // an authenticated application ciphertext and must never be decrypted.
    let bytes = hex::decode(ENVELOPE_HEX).unwrap();
    assert_eq!(bytes.len(), 45);
    assert_eq!(envelope().encode().unwrap(), bytes);
    assert_eq!(EnvelopeV1::decode(&bytes).unwrap(), envelope());
}

#[test]
fn envelope_receiver_accepts_reserved_flags_without_changing_known_bits() {
    // §21.3 says ignore reserved bits on receive, not reject the envelope.
    let mut bytes = hex::decode(ENVELOPE_HEX).unwrap();
    bytes[24..26].copy_from_slice(&0x8007u16.to_be_bytes());
    let decoded = EnvelopeV1::decode(&bytes).unwrap();
    assert_eq!(decoded.flags & 0x0007, 7);
    assert_eq!(decoded.ciphertext, envelope().ciphertext);
}

#[test]
fn envelope_rejects_every_truncated_prefix_and_trailing_bytes() {
    let mut bytes = hex::decode(ENVELOPE_HEX).unwrap();
    for end in 0..bytes.len() {
        assert!(EnvelopeV1::decode(&bytes[..end]).is_err(), "prefix {end}");
    }
    bytes.push(0);
    assert!(EnvelopeV1::decode(&bytes).is_err());
}

#[test]
fn ciphertext_limit_is_inclusive_and_checked_before_body_allocation() {
    let mut env = envelope();
    env.ciphertext = vec![0; MAX_CIPHERTEXT_LEN as usize];
    let encoded = env.encode().unwrap();
    assert_eq!(EnvelopeV1::decode(&encoded).unwrap(), env);
    env.ciphertext.push(0);
    assert!(matches!(env.encode(), Err(OmcpError::Oversized { .. })));

    let mut short = hex::decode(ENVELOPE_HEX).unwrap();
    // Fixed fields end at byte 37, then the one-byte header_aad.
    short[38..42].copy_from_slice(&(MAX_CIPHERTEXT_LEN + 1).to_be_bytes());
    assert!(matches!(
        EnvelopeV1::decode(&short),
        Err(OmcpError::Oversized { .. })
    ));
}

#[test]
fn header_aad_length_uses_the_full_u16_range() {
    let mut env = envelope();
    env.header_aad = vec![0; u16::MAX as usize];
    assert_eq!(EnvelopeV1::decode(&env.encode().unwrap()).unwrap(), env);
    env.header_aad.push(0);
    assert!(matches!(
        env.encode(),
        Err(OmcpError::HeaderAadLength { .. })
    ));
}

#[test]
fn discovery_magic_layout_and_capability_bits_are_brand_independent() {
    let id = std::array::from_fn(|i| i as u8);
    let ad = DiscoveryAdvertisement::new(1, &id, capabilities::PQ);
    assert_eq!(hex::encode(ad.encode()), "4f4d4350000100010203040508");
    assert_eq!(DiscoveryAdvertisement::decode(&ad.encode()).unwrap(), ad);
    assert_eq!(capabilities::GATEWAY, 1);
    assert_eq!(capabilities::STORE_FORWARD, 2);
    assert_eq!(capabilities::MLS, 4);
    assert_eq!(capabilities::PQ, 8);
}

#[test]
fn peer_id_uses_blake2b_256_not_truncated_blake2b_512() {
    let public_key = std::array::from_fn(|i| i as u8);
    assert_eq!(
        hex::encode(peer_id_from_identity(&public_key)),
        "cb2f5160fc1f7e05a55ef49d340b48da2e5a78099d53393351cd579dd42503d6"
    );
}

#[test]
fn device_certificate_v2_has_fixed_fields_and_rejects_tampering() {
    let cert = DeviceCert::issue(&[1; 32], [2; 32], [3; 32], EPOCH, Some(7)).unwrap();
    let mut bytes = cert.encode();
    assert_eq!(DEVICE_CERT_LEN, 209);
    assert_eq!(bytes.len(), 209);
    assert_eq!(bytes[0], 2);
    assert_eq!(&bytes[65..97], &[2; 32]);
    assert_eq!(&bytes[97..129], &[3; 32]);
    assert_eq!(&bytes[129..137], &EPOCH.to_be_bytes());
    assert_eq!(&bytes[137..145], &7u64.to_be_bytes());
    assert_eq!(DeviceCert::decode(&bytes).unwrap(), cert);
    bytes[97] ^= 1;
    assert!(DeviceCert::decode(&bytes).is_err());
}

#[test]
fn burn_inner_header_matches_golden_vector_and_rejects_invalid_values() {
    let burn = BurnHeader::timer(60, 86_400).unwrap();
    let content = AppContent::Text("hi".into());
    let bytes = encode_app(Some(&burn), &content).unwrap();
    assert_eq!(hex::encode(&bytes), "01010000003c00015180016869");
    assert_eq!(decode_app(&bytes).unwrap(), (Some(burn), content));
    assert!(BurnHeader::new(BurnMode::OnOpen, 1, 86_400).is_err());
    assert!(BurnHeader::timer(11, 86_400).is_err());
    assert!(BurnHeader::on_open(86_401).is_err());
}

#[test]
fn opened_and_media_control_messages_cannot_carry_a_burn_header() {
    let burn = BurnHeader::on_open(3_600).unwrap();
    let opened = AppContent::Opened(OpenedReceipt {
        msg_id: [1; 16],
        opened_ms: EPOCH,
    });
    let request =
        AppContent::MediaControl(MediaControl::new(CONTROL_MEDIA_REQUEST, [2; 16], [3; 32]));
    for content in [opened, request] {
        assert!(encode_app(Some(&burn), &content).is_err());
        let body = encode_app(None, &content).unwrap();
        assert_eq!(decode_app(&body).unwrap(), (None, content));
    }
}

#[test]
fn resend_note_limit_is_utf8_bytes_and_legacy_note_less_form_remains_valid() {
    let mut request = MediaControl::new(CONTROL_MEDIA_RESEND_ASK, [1; 16], [2; 32]);
    assert_eq!(request.encode().len(), 49);
    assert_eq!(MediaControl::decode(&request.encode()).unwrap(), request);
    request.note = Some("é".repeat(140)); // 140 characters, exactly 280 UTF-8 bytes.
    assert_eq!(request.encode().len(), 329);
    assert_eq!(MediaControl::decode(&request.encode()).unwrap(), request);
    let mut too_long = request.encode();
    too_long.push(b'!');
    assert!(MediaControl::decode(&too_long).is_err());
}

#[test]
fn sync_tags_and_rendezvous_label_match_independent_vectors() {
    let secret: [u8; 32] = std::array::from_fn(|i| i as u8);
    assert_eq!(
        hex::encode(mailbox_tag(&secret, EPOCH).unwrap()),
        "8f31e2d5ea5d18e1"
    );
    assert_eq!(
        hex::encode(subtag(&secret, 0x1020_3040, EPOCH).unwrap()),
        "3250b16134b0f9b2"
    );
    assert_eq!(
        hex::encode(rendezvous_label(&secret, EPOCH).unwrap()),
        "8697c9f3aa0f117c4677cbe220219d953f96d4309efc4bed774a21c4eee485bb"
    );
}

#[test]
fn beacon_layout_and_digest_use_the_pinned_32_bit_blake2b_output() {
    assert_eq!(digest_id_of_bytes(b"OMCP"), 0x6f0d_fe8c);
    let hello = LinkControl::Hello {
        has_delta: true,
        digest_id: 0x6f0d_fe8c,
    };
    assert_eq!(hello.encode(), [0x48, 1, 0x6f, 0x0d, 0xfe, 0x8c]);
    assert_eq!(LinkControl::decode(&hello.encode()).unwrap(), hello);
    let mut invalid = hello.encode();
    invalid[1] = 0x80;
    assert!(LinkControl::decode(&invalid).is_err());
    assert_eq!(
        LinkControl::DigestReq {
            has_delta: false,
            digest_id: 0,
        }
        .encode(),
        [0x52, 0, 0, 0, 0, 0]
    );
}

#[test]
fn negotiation_does_not_silently_fallback_to_an_unadvertised_suite() {
    let local = Hello::local(["PQ"]);
    let peer = Hello {
        versions: vec![1],
        suites: vec![0x1234],
        features: vec![],
    };
    assert!(matches!(
        negotiate(&local, &peer),
        Err(OmcpError::NoSuiteOverlap)
    ));
}
