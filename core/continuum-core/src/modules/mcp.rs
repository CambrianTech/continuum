//! MCPModule — Dynamic tool discovery for MCP servers.
//!
//! Provides commands for MCP (Model Context Protocol) servers to discover
//! and call JTAG commands dynamically. No static schemas, no stale data.
//!
//! Single source of truth: Commands ARE tools. The MCPModule queries the
//! ModuleRegistry and TypeScript-generated schemas at runtime.
//!
//! Commands:
//! - mcp/list-tools: Return all commands as MCP tool definitions
//! - mcp/search-tools: Search tools by keyword
//! - mcp/tool-help: Get detailed help for a specific tool

use crate::runtime::{
    CommandResult, CommandSchema, ModuleConfig, ModuleContext, ModulePriority, ParamSchema,
    ServiceModule,
};
use async_trait::async_trait;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::any::Any;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use ts_rs::TS;

/// MCP tool definition (matches MCP protocol)
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/mcp/MCPTool.ts")]
pub struct MCPTool {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: MCPInputSchema,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/mcp/MCPInputSchema.ts"
)]
pub struct MCPInputSchema {
    #[serde(rename = "type")]
    pub schema_type: String,
    pub properties: HashMap<String, MCPProperty>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub required: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/mcp/MCPProperty.ts")]
pub struct MCPProperty {
    #[serde(rename = "type")]
    pub prop_type: String,
    pub description: String,
}

/// Category configuration for tool organization
#[derive(Debug, Clone)]
struct ToolCategory {
    priority: i32,
    #[allow(dead_code)]
    description: &'static str,
}

/// The MCP tool catalog: the shared state both the module's `initialize`
/// (which builds the cache from the live registry) and the typed
/// `commands/mcp/*` verbs (which read it) hold via `Arc`. Extracted from the
/// old monolithic `MCPModule` so the catalog-reading commands can migrate onto
/// the typed [`ActionCommand`](crate::sdk_codegen::ActionCommand) path — the
/// [`CodeState`](crate::modules::code::CodeState) dep-holding pattern.
pub(crate) struct McpCatalog {
    /// Cached tools (refreshed on first request or when stale)
    tools_cache: RwLock<Option<Vec<MCPTool>>>,
    /// Path to TypeScript generated schemas
    schemas_path: PathBuf,
    /// Category priorities for sorting
    categories: HashMap<&'static str, ToolCategory>,
}

impl Default for McpCatalog {
    fn default() -> Self {
        Self::new()
    }
}

impl McpCatalog {
    pub(crate) fn new() -> Self {
        // Compute schemas path relative to the binary location
        // In development, this is: workers/continuum-core -> ../../generated/command-schemas.json
        let schemas_path = std::env::current_dir()
            .map(|p| p.join("generated/command-schemas.json"))
            .unwrap_or_else(|_| PathBuf::from("generated/command-schemas.json"));

        let mut categories = HashMap::new();

        // Essential tools (always shown first)
        categories.insert(
            "ping",
            ToolCategory {
                priority: 0,
                description: "Health check",
            },
        );
        categories.insert(
            "help",
            ToolCategory {
                priority: 0,
                description: "Documentation",
            },
        );
        categories.insert(
            "list",
            ToolCategory {
                priority: 0,
                description: "List commands",
            },
        );

        // Common interface tools
        categories.insert(
            "interface/screenshot",
            ToolCategory {
                priority: 1,
                description: "Screenshot",
            },
        );
        categories.insert(
            "interface/navigate",
            ToolCategory {
                priority: 1,
                description: "Navigation",
            },
        );
        categories.insert(
            "interface/click",
            ToolCategory {
                priority: 1,
                description: "Click",
            },
        );

        // Chat tools
        categories.insert(
            "collaboration/chat/send",
            ToolCategory {
                priority: 1,
                description: "Send chat",
            },
        );
        categories.insert(
            "collaboration/chat/export",
            ToolCategory {
                priority: 1,
                description: "Export chat",
            },
        );

        // Category prefixes
        categories.insert(
            "interface/",
            ToolCategory {
                priority: 10,
                description: "Interface",
            },
        );
        categories.insert(
            "collaboration/",
            ToolCategory {
                priority: 20,
                description: "Collaboration",
            },
        );
        categories.insert(
            "ai/",
            ToolCategory {
                priority: 30,
                description: "AI",
            },
        );
        categories.insert(
            "data/",
            ToolCategory {
                priority: 40,
                description: "Data",
            },
        );
        categories.insert(
            "workspace/",
            ToolCategory {
                priority: 50,
                description: "Workspace",
            },
        );
        categories.insert(
            "development/",
            ToolCategory {
                priority: 60,
                description: "Development",
            },
        );
        categories.insert(
            "media/",
            ToolCategory {
                priority: 70,
                description: "Media",
            },
        );
        categories.insert(
            "system/",
            ToolCategory {
                priority: 80,
                description: "System",
            },
        );
        categories.insert(
            "mcp/",
            ToolCategory {
                priority: -1,
                description: "MCP meta-tools",
            },
        );

        Self {
            tools_cache: RwLock::new(None),
            schemas_path,
            categories,
        }
    }

