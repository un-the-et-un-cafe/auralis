//! `omcp-ffi` — UniFFI bridge over `omcp-core` for the Android (and later iOS)
//! apps. **M5 beta, experimental.**
//!
//! The API is intentionally small and safe: no raw pointers or unsafe code
//! is written here; UniFFI's generated scaffolding is the only FFI glue.
//!
//! # Transport model
//!
//! The platform owns the radio (Android BLE GATT). This crate owns crypto and
//! framing. One [`MeshLink`] per BLE connection turns opaque PDUs written to /
//! notified from the GATT `Inbox` / `Outbox` characteristics (PROTOCOL §8.1)
//! into decrypted messages, and outbound text into PDUs.
//!
//! ## BLE PDU framing (beta convention, not yet in PROTOCOL.md)
//!
//! A single ATT attribute value is at most 512 bytes, but the hybrid Noise
//! handshake messages are > 1 KiB. Each PDU therefore starts with a kind byte:
//!
//! - `0x01 ‖ idx(u8) ‖ total(u8) ‖ chunk` — handshake message chunk
//!   (≤ [`HANDSHAKE_CHUNK`] bytes per chunk).
//! - `0x02 ‖ frame` — one M2 link-AEAD fragment frame
//!   (`fragment_and_seal` with [`LINK_FRAGMENT_PAYLOAD`]-byte payloads).
//!
//! The central (GATT client) is the Noise initiator: it knows which contact it
//! is dialling from the 6-byte PeerID prefix in the advertisement. The
//! peripheral is the responder and only accepts initiators that are already
//! contacts (mutual QR add is required in the beta).

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use omcp_core::ble::fragment::{fragment_and_seal, random_frag_id, FragmentReassembler};
use omcp_core::ble::gossip::SeenCache;
use omcp_core::ble::link::LinkSession;
use omcp_core::ble::message::{open_text_envelope, seal_text_envelope};
use omcp_core::ble::{capabilities, DiscoveryAdvertisement};
use omcp_core::gateway::policy::{GatewayPolicy, NetworkKind, DEFAULT_DAILY_BYTE_CAP};
use omcp_core::{
    backup, recovery, safety, EnvelopeV1, HybridInitiator, HybridResponder, HybridSessionKeys,
    StaticIdentity, SUITE_OMCP_2026_A_NAME,
};
use zeroize::Zeroizing;

uniffi::setup_scaffolding!();

/// Max handshake bytes per PDU chunk.
pub const HANDSHAKE_CHUNK: usize = 480;
/// Link fragment plaintext payload; frame = 2 + 12 + payload + 16 (+1 kind) < 512.
pub const LINK_FRAGMENT_PAYLOAD: usize = 400;
const PDU_HANDSHAKE: u8 = 0x01;
const PDU_LINK: u8 = 0x02;
const QR_PREFIX: &str = "omcp:1:";
const TTL_HOPS: u8 = 8;
const MAX_TEXT_BYTES: usize = 16 * 1024;

/// Errors surfaced to Kotlin.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum FfiError {
    #[error("invalid input: {msg}")]
    Invalid { msg: String },
    #[error("crypto/protocol error: {msg}")]
    Protocol { msg: String },
    #[error("unknown contact")]
    UnknownContact,
    #[error("bad state: {msg}")]
    State { msg: String },
    #[error("wrong backup password")]
    WrongBackupPassword,
    #[error("file too large: {size} bytes exceeds {max}")]
    FileTooLarge { size: u64, max: u64 },
}

impl From<omcp_core::OmcpError> for FfiError {
    fn from(e: omcp_core::OmcpError) -> Self {
        match e {
            omcp_core::OmcpError::FileTooLarge { size, max } => {
                return FfiError::FileTooLarge { size, max }
            }
            omcp_core::OmcpError::BackupAuth => return FfiError::WrongBackupPassword,
            _ => {}
        }
        FfiError::Protocol { msg: e.to_string() }
    }
}

fn invalid(msg: &str) -> FfiError {
    FfiError::Invalid { msg: msg.into() }
}

