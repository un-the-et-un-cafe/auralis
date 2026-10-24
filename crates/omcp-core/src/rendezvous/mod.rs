//! Optional account-free rendezvous (PROTOCOL §9.2) — **experimental (M4)**.
//!
//! - [`identity::RendezvousIdentity`]: Ed25519 token-signing key + X25519
//!   sealed-sender key.
//! - [`label`]: blinded destination labels (what gateways/rendezvous see instead
//!   of PeerIDs).
//! - [`sealed`]: sealed-sender box so gateways only learn the blinded label.
//! - [`token`]: short-lived signed tokens for `POST /v1/enqueue` / `POST /v1/fetch`.
//! - [`server`]: in-memory reference server + client transport.
//!
//! No network server is shipped; the wire types are transport-agnostic byte
//! encodings that a TLS 1.3 / QUIC HTTP binding would carry as request bodies.

pub mod identity;
pub mod label;
pub mod sealed;
pub mod server;
pub mod token;

pub use identity::RendezvousIdentity;
pub use label::{blinded_label, epoch_for, BlindedLabel, LABEL_EPOCH_MS};
pub use server::{
    EnqueueRequest, FetchRequest, InMemoryRendezvous, RendezvousServer, RendezvousTransport,
    PATH_ENQUEUE, PATH_FETCH,
};
pub use token::{RendezvousToken, TokenOp, MAX_TOKEN_LIFETIME_MS};