    /// Get priority for a command (lower = shown first)
    fn get_priority(&self, command_name: &str) -> i32 {
        // Check exact match first
        if let Some(cat) = self.categories.get(command_name) {
            return cat.priority;
        }

        // Check prefix matches
        for (prefix, cat) in &self.categories {
            if prefix.ends_with('/') && command_name.starts_with(*prefix) {
                return cat.priority;
            }
        }

        100 // Default priority
    }

    /// Load TypeScript generated schemas from JSON file
    fn load_ts_schemas(&self) -> HashMap<String, Value> {
        match fs::read_to_string(&self.schemas_path) {
            Ok(content) => match serde_json::from_str::<HashMap<String, Value>>(&content) {
                Ok(schemas) => schemas,
                Err(e) => {
                    tracing::warn!("Failed to parse command schemas JSON: {}", e);
                    HashMap::new()
                }
            },
            Err(e) => {
                tracing::debug!(
                    "Could not read schemas file at {:?}: {}",
                    self.schemas_path,
                    e
                );
                HashMap::new()
            }
        }
    }

    /// Convert TypeScript schema to MCP tool
    fn ts_schema_to_tool(&self, name: &str, schema: &Value) -> Option<MCPTool> {
        let description = schema
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or(name);

        let params = schema.get("params").and_then(|v| v.as_object());

        let mut properties = HashMap::new();
        let mut required = Vec::new();

        if let Some(params_obj) = params {
            for (param_name, param_def) in params_obj {
                let param_type = param_def
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("string");

                // Map TypeScript types to JSON Schema types
                let json_type = match param_type {
                    t if t.contains("number") => "number",
                    t if t.contains("boolean") => "boolean",
                    t if t.contains("array") || t.starts_with("Array") => "array",
                    t if t.contains("object") || t.starts_with("{") => "object",
                    _ => "string",
                };

                let is_required = param_def
                    .get("required")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                let param_description = param_def
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or(param_name);

                properties.insert(
                    param_name.clone(),
                    MCPProperty {
                        prop_type: json_type.to_string(),
                        description: param_description.to_string(),
                    },
                );

                if is_required {
                    required.push(param_name.clone());
                }
            }
        }

        // Sanitize command name for MCP (replace / with _)
        let tool_name = name.replace('/', "_");

        Some(MCPTool {
            name: tool_name,
            description: format!("[JTAG] {}", description),
            input_schema: MCPInputSchema {
                schema_type: "object".to_string(),
                properties,
                required: if required.is_empty() {
                    None
                } else {
                    Some(required)
                },
            },
        })
    }

    /// Convert Rust CommandSchema to MCP tool
    fn rust_schema_to_tool(&self, schema: &CommandSchema) -> MCPTool {
        let mut properties = HashMap::new();
        let mut required = Vec::new();

        for param in &schema.params {
            properties.insert(
                param.name.to_string(),
                MCPProperty {
                    prop_type: param.param_type.to_string(),
                    description: param.description.to_string(),
                },
            );

            if param.required {
                required.push(param.name.to_string());
            }
        }

        let tool_name = schema.name.replace('/', "_");

        MCPTool {
            name: tool_name,
            description: format!("[JTAG] {}", schema.description),
            input_schema: MCPInputSchema {
                schema_type: "object".to_string(),
                properties,
                required: if required.is_empty() {
                    None
                } else {
                    Some(required)
                },
            },
        }
    }

