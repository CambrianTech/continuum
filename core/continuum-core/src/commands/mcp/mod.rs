//! `mcp/*` — the MCP tool-catalog verbs, migrated off the legacy
//! `MCPModule::handle_command` match onto the typed
//! [`ActionCommand`](crate::sdk_codegen::ActionCommand) path.
//!
//! All four verbs read the shared tool cache, so they capture the module's
//! `Arc<McpCatalog>` (the [`CodeState`](crate::modules::code::CodeState)
//! dep-holding shape). The owning
//! [`MCPModule`](crate::modules::mcp::MCPModule) builds the cache at
//! `initialize` and contributes these objects via its `commands()`, so they
//! reach `command_registry()`, the persona tool surface, the ACL, codegen, and
//! `uu` — the whole point of the migration (the catalog verbs were previously
//! invisible to every one of those surfaces).
//!
//! - [`McpListTools`](list_tools::McpListTools) (`mcp/list-tools`, AiSafe) — the
//!   full catalog as MCP tool definitions. THE contract `mcp_protocol.rs`
//!   deserializes (its `tools` field).
//! - [`McpSearchTools`](search_tools::McpSearchTools) (`mcp/search-tools`, AiSafe)
//!   — keyword-ranked catalog subset.
//! - [`McpToolHelp`](tool_help::McpToolHelp) (`mcp/tool-help`, AiSafe) — one
//!   tool's params/description; a not-found tool is a legitimate `found: false`
//!   result, not an error.
//! - [`McpRefresh`](refresh::McpRefresh) (`mcp/refresh`, Privileged) — cache
//!   refresh signal (rebuilds on next initialization). It reads no catalog state,
//!   so it's the **stateless** auto-registered form and is NOT assembled by
//!   [`command_objects`] below — `register_stateless_command!` wires it directly.

use std::sync::Arc;

use crate::modules::mcp::McpCatalog;
use crate::sdk_codegen::DynCommand;

pub mod list_tools;
pub mod refresh;
pub mod search_tools;
pub mod tool_help;

use list_tools::McpListTools;
use search_tools::McpSearchTools;
use tool_help::McpToolHelp;

/// The dep-holding `mcp/*` command objects over the module's shared
/// [`McpCatalog`]. Called from
/// [`MCPModule::commands`](crate::modules::mcp::MCPModule) so they reach
/// `command_registry()`, the ACL, codegen, and `uu`. (`mcp/refresh` is stateless
/// and auto-registers itself — it isn't in this list.)
pub fn command_objects(catalog: Arc<McpCatalog>) -> Vec<Arc<dyn DynCommand>> {
    vec![
        Arc::new(McpListTools {
            catalog: catalog.clone(),
        }),
        Arc::new(McpSearchTools {
            catalog: catalog.clone(),
        }),
        Arc::new(McpToolHelp { catalog }),
    ]
}
