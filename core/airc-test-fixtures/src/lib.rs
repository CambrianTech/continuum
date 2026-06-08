//! airc-test-fixtures — shared test fixtures for integration tests that
//! pair a continuum substrate against a real `airc_lib::Airc` peer.
//!
//! ## Why this crate exists
//!
//! The substrate's `command_handler` (server side) and
//! `continuum-client::AircIpcTransport` (client side) speak the same
//! `continuum-airc-protocol` envelopes. Unit tests of each end prove
//! the parsing surface in isolation. They do NOT prove that an
//! envelope serialized by the client end-to-end deserializes correctly
//! at the server end after airc-lib's CBOR framing, header rewrites,
//! correlation_id stamping, and deadline negotiation. That gap was
//! flagged by adversarial reviewer 1 on PR #1557.
//!
//! `TwoAircLoopback` closes the gap. It spins up two `Arc<airc_lib::Airc>`
//! peers wired together over a real loopback transport so integration
//! tests can do a full client→server→client roundtrip in-process.
//!
//! ## Scope
//!
//! Substrate-internal test code only. Never imported by any production
//! binary; the crate is consumed via `[dev-dependencies]` in
//! `continuum-core` and `continuum-client` integration test targets.

pub mod two_airc_loopback;

pub use two_airc_loopback::{LoopbackError, TwoAircLoopback};
