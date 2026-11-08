//! The proving ground's data shapes, each defined once (docs/architecture/THE-PROVING-GROUND.md 4a,
//! 8a). The T0/T1 harness, the grid world and the reducer all read these, so no shape is defined
//! twice. Data only: serde, no logic, no clock, no I/O.
//!
//! - [`closed`]: the only strings a file may hold (P9's text check, enforced by type).
//! - [`provenance`]: recorded or unavailable, joined how, measured when and how well.
//! - [`reduction`]: one act without its words.
//! - [`profile`]: what is measured on a node.
//! - [`scenario`]: a world, a workload, events, a policy and what passes.
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod closed;
pub mod profile;
pub mod provenance;
pub mod reduction;
pub mod scenario;
