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
    CommandResult, ModuleConfig, ModulePriority, ServiceModule, ModuleContext,
    CommandSchema, ParamSchema,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::any::Any;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use parking_lot::RwLock;

/// MCP tool definition (matches MCP protocol)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCPTool {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: MCPInputSchema,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCPInputSchema {
    #[serde(rename = "type")]
    pub schema_type: String,
    pub properties: HashMap<String, MCPProperty>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

pub struct MCPModule {
    /// Cached tools (refreshed on first request or when stale)
    tools_cache: RwLock<Option<Vec<MCPTool>>>,
    /// Path to TypeScript generated schemas
    schemas_path: PathBuf,
    /// Category priorities for sorting
    categories: HashMap<&'static str, ToolCategory>,
}

impl MCPModule {
    pub fn new() -> Self {
        // Compute schemas path relative to the binary location
        // In development, this is: workers/continuum-core -> ../../generated/command-schemas.json
        let schemas_path = std::env::current_dir()
            .map(|p| p.join("generated/command-schemas.json"))
            .unwrap_or_else(|_| PathBuf::from("generated/command-schemas.json"));

        let mut categories = HashMap::new();

        // Essential tools (always shown first)
        categories.insert("ping", ToolCategory { priority: 0, description: "Health check" });
        categories.insert("help", ToolCategory { priority: 0, description: "Documentation" });
        categories.insert("list", ToolCategory { priority: 0, description: "List commands" });

        // Common interface tools
        categories.insert("interface/screenshot", ToolCategory { priority: 1, description: "Screenshot" });
        categories.insert("interface/navigate", ToolCategory { priority: 1, description: "Navigation" });
        categories.insert("interface/click", ToolCategory { priority: 1, description: "Click" });

        // Chat tools
        categories.insert("collaboration/chat/send", ToolCategory { priority: 1, description: "Send chat" });
        categories.insert("collaboration/chat/export", ToolCategory { priority: 1, description: "Export chat" });

        // Category prefixes
        categories.insert("interface/", ToolCategory { priority: 10, description: "Interface" });
        categories.insert("collaboration/", ToolCategory { priority: 20, description: "Collaboration" });
        categories.insert("ai/", ToolCategory { priority: 30, description: "AI" });
        categories.insert("data/", ToolCategory { priority: 40, description: "Data" });
        categories.insert("workspace/", ToolCategory { priority: 50, description: "Workspace" });
        categories.insert("development/", ToolCategory { priority: 60, description: "Development" });
        categories.insert("media/", ToolCategory { priority: 70, description: "Media" });
        categories.insert("system/", ToolCategory { priority: 80, description: "System" });
        categories.insert("mcp/", ToolCategory { priority: -1, description: "MCP meta-tools" });

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
            Ok(content) => {
                match serde_json::from_str::<HashMap<String, Value>>(&content) {
                    Ok(schemas) => schemas,
                    Err(e) => {
                        tracing::warn!("Failed to parse command schemas JSON: {}", e);
                        HashMap::new()
                    }
                }
            }
            Err(e) => {
                tracing::debug!("Could not read schemas file at {:?}: {}", self.schemas_path, e);
                HashMap::new()
            }
        }
    }

    /// Convert TypeScript schema to MCP tool
    fn ts_schema_to_tool(&self, name: &str, schema: &Value) -> Option<MCPTool> {
        let description = schema.get("description")
            .and_then(|v| v.as_str())
            .unwrap_or(name);

        let params = schema.get("params").and_then(|v| v.as_object());

        let mut properties = HashMap::new();
        let mut required = Vec::new();

        if let Some(params_obj) = params {
            for (param_name, param_def) in params_obj {
                let param_type = param_def.get("type")
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

                let is_required = param_def.get("required")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                let param_description = param_def.get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or(param_name);

                properties.insert(param_name.clone(), MCPProperty {
                    prop_type: json_type.to_string(),
                    description: param_description.to_string(),
                });

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
                required: if required.is_empty() { None } else { Some(required) },
            },
        })
    }

    /// Convert Rust CommandSchema to MCP tool
    fn rust_schema_to_tool(&self, schema: &CommandSchema) -> MCPTool {
        let mut properties = HashMap::new();
        let mut required = Vec::new();

        for param in &schema.params {
            properties.insert(param.name.to_string(), MCPProperty {
                prop_type: param.param_type.to_string(),
                description: param.description.to_string(),
            });

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
                required: if required.is_empty() { None } else { Some(required) },
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
            description: "[JTAG] Search for tools by keyword. Returns matching tool names and descriptions.".to_string(),
            input_schema: MCPInputSchema {
                schema_type: "object".to_string(),
                properties: {
                    let mut props = HashMap::new();
                    props.insert("query".to_string(), MCPProperty {
                        prop_type: "string".to_string(),
                        description: "Search query - matches against tool names and descriptions".to_string(),
                    });
                    props.insert("limit".to_string(), MCPProperty {
                        prop_type: "number".to_string(),
                        description: "Max results to return (default: 10)".to_string(),
                    });
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
                    props.insert("tool".to_string(), MCPProperty {
                        prop_type: "string".to_string(),
                        description: "Tool name to get help for".to_string(),
                    });
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

    /// Search tools by keyword
    fn search_tools(&self, tools: &[MCPTool], query: &str, limit: usize) -> Vec<Value> {
        let query_lower = query.to_lowercase();
        let mut results: Vec<(i32, &MCPTool)> = Vec::new();

        for tool in tools {
            let name_lower = tool.name.to_lowercase();
            let desc_lower = tool.description.to_lowercase();

            let mut score = 0i32;
            if name_lower.contains(&query_lower) { score += 10; }
            if name_lower.starts_with(&query_lower) { score += 5; }
            if desc_lower.contains(&query_lower) { score += 3; }

            // Exact segment match
            let segments: Vec<&str> = name_lower.split(|c| c == '/' || c == '-' || c == '_').collect();
            if segments.contains(&query_lower.as_str()) { score += 8; }

            if score > 0 {
                results.push((score, tool));
            }
        }

        results.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.name.cmp(&b.1.name)));

        results.into_iter()
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
    fn get_tool_help(&self, tools: &[MCPTool], tool_name: &str) -> Option<Value> {
        // Normalize tool name
        let normalized = tool_name.replace('/', "_").replace("mcp__jtag__", "");

        tools.iter()
            .find(|t| t.name == normalized || t.name == tool_name)
            .map(|tool| {
                let params: Vec<Value> = tool.input_schema.properties.iter()
                    .map(|(name, prop)| {
                        let required = tool.input_schema.required.as_ref()
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
        }
    }

    async fn initialize(&self, ctx: &ModuleContext) -> Result<(), String> {
        // Pre-build tools cache
        let tools = self.build_tools(ctx);
        *self.tools_cache.write() = Some(tools);
        tracing::info!("MCPModule initialized with {} tools",
            self.tools_cache.read().as_ref().map(|t| t.len()).unwrap_or(0));
        Ok(())
    }

    async fn handle_command(
        &self,
        command: &str,
        params: Value,
    ) -> Result<CommandResult, String> {
        match command {
            "mcp/list-tools" => {
                let tools = self.tools_cache.read();
                let tools = tools.as_ref().ok_or("Tools cache not initialized")?;

                Ok(CommandResult::Json(json!({
                    "success": true,
                    "tools": tools,
                    "count": tools.len()
                })))
            }

            "mcp/search-tools" => {
                let query = params.get("query")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing required parameter: query")?;

                let limit = params.get("limit")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(10) as usize;

                let tools = self.tools_cache.read();
                let tools = tools.as_ref().ok_or("Tools cache not initialized")?;

                let results = self.search_tools(tools, query, limit);

                Ok(CommandResult::Json(json!({
                    "success": true,
                    "query": query,
                    "count": results.len(),
                    "tools": results
                })))
            }

            "mcp/tool-help" => {
                let tool_name = params.get("tool")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing required parameter: tool")?;

                let tools = self.tools_cache.read();
                let tools = tools.as_ref().ok_or("Tools cache not initialized")?;

                match self.get_tool_help(tools, tool_name) {
                    Some(help) => Ok(CommandResult::Json(json!({
                        "success": true,
                        "help": help
                    }))),
                    None => Ok(CommandResult::Json(json!({
                        "success": false,
                        "error": format!("Tool not found: {}", tool_name),
                        "hint": "Use mcp/search-tools to find available tools"
                    })))
                }
            }

            "mcp/refresh" => {
                // Force refresh the tools cache
                // Note: We can't access ctx here, so this is a no-op for now
                // The tools will be refreshed on next server restart
                Ok(CommandResult::Json(json!({
                    "success": true,
                    "message": "Tools will be refreshed on next initialization"
                })))
            }

            _ => Err(format!("Unknown MCP command: {}", command)),
        }
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
                params: vec![
                    ParamSchema {
                        name: "tool",
                        param_type: "string",
                        required: true,
                        description: "Tool name to get help for",
                    },
                ],
            },
        ]
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
