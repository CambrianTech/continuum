//! The driver gateway: one door every external agent calls (docs/architecture/DRIVER-GATEWAY.md).
//!
//! Claude Code, Codex CLI, Grok Build, Hermes and opencode each speak one of three APIs.
//! Each API is an adapter here holding only its diff ([`wire`]): it decodes the agent's body
//! into the engine's one request and encodes the engine's stream back. Selection and
//! generation go through the same adapter registry the residents use ([`route`]). Tools
//! pass straight through: the agent runs its own tools.
//!
//! Slices G0-G1. A session becomes a driver mind seated by `MindMemory` in G2; until then a
//! call is admitted as anonymous traffic.
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
#![deny(clippy::await_holding_lock, clippy::significant_drop_in_scrutinee, clippy::let_underscore_must_use)] // clean on 2026-11-08 (#5858 CI); CI's clippy step enforces it

mod answer;
mod listener;
mod module;
mod route;
mod server;
mod sever;
mod session;
mod stop;
mod wire;

pub use module::GatewayModule;