    /// Build the complete tool list (TypeScript + Rust modules)
    fn build_tools(&self, ctx: &ModuleContext) -> Vec<MCPTool> {
        let mut tools = Vec::new();
        let mut seen_names = std::collections::HashSet::new();

        // 1. Add MCP meta-tools first
        tools.push(MCPTool {
            name: "mcp_search_tools".to_string(),
            description:
                "[JTAG] Search for tools by keyword. Returns matching tool names and descriptions."
                    .to_string(),
            input_schema: MCPInputSchema {
                schema_type: "object".to_string(),
                properties: {
                    let mut props = HashMap::new();
                    props.insert(
                        "query".to_string(),
                        MCPProperty {
                            prop_type: "string".to_string(),
                            description:
                                "Search query - matches against tool names and descriptions"
                                    .to_string(),
                        },
                    );
                    props.insert(
                        "limit".to_string(),
                        MCPProperty {
                            prop_type: "number".to_string(),
                            description: "Max results to return (default: 10)".to_string(),
                        },
                    );
                    props
                },
                required: Some(vec!["query".to_string()]),
            },
        });
        seen_names.insert("mcp_search_tools".to_string());

        tools.push(MCPTool {
            name: "mcp_tool_help".to_string(),
            description: "[JTAG] Get detailed help for a specific tool.".to_string(),
            input_schema: MCPInputSchema {
                schema_type: "object".to_string(),
                properties: {
                    let mut props = HashMap::new();
                    props.insert(
                        "tool".to_string(),
                        MCPProperty {
                            prop_type: "string".to_string(),
                            description: "Tool name to get help for".to_string(),
                        },
                    );
                    props
                },
                required: Some(vec!["tool".to_string()]),
            },
        });
        seen_names.insert("mcp_tool_help".to_string());

        // 2. Load TypeScript generated schemas
        let ts_schemas = self.load_ts_schemas();
        for (name, schema) in &ts_schemas {
            if let Some(tool) = self.ts_schema_to_tool(name, schema) {
                if !seen_names.contains(&tool.name) {
                    seen_names.insert(tool.name.clone());
                    tools.push(tool);
                }
            }
        }

        // 3. Add Rust module schemas (override TypeScript if present)
        for module_name in ctx.registry.list_modules() {
            if let Some(module) = ctx.registry.get_by_name(module_name) {
                for schema in module.command_schemas() {
                    let tool = self.rust_schema_to_tool(&schema);
                    // Rust schemas take precedence
                    if let Some(pos) = tools.iter().position(|t| t.name == tool.name) {
                        tools[pos] = tool;
                    } else {
                        seen_names.insert(tool.name.clone());
                        tools.push(tool);
                    }
                }
            }
        }

        // 4. Sort by priority
        tools.sort_by(|a, b| {
            let priority_a = self.get_priority(&a.name.replace('_', "/"));
            let priority_b = self.get_priority(&b.name.replace('_', "/"));
            if priority_a != priority_b {
                priority_a.cmp(&priority_b)
            } else {
                a.name.cmp(&b.name)
            }
        });

        tools
    }

    /// Clone the built tool catalog, or fail loud if `initialize` never ran (the
    /// commands read a pre-built cache; an empty cache is a boot-ordering bug, not
    /// a silent empty result).
    pub(crate) fn list(&self) -> Result<Vec<MCPTool>, crate::sdk_codegen::CommandError> {
        self.tools_cache.read().as_ref().cloned().ok_or_else(|| {
            crate::sdk_codegen::CommandError::Internal(
                "MCP tools cache not initialized".to_string(),
            )
        })
    }

    /// Search tools by keyword
    pub(crate) fn search_tools(&self, tools: &[MCPTool], query: &str, limit: usize) -> Vec<Value> {
        let query_lower = query.to_lowercase();
        let mut results: Vec<(i32, &MCPTool)> = Vec::new();

        for tool in tools {
            let name_lower = tool.name.to_lowercase();
            let desc_lower = tool.description.to_lowercase();

            let mut score = 0i32;
            if name_lower.contains(&query_lower) {
                score += 10;
            }
            if name_lower.starts_with(&query_lower) {
                score += 5;
            }
            if desc_lower.contains(&query_lower) {
                score += 3;
            }

            // Exact segment match
            let segments: Vec<&str> = name_lower.split(['/', '-', '_']).collect();
            if segments.contains(&query_lower.as_str()) {
                score += 8;
            }

            if score > 0 {
                results.push((score, tool));
            }
        }

        results.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.name.cmp(&b.1.name)));

