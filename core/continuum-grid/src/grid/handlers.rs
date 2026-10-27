//! Grid command handlers — one function per grid/* command.
//!
//! Each handler takes GridState + params, returns CommandResult.
//! Separated from the ServiceModule impl for testability and clarity.

use continuum_base::time::EpochMs;
use super::audit::{AuditDirection, AuditEntry, AuditOutcome};
use super::frame::{GridFrame, GridPayload};
use super::node::{GridNode, TransportAddress};
use crate::routing::trust_level::TrustLevel;
mod routing;
pub(crate) mod nodes;
pub use nodes::handle_nodes;
pub use routing::handle_route;
use super::GridState;
use crate::runtime::CommandResult;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;

mod handle_status;
pub use handle_status::*;
mod vram_string_to_mb;
pub use vram_string_to_mb::*;

#[cfg(test)]
mod tests;
