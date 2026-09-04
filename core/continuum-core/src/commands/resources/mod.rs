//! `resources/<verb>` — read surface over the one per-machine resource authority.
//!
//! The [`ResourceDaemon`](crate::resources::ResourceDaemon) is the single authority
//! over VRAM/RAM/disk/ports (#56); it owns the accounting board and refreshes each
//! consumer's measured footprint on its background tick. These commands are the
//! *read* side of that authority — snapshots, never mutations. The daemon (or a
//! higher arbiter) still owns every grant/reclaim/capacity decision; a command here
//! only observes. The owning [`ResourcesModule`](crate::modules::resources_module)
//! wraps the `Arc<ResourceDaemon>` and contributes these objects in `commands()`.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::resources::ResourceDaemon;
use crate::sdk_codegen::DynCommand;

pub mod board;

use board::ResourcesBoard;

/// Empty input contract shared across the `resources/*` read verbs — each reports the
/// machine's live state and takes no parameters. One contract rather than a placeholder
/// struct per verb (compression principle), mirroring `system::SystemQuery`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/resources/ResourcesQuery.ts"
)]
pub struct ResourcesQuery {}

/// The dep-holding `resources/*` command objects the
/// [`ResourcesModule`](crate::modules::resources_module) contributes to the typed
/// object map, each capturing the shared `Arc<ResourceDaemon>`.
pub fn command_objects(daemon: Arc<ResourceDaemon>) -> Vec<Arc<dyn DynCommand>> {
    vec![Arc::new(ResourcesBoard { daemon })]
}