        results
            .into_iter()
            .take(limit)
            .map(|(_, tool)| {
                json!({
                    "name": tool.name,
                    "description": tool.description,
                    "jtag_command": tool.name.replace('_', "/")
                })
            })
            .collect()
    }

    /// Get help for a specific tool
    pub(crate) fn get_tool_help(&self, tools: &[MCPTool], tool_name: &str) -> Option<Value> {
        // Normalize tool name
        let normalized = tool_name.replace('/', "_").replace("mcp__jtag__", "");

        tools
            .iter()
            .find(|t| t.name == normalized || t.name == tool_name)
            .map(|tool| {
                let params: Vec<Value> = tool
                    .input_schema
                    .properties
                    .iter()
                    .map(|(name, prop)| {
                        let required = tool
                            .input_schema
                            .required
                            .as_ref()
                            .map(|r| r.contains(name))
                            .unwrap_or(false);
                        json!({
                            "name": name,
                            "type": prop.prop_type,
                            "required": required,
                            "description": prop.description
                        })
                    })
                    .collect();

                json!({
                    "name": tool.name.replace('_', "/"),
                    "mcp_tool_name": format!("mcp__jtag__{}", tool.name),
                    "description": tool.description,
                    "params": params
                })
            })
    }
}

/// Thin [`ServiceModule`] wrapper over the shared [`McpCatalog`]. Owns the
/// module lifecycle (build the catalog cache at `initialize` from the live
/// registry) and contributes the typed `mcp/*` commands, which share the same
/// `Arc<McpCatalog>` — the [`CodeModule`](crate::modules::code::CodeModule)
/// shape. `command_schemas` stays so the module still self-advertises its verbs
/// into the catalog it builds during the Registry-A→B transition.
pub struct MCPModule {
    catalog: Arc<McpCatalog>,
}

impl Default for MCPModule {
    fn default() -> Self {
        Self::new()
    }
}

impl MCPModule {
    pub fn new() -> Self {
        Self {
            catalog: Arc::new(McpCatalog::new()),
        }
    }
}