/// Aggregate path shown in the UI banner (mesh / gateway / mixed / queued only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum PathKind {
    Mesh,
    Gateway,
    Mixed,
    Queued,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PathStatus {
    pub nearby_peers: u32,
    pub queued_count: u32,
    pub path: PathKind,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct IdentityInfo {
    pub peer_id_hex: String,
    pub display_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ContactInfo {
    /// Hex X25519 static public key (the full PeerID in v0.1 beta).
    pub contact_id: String,
    pub display_name: String,
    /// 12 groups of 5 digits.
    pub safety_number: Vec<String>,
    pub verified: bool,
}

/// Result of scanning a QR before the contact is saved (§5.22 preview sheet).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ContactPreview {
    pub contact: ContactInfo,
    /// Same PeerID already saved.
    pub already_known: bool,
    /// A saved contact with the same name has a *different* key — safety number changed.
    pub mismatch_with: Option<ContactInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ReceivedText {
    pub contact_id: String,
    pub text: String,
    pub created_ms: u64,
    pub envelope_id_hex: String,
}

/// Result of feeding a link: PDUs to transmit (in order) + decrypted messages.
#[derive(Debug, Clone, Default, PartialEq, Eq, uniffi::Record)]
pub struct LinkOutput {
    pub outgoing: Vec<Vec<u8>>,
    pub received: Vec<ReceivedText>,
    pub established: bool,
    pub peer_contact_id: Option<String>,
    /// Number of queued texts flushed onto this link by this call.
    pub flushed: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Network {
    Offline,
    Wifi { metered: bool },
    Cellular { metered: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct GatewayDecision {
    pub bridge: bool,
    pub metered_warning: bool,
}

/// Generate a fresh 24-word recovery phrase (show once, then discard).
#[uniffi::export]
pub fn generate_recovery_phrase() -> Result<String, FfiError> {
    let seed = recovery::generate_seed()?;
    Ok(recovery::seed_to_phrase(&seed)?.to_string())
}

/// True if `phrase` is a valid 24-word recovery phrase.
#[uniffi::export]
pub fn validate_recovery_phrase(phrase: String) -> bool {
    recovery::phrase_to_seed(&phrase).is_ok()
}

/// Max file size in bytes (decimal 50 MB) — UI reads this, never hard-codes it.
#[uniffi::export]
pub fn max_file_bytes() -> u64 {
    omcp_core::files::MAX_FILE_BYTES
}

/// Check a file size before any encryption/chunking.
#[uniffi::export]
pub fn check_file_size(size: u64) -> Result<(), FfiError> {
    Ok(omcp_core::files::check_file_size(size)?)
}

/// Is `word` in the recovery word list?
#[uniffi::export]
pub fn is_recovery_word(word: String) -> bool {
    recovery::is_recovery_word(&word)
}

/// Up to 3 autocomplete suggestions for a recovery-word prefix.
#[uniffi::export]
pub fn suggest_recovery_words(prefix: String) -> Vec<String> {
    recovery::suggest_words(&prefix, 3)
        .into_iter()
        .map(String::from)
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PasswordCheck {
    pub ok: bool,
    pub score: u8,
    pub too_short: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BackupInfo {
    pub created_at_ms: u64,
    pub device_hint: String,
    pub needs_password: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BackupContact {
    pub contact_id: String,
    pub display_name: String,
    pub verified: bool,
}

/// Decrypted backup: the phrase (to load an [`OmcpNode`]) + contacts to restore.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct RestoredBackup {
    pub phrase: String,
    pub contacts: Vec<BackupContact>,
}

/// Backup password policy: ≥ 8 chars and zxcvbn score ≥ 3.
#[uniffi::export]
pub fn check_backup_password(password: String) -> PasswordCheck {
    let c = backup::check_password(&password);
    PasswordCheck {
        ok: c.ok,
        score: c.score,
        too_short: c.too_short,
    }
}

/// Read unencrypted blob metadata (for the restore-on-reinstall prompt).
#[uniffi::export]
pub fn backup_info(blob: Vec<u8>) -> Result<BackupInfo, FfiError> {
    let i = backup::backup_info(&blob)?;
    Ok(BackupInfo {
        created_at_ms: i.created_at,
        device_hint: i.device_hint,
        needs_password: i.needs_password,
    })
}

/// Decrypt a password backup. Wrong password → `WrongBackupPassword`.
#[uniffi::export]
pub fn open_backup_with_password(
    blob: Vec<u8>,
    password: String,
) -> Result<RestoredBackup, FfiError> {
    let c = backup::open_backup(&blob, backup::BackupSecret::Password(&password))?;
    Ok(RestoredBackup {
        phrase: recovery::seed_to_phrase(&c.entropy)?.to_string(),
        contacts: c
            .contacts
            .into_iter()
            .map(|b| BackupContact {
                contact_id: hex::encode(b.peer_id),
                display_name: b.name,
                verified: b.verified,
            })
            .collect(),
    })
}

/// Product default: 500 MB/day (decimal).
#[uniffi::export]
pub fn default_gateway_daily_cap_bytes() -> u64 {
    DEFAULT_DAILY_BYTE_CAP
}

/// Evaluate gateway policy: Wi-Fi-only default; metered warns, never blocks.
#[uniffi::export]
pub fn gateway_decision(wifi_only: bool, network: Network) -> GatewayDecision {
    let policy = GatewayPolicy {
        wifi_only,
        ..GatewayPolicy::default()
    };
    let net = match network {
        Network::Offline => NetworkKind::Offline,
        Network::Wifi { metered } => NetworkKind::Wifi { metered },
        Network::Cellular { metered } => NetworkKind::Cellular { metered },
    };
    let d = policy.evaluate(net);
    GatewayDecision {
        bridge: d.bridge,
        metered_warning: d.metered_warning,
    }
}

/// Suite label for the Security centre.
#[uniffi::export]
pub fn suite_label() -> String {
    SUITE_OMCP_2026_A_NAME.to_string()
}

/// Parse a 13-byte discovery advertisement; returns the 6-byte PeerID prefix hex.
#[uniffi::export]
pub fn parse_advertisement(bytes: Vec<u8>) -> Option<String> {
    DiscoveryAdvertisement::decode(&bytes)
        .ok()
        .map(|a| hex::encode(a.peer_id_trunc))
}

struct Contact {
    name: String,
    verified: bool,
}

struct NodeState {
    contacts: HashMap<[u8; 32], Contact>,
    queues: HashMap<[u8; 32], VecDeque<String>>,
    links_up: HashMap<u64, [u8; 32]>,
    next_link: u64,
    seen: SeenCache,
}

struct NodeInner {
    seed: Zeroizing<[u8; 32]>,
    peer_id: [u8; 32],
    name: String,
    state: Mutex<NodeState>,
}

impl NodeInner {
    fn lock(&self) -> MutexGuard<'_, NodeState> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }
    fn identity(&self) -> Result<StaticIdentity, FfiError> {
        Ok(StaticIdentity::from_seed(&self.seed)?)
    }
}

fn parse_id(hex_id: &str) -> Result<[u8; 32], FfiError> {
    let v = hex::decode(hex_id).map_err(|_| invalid("contact id hex"))?;
    v.try_into().map_err(|_| invalid("contact id length"))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Local node: identity, contacts, offline queue.
#[derive(uniffi::Object)]
pub struct OmcpNode {
    inner: Arc<NodeInner>,
}

#[uniffi::export]
impl OmcpNode {
    /// Load (or restore) the identity from its 24-word recovery phrase.
    #[uniffi::constructor]
    pub fn from_phrase(phrase: String, display_name: String) -> Result<Arc<Self>, FfiError> {
        let seed = recovery::phrase_to_seed(&phrase)?;
        let id = StaticIdentity::from_seed(&seed)?;
        Ok(Arc::new(Self {
            inner: Arc::new(NodeInner {
                seed,
                peer_id: id.dh_public(),
                name: display_name,
                state: Mutex::new(NodeState {
                    contacts: HashMap::new(),
                    queues: HashMap::new(),
                    links_up: HashMap::new(),
                    next_link: 1,
                    seen: SeenCache::default(),
                }),
            }),
        }))
    }

    pub fn identity(&self) -> IdentityInfo {
        IdentityInfo {
            peer_id_hex: hex::encode(self.inner.peer_id),
            display_name: self.inner.name.clone(),
        }
    }

    /// The 24-word phrase (Settings › Backup › View phrase; gate behind device auth).
    pub fn recovery_phrase(&self) -> Result<String, FfiError> {
        Ok(recovery::seed_to_phrase(&self.inner.seed)?.to_string())
    }

    /// Encrypted backup blob (identity + contacts + verification state).
    /// The plaintext phrase never leaves this function unencrypted.
    pub fn create_backup(
        &self,
        password: String,
        device_hint: String,
        created_at_ms: u64,
    ) -> Result<Vec<u8>, FfiError> {
        let contacts = self
            .inner
            .lock()
            .contacts
            .iter()
            .map(|(id, c)| backup::BackupContact {
                peer_id: *id,
                name: c.name.clone(),
                verified: c.verified,
            })
            .collect();
        let contents = backup::BackupContents {
            entropy: Zeroizing::new(*self.inner.seed),
            contacts,
        };
        Ok(backup::seal_backup(
            &contents,
            backup::BackupSecret::Password(&password),
            created_at_ms,
            &device_hint,
        )?)
    }

    /// Contact QR payload: `omcp:1:<peer id hex>:<display name>`.
    pub fn qr_payload(&self) -> String {
        format!(
            "{QR_PREFIX}{}:{}",
            hex::encode(self.inner.peer_id),
            self.inner.name
        )
    }

    /// 13-byte BLE discovery advertisement (PROTOCOL §8.2).
    pub fn advertisement(&self) -> Vec<u8> {
        DiscoveryAdvertisement::new(1, &self.inner.peer_id, capabilities::PQ)
            .encode()
            .to_vec()
    }

    /// Parse a QR and compute its safety number without saving anything.
    pub fn preview_contact_from_qr(&self, payload: String) -> Result<ContactPreview, FfiError> {
        let (id, name) = self.parse_qr(&payload)?;
        let st = self.inner.lock();
        let known = st.contacts.get(&id);
        let mismatch_with = st
            .contacts
            .iter()
            .find(|(k, c)| **k != id && c.name == name)
            .map(|(k, c)| self.info(k, c));
        let contact = match known {
            Some(c) => self.info(&id, c),
            None => self.info(
                &id,
                &Contact {
                    name,
                    verified: false,
                },
            ),
        };
        Ok(ContactPreview {
            contact,
            already_known: known.is_some(),
            mismatch_with,
        })
    }

    /// Add a contact from a scanned QR payload.
    pub fn add_contact_from_qr(&self, payload: String) -> Result<ContactInfo, FfiError> {
        let (id, name) = self.parse_qr(&payload)?;
        let mut st = self.inner.lock();
        let c = st.contacts.entry(id).or_insert(Contact {
            name,
            verified: false,
        });
        Ok(self.info(&id, c))
    }
}

impl OmcpNode {
    fn parse_qr(&self, payload: &str) -> Result<([u8; 32], String), FfiError> {
        let rest = payload
            .trim()
            .strip_prefix(QR_PREFIX)
            .ok_or_else(|| invalid("not an OMCP contact QR"))?;
        let (hex_id, name) = rest.split_once(':').unwrap_or((rest, ""));
        let id = parse_id(hex_id)?;
        if id == self.inner.peer_id {
            return Err(invalid("cannot add yourself"));
        }
        let name = if name.is_empty() {
            format!("Peer {}", &hex_id[..8])
        } else {
            name.chars().take(64).collect()
        };
        Ok((id, name))
    }
}

#[uniffi::export]
impl OmcpNode {
    /// Restore a contact from app storage (no QR parse).
    pub fn restore_contact(
        &self,
        contact_id: String,
        display_name: String,
        verified: bool,
    ) -> Result<(), FfiError> {
        let id = parse_id(&contact_id)?;
        self.inner.lock().contacts.insert(
            id,
            Contact {
                name: display_name,
                verified,
            },
        );
        Ok(())
    }

    pub fn contacts(&self) -> Vec<ContactInfo> {
        let st = self.inner.lock();
        let mut v: Vec<_> = st.contacts.iter().map(|(id, c)| self.info(id, c)).collect();
        v.sort_by(|a, b| a.display_name.cmp(&b.display_name));
        v
    }

    pub fn set_verified(&self, contact_id: String, verified: bool) -> Result<(), FfiError> {
        let id = parse_id(&contact_id)?;
        let mut st = self.inner.lock();
        let c = st.contacts.get_mut(&id).ok_or(FfiError::UnknownContact)?;
        c.verified = verified;
        Ok(())
    }

    /// Contact whose PeerID starts with this 6-byte prefix (from an advertisement).
    pub fn contact_for_prefix(&self, prefix_hex: String) -> Option<String> {
        let st = self.inner.lock();
        st.contacts
            .keys()
            .find(|id| hex::encode(&id[..6]) == prefix_hex)
            .map(hex::encode)
    }

    /// Queue a text for a contact; it is sent when a link to them is up.
    pub fn queue_text(&self, contact_id: String, text: String) -> Result<(), FfiError> {
        if text.len() > MAX_TEXT_BYTES {
            return Err(invalid("text too long"));
        }
        let id = parse_id(&contact_id)?;
        let mut st = self.inner.lock();
        if !st.contacts.contains_key(&id) {
            return Err(FfiError::UnknownContact);
        }
        st.queues.entry(id).or_default().push_back(text);
        Ok(())
    }

    pub fn queued_for(&self, contact_id: String) -> u32 {
        let Ok(id) = parse_id(&contact_id) else {
            return 0;
        };
        self.inner
            .lock()
            .queues
            .get(&id)
            .map_or(0, |q| q.len() as u32)
    }

    /// Aggregate path status for the banner. Gateway transport is not wired in
    /// the beta, so `Gateway`/`Mixed` are not produced yet.
    pub fn path_status(&self) -> PathStatus {
        let st = self.inner.lock();
        let queued: usize = st.queues.values().map(VecDeque::len).sum();
        let mut peers: Vec<_> = st.links_up.values().collect();
        peers.sort();
        peers.dedup();
        PathStatus {
            nearby_peers: peers.len() as u32,
            queued_count: queued as u32,
            path: if queued == 0 && !peers.is_empty() {
                PathKind::Mesh
            } else {
                PathKind::Queued
            },
        }
    }

    /// New link as Noise initiator (we are the GATT central dialling a contact).
    pub fn open_initiator_link(&self, contact_id: String) -> Result<Arc<MeshLink>, FfiError> {
        let id = parse_id(&contact_id)?;
        if !self.inner.lock().contacts.contains_key(&id) {
            return Err(FfiError::UnknownContact);
        }
        let init = HybridInitiator::new(self.inner.identity()?, &id)?;
        Ok(self.new_link(Role::Initiator(Box::new(init)), Some(id)))
    }

    /// New link as Noise responder (we are the GATT peripheral).
    pub fn open_responder_link(&self) -> Result<Arc<MeshLink>, FfiError> {
        let resp = HybridResponder::new(self.inner.identity()?);
        Ok(self.new_link(Role::Responder(Box::new(resp)), None))
    }
}

impl OmcpNode {
    fn info(&self, id: &[u8; 32], c: &Contact) -> ContactInfo {
        ContactInfo {
            contact_id: hex::encode(id),
            display_name: c.name.clone(),
            safety_number: safety::safety_number(&self.inner.peer_id, id),
            verified: c.verified,
        }
    }

    fn new_link(&self, role: Role, peer: Option<[u8; 32]>) -> Arc<MeshLink> {
        let mut st = self.inner.lock();
        let handle = st.next_link;
        st.next_link += 1;
        Arc::new(MeshLink {
            node: self.inner.clone(),
            handle,
            state: Mutex::new(LinkState {
                role,
                peer,
                hs_buf: Vec::new(),
                hs_total: 0,
            }),
        })
    }
}

enum Role {
    Initiator(Box<HybridInitiator>),
    Responder(Box<HybridResponder>),
    Up(Box<Established>),
    Closed,
}

struct Established {
    link: LinkSession,
    keys: HybridSessionKeys,
    initiator: bool,
    reasm: FragmentReassembler,
    app_send: u64,
    app_recv: u64,
}

impl Established {
    fn send_key(&self) -> [u8; 32] {
        if self.initiator {
            self.keys.k_i2r
        } else {
            self.keys.k_r2i
        }
    }
    fn recv_key(&self) -> [u8; 32] {
        if self.initiator {
            self.keys.k_r2i
        } else {
            self.keys.k_i2r
        }
    }
}

struct LinkState {
    role: Role,
    peer: Option<[u8; 32]>,
    hs_buf: Vec<u8>,
    hs_total: u8,
}

/// One BLE connection's secure session.
#[derive(uniffi::Object)]
pub struct MeshLink {
    node: Arc<NodeInner>,
    handle: u64,
    state: Mutex<LinkState>,
}

fn chunk_handshake(msg: &[u8], out: &mut Vec<Vec<u8>>) -> Result<(), FfiError> {
    let chunks: Vec<_> = msg.chunks(HANDSHAKE_CHUNK).collect();
    let total = u8::try_from(chunks.len()).map_err(|_| invalid("handshake too large"))?;
    for (i, c) in chunks.iter().enumerate() {
        let mut p = Vec::with_capacity(3 + c.len());
        p.extend_from_slice(&[PDU_HANDSHAKE, i as u8, total]);
        p.extend_from_slice(c);
        out.push(p);
    }
    Ok(())
}

#[uniffi::export]
impl MeshLink {
    /// Initiator: emit handshake message 1. Responder: no-op.
    pub fn start(&self) -> Result<LinkOutput, FfiError> {
        let mut st = self.lock();
        let mut out = LinkOutput::default();
        if let Role::Initiator(init) = &mut st.role {
            let m1 = init.write_message_1(b"omcp-ffi-v1")?;
            chunk_handshake(&m1, &mut out.outgoing)?;
        }
        Ok(out)
    }

    /// Feed one PDU received from the peer (Inbox write or Outbox notify).
    pub fn on_pdu(&self, pdu: Vec<u8>) -> Result<LinkOutput, FfiError> {
        let mut st = self.lock();
        let mut out = LinkOutput::default();
        let (&kind, body) = pdu.split_first().ok_or_else(|| invalid("empty pdu"))?;
        match kind {
            PDU_HANDSHAKE => {
                if body.len() < 2 {
                    return Err(invalid("handshake pdu"));
                }
                let (idx, total) = (body[0], body[1]);
                if idx == 0 {
                    st.hs_buf.clear();
                    st.hs_total = total;
                } else if total != st.hs_total {
                    return Err(invalid("handshake chunk mismatch"));
                }
                st.hs_buf.extend_from_slice(&body[2..]);
                if idx + 1 == total {
                    let msg = std::mem::take(&mut st.hs_buf);
                    self.complete_handshake(&mut st, &msg, &mut out)?;
                }
            }
            PDU_LINK => {
                let peer = st.peer;
                let Role::Up(up) = &mut st.role else {
                    return Err(FfiError::State {
                        msg: "link fragment before handshake".into(),
                    });
                };
                let peer = peer.ok_or(FfiError::UnknownContact)?;
                if let Some(bytes) = up.reasm.push_frame(&mut up.link, body, Instant::now())? {
                    let env = EnvelopeV1::decode(&bytes)?;
                    let key = up.recv_key();
                    let text = open_text_envelope(&key, &peer, &env, &mut up.app_recv)?;
                    let dup = self
                        .node
                        .lock()
                        .seen
                        .check_and_insert(env.envelope_id, Instant::now());
                    if !dup {
                        out.received.push(ReceivedText {
                            contact_id: hex::encode(peer),
                            text,
                            created_ms: env.created_ms,
                            envelope_id_hex: hex::encode(env.envelope_id),
                        });
                    }
                }
            }
            _ => return Err(invalid("unknown pdu kind")),
        }
        Self::fill(&st, &mut out);
        Ok(out)
    }

    /// Send text now if established, otherwise queue it on the node.
    pub fn send_text(&self, text: String) -> Result<LinkOutput, FfiError> {
        let mut st = self.lock();
        let mut out = LinkOutput::default();
        let peer = st.peer.ok_or(FfiError::UnknownContact)?;
        match &mut st.role {
            Role::Up(up) => {
                Self::seal_into(&self.node, up, &text, &mut out)?;
            }
            _ => {
                drop(st);
                return OmcpNode {
                    inner: self.node.clone(),
                }
                .queue_text(hex::encode(peer), text)
                .map(|_| LinkOutput::default());
            }
        }
        Self::fill(&st, &mut out);
        Ok(out)
    }

    /// Flush any queued texts for this peer (e.g. after queueing elsewhere).
    pub fn flush(&self) -> Result<LinkOutput, FfiError> {
        let mut st = self.lock();
        let mut out = LinkOutput::default();
        self.flush_locked(&mut st, &mut out)?;
        Self::fill(&st, &mut out);
        Ok(out)
    }

    pub fn is_established(&self) -> bool {
        matches!(self.lock().role, Role::Up(_))
    }

    /// Tear down; the peer no longer counts as nearby.
    pub fn shutdown(&self) {
        self.lock().role = Role::Closed;
        self.node.lock().links_up.remove(&self.handle);
    }
}

impl MeshLink {
    fn lock(&self) -> MutexGuard<'_, LinkState> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn fill(st: &LinkState, out: &mut LinkOutput) {
        out.established = matches!(st.role, Role::Up(_));
        out.peer_contact_id = st.peer.map(hex::encode);
    }

    fn seal_into(
        node: &NodeInner,
        up: &mut Established,
        text: &str,
        out: &mut LinkOutput,
    ) -> Result<(), FfiError> {
        let env = seal_text_envelope(
            &up.send_key(),
            &node.peer_id,
            text,
            now_ms(),
            &mut up.app_send,
            TTL_HOPS,
        )?;
        // Remember our own envelope so a gossip echo is not shown as received.
        node.lock()
            .seen
            .check_and_insert(env.envelope_id, Instant::now());
        let bytes = env.encode()?;
        for f in fragment_and_seal(
            &mut up.link,
            &bytes,
            random_frag_id()?,
            LINK_FRAGMENT_PAYLOAD,
        )? {
            let mut p = Vec::with_capacity(1 + f.len());
            p.push(PDU_LINK);
            p.extend_from_slice(&f);
            out.outgoing.push(p);
        }
        Ok(())
    }

    fn flush_locked(&self, st: &mut LinkState, out: &mut LinkOutput) -> Result<(), FfiError> {
        let (Some(peer), Role::Up(up)) = (st.peer, &mut st.role) else {
            return Ok(());
        };
        let pending: Vec<String> = self
            .node
            .lock()
            .queues
            .remove(&peer)
            .map(Vec::from)
            .unwrap_or_default();
        for t in &pending {
            Self::seal_into(&self.node, up, t, out)?;
            out.flushed += 1;
        }
        Ok(())
    }

    fn complete_handshake(
        &self,
        st: &mut LinkState,
        msg: &[u8],
        out: &mut LinkOutput,
    ) -> Result<(), FfiError> {
        let role = std::mem::replace(&mut st.role, Role::Closed);
        let up = match role {
            Role::Initiator(mut init) => {
                let (_payload, keys) = init.read_message_2(msg)?;
                Established {
                    link: LinkSession::initiator(&keys),
                    keys,
                    initiator: true,
                    reasm: FragmentReassembler::default(),
                    app_send: 0,
                    app_recv: 0,
                }
            }
            Role::Responder(mut resp) => {
                resp.read_message_1(msg)?;
                let rs = resp.remote_static().ok_or(FfiError::UnknownContact)?;
                if !self.node.lock().contacts.contains_key(&rs) {
                    // Unknown initiator: refuse (beta requires mutual QR add).
                    return Err(FfiError::UnknownContact);
                }
                let (m2, keys) = resp.write_message_2(b"omcp-ffi-v1")?;
                chunk_handshake(&m2, &mut out.outgoing)?;
                st.peer = Some(rs);
                Established {
                    link: LinkSession::responder(&keys),
                    keys,
                    initiator: false,
                    reasm: FragmentReassembler::default(),
                    app_send: 0,
                    app_recv: 0,
                }
            }
            other => {
                st.role = other;
                return Err(FfiError::State {
                    msg: "unexpected handshake message".into(),
                });
            }
        };
        st.role = Role::Up(Box::new(up));
        if let Some(p) = st.peer {
            self.node.lock().links_up.insert(self.handle, p);
        }
        self.flush_locked(st, out)
    }
}

#[cfg(test)]
mod tests;