#[async_trait]
impl ServiceModule for MCPModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "mcp",
            priority: ModulePriority::Normal,
            command_prefixes: &["mcp/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(&self, ctx: &ModuleContext) -> Result<(), String> {
        // Pre-build tools cache
        let tools = self.catalog.build_tools(ctx);
        *self.catalog.tools_cache.write() = Some(tools);
        tracing::info!(
            "MCPModule initialized with {} tools",
            self.catalog
                .tools_cache
                .read()
                .as_ref()
                .map(|t| t.len())
                .unwrap_or(0)
        );
        Ok(())
    }

    fn commands(&self) -> Vec<Arc<dyn crate::sdk_codegen::DynCommand>> {
        crate::commands::mcp::command_objects(self.catalog.clone())
    }

    async fn handle_command(&self, command: &str, _params: Value) -> Result<CommandResult, String> {
        // Every `mcp/*` verb is now a typed `ActionCommand` that routes via
        // `route_object` (the catalog family in `crate::commands::mcp` + the
        // stateless `mcp/refresh`). Reaching this legacy path at all means a
        // descriptor failed to register — fail loud naming the command rather than
        // silently re-handling it. (This whole impl is retired wholesale when
        // Registry A's trait default becomes fail-loud — #63.)
        Err(format!(
            "'{command}' is a migrated, typed mcp command — it must route via the \
             object registry (route_object), not the legacy handle_command path. \
             Reaching here means its descriptor failed to register."
        ))
    }

    fn command_schemas(&self) -> Vec<CommandSchema> {
        vec![
            CommandSchema {
                name: "mcp/list-tools",
                description: "List all available commands as MCP tool definitions",
                params: vec![],
            },
            CommandSchema {
                name: "mcp/search-tools",
                description: "Search for tools by keyword",
                params: vec![
                    ParamSchema {
                        name: "query",
                        param_type: "string",
                        required: true,
                        description: "Search query - matches tool names and descriptions",
                    },
                    ParamSchema {
                        name: "limit",
                        param_type: "number",
                        required: false,
                        description: "Max results to return (default: 10)",
                    },
                ],
            },
            CommandSchema {
                name: "mcp/tool-help",
                description: "Get detailed help for a specific tool",
                params: vec![ParamSchema {
                    name: "tool",
                    param_type: "string",
                    required: true,
                    description: "Tool name to get help for",
                }],
            },
        ]
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    //! Per-module integration TDD for the MCP catalog, via `ModuleHarness` — the
    //! module is booted in ISOLATION (registry + context + executor, no monolith)
    //! and initialized (its tool cache builds from its own command_schemas), then
    //! driven through the REAL dispatch chain. Post-migration this exercises the
    //! typed `commands/mcp/*` verbs end to end (the executor's `route_object` wins),
    //! which is why it lives here and not in the command files: the success paths
    //! need a live `ModuleContext` to build the cache, which only the harness
    //! provides. Assertions pin only the STABLE surface (the hardcoded meta-tools),
    //! so they're deterministic regardless of whatever `generated/command-schemas.json`
    //! happens to be present. The command files hold the uninitialized-cache
    //! fail-loud unit tests.

    use super::*;
    use crate::runtime::module_harness::ModuleHarness;
    use serde_json::json;
    use std::sync::Arc;

    async fn harness() -> ModuleHarness {
        ModuleHarness::with(Arc::new(MCPModule::new())).await
    }

    // what this catches: mcp/list-tools returns the catalog, including the
    // always-present meta-tools — proving the harness initialized the module (its
    // cache built) and dispatched the typed command in isolation. `count` must
    // equal the array length (the parity field the migration preserved).
    #[tokio::test]
    async fn list_tools_includes_the_meta_tools() {
        let h = harness().await;
        let out = h.execute_json("mcp/list-tools", json!({})).await.unwrap();
        let tools = out["tools"].as_array().expect("tools array");
        let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
        assert!(
            names.contains(&"mcp_search_tools"),
            "meta-tool present: {names:?}"
        );
        assert!(
            names.contains(&"mcp_tool_help"),
            "meta-tool present: {names:?}"
        );
        assert_eq!(out["count"], tools.len(), "count matches the array length");
    }

    // what this catches: mcp/search-tools scores + filters by keyword — searching
    // "search" surfaces mcp_search_tools, and each hit carries the typed shape.
    #[tokio::test]
    async fn search_tools_finds_by_keyword() {
        let h = harness().await;
        let out = h
            .execute_json("mcp/search-tools", json!({ "query": "search" }))
            .await
            .unwrap();
        let hits = out["tools"].as_array().unwrap();
        let names: Vec<&str> = hits.iter().filter_map(|t| t["name"].as_str()).collect();
        assert!(
            names.contains(&"mcp_search_tools"),
            "keyword match: {names:?}"
        );
        assert_eq!(out["count"], hits.len(), "count matches the hit array");
        assert!(
            hits.iter().all(|h| h["jtag_command"].is_string()),
            "each hit carries the jtag form: {hits:?}"
        );
    }

    // what this catches: mcp/search-tools requires `query` — a missing required
    // param is a loud refusal (the typed-param deserialization error), not a
    // panic/empty.
    #[tokio::test]
    async fn search_tools_requires_query() {
        let h = harness().await;
        let err = h
            .execute_json("mcp/search-tools", json!({}))
            .await
            .unwrap_err();
        assert!(!err.is_empty(), "missing query must refuse, got: {err:?}");
    }

    // what this catches: mcp/tool-help returns typed params for a known tool
    // (found:true + help), and a clean not-found (found:false + hint) for an
    // unknown one — both Ok-shaped, never a panic (the not-found-as-Ok contract).
    #[tokio::test]
    async fn tool_help_known_and_unknown() {
        let h = harness().await;

        let known = h
            .execute_json("mcp/tool-help", json!({ "tool": "mcp_search_tools" }))
            .await
            .unwrap();
        assert_eq!(known["found"], true, "help for a known tool: {known}");
        let params: Vec<&str> = known["help"]["params"]
            .as_array()
            .map(|a| a.iter().filter_map(|p| p["name"].as_str()).collect())
            .unwrap_or_default();
        assert!(
            params.contains(&"query"),
            "tool-help lists the tool's params: {params:?}"
        );

        let unknown = h
            .execute_json("mcp/tool-help", json!({ "tool": "definitely-not-a-tool" }))
            .await
            .unwrap();
        assert_eq!(unknown["found"], false);
        assert!(unknown["hint"].is_string(), "not-found carries a hint");
    }
}
