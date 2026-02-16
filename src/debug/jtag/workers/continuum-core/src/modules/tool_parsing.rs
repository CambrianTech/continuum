//! ToolParsingModule — stateless tool call parsing + correction IPC.
//!
//! Commands:
//! - `tool-parsing/parse`: Parse response text -> tool calls + cleaned text
//! - `tool-parsing/correct`: Correct a single tool call (name + params)
//! - `tool-parsing/register-tools`: Register tool names for codec
//! - `tool-parsing/decode-name`: Decode a model-produced tool name
//! - `tool-parsing/encode-name`: Encode a tool name for API transmission

use crate::runtime::{ServiceModule, ModuleConfig, ModulePriority, CommandResult, ModuleContext};
use crate::tool_parsing::{self, ToolNameCodec};
use crate::utils::params::Params;
use async_trait::async_trait;
use serde_json::Value;
use std::any::Any;
use std::sync::Arc;
use std::collections::HashMap;

pub struct ToolParsingModule {
    codec: Arc<ToolNameCodec>,
}

impl ToolParsingModule {
    pub fn new() -> Self {
        Self {
            codec: Arc::new(ToolNameCodec::new()),
        }
    }
}

#[async_trait]
impl ServiceModule for ToolParsingModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "tool-parsing",
            priority: ModulePriority::Normal,
            command_prefixes: &["tool-parsing/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
        Ok(())
    }

    async fn handle_command(
        &self,
        command: &str,
        params: Value,
    ) -> Result<CommandResult, String> {
        let p = Params::new(&params);

        match command {
            "tool-parsing/parse" => {
                let response_text = p.str("response_text")?;
                let result = tool_parsing::parse_and_correct(response_text);
                CommandResult::json(&result)
            }

            "tool-parsing/correct" => {
                let tool_name = p.str("tool_name")?;
                let parameters: HashMap<String, String> = match params.get("parameters") {
                    Some(Value::Object(map)) => {
                        map.iter().map(|(k, v)| {
                            (k.clone(), match v {
                                Value::String(s) => s.clone(),
                                _ => v.to_string(),
                            })
                        }).collect()
                    }
                    _ => HashMap::new(),
                };
                let corrected = tool_parsing::correction::correct_tool_call(tool_name, &parameters);
                CommandResult::json(&corrected)
            }

            "tool-parsing/register-tools" => {
                let tools: Vec<String> = match params.get("tools") {
                    Some(Value::Array(arr)) => {
                        arr.iter().filter_map(|v| v.as_str().map(String::from)).collect()
                    }
                    _ => return Err("Missing 'tools' array".to_string()),
                };
                let count = tools.len();
                self.codec.register_all(&tools);
                Ok(CommandResult::Json(serde_json::json!({
                    "registered": count,
                    "total": self.codec.count(),
                })))
            }

            "tool-parsing/decode-name" => {
                let raw = p.str("name")?;
                let decoded = self.codec.decode(raw);
                Ok(CommandResult::Json(serde_json::json!({
                    "decoded": decoded,
                    "changed": decoded != raw,
                })))
            }

            "tool-parsing/encode-name" => {
                let name = p.str("name")?;
                let encoded = self.codec.encode(name);
                Ok(CommandResult::Json(serde_json::json!({
                    "encoded": encoded,
                })))
            }

            _ => Err(format!("Unknown command: {command}")),
        }
    }

    fn as_any(&self) -> &dyn Any { self }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_parse_command() {
        let module = ToolParsingModule::new();
        let params = serde_json::json!({
            "response_text": "<tool_use><tool_name>code/search</tool_name><parameters><query>test</query></parameters></tool_use>"
        });
        let result = module.handle_command("tool-parsing/parse", params).await;
        assert!(result.is_ok());
        if let Ok(CommandResult::Json(json)) = result {
            let calls = json["tool_calls"].as_array().unwrap();
            assert_eq!(calls.len(), 1);
            assert_eq!(calls[0]["tool_name"], "code/search");
            // query -> pattern (correction)
            assert!(calls[0]["parameters"]["pattern"].is_string());
        }
    }

    #[tokio::test]
    async fn test_correct_command() {
        let module = ToolParsingModule::new();
        let params = serde_json::json!({
            "tool_name": "workspace/tree",
            "parameters": { "directory": "./src" }
        });
        let result = module.handle_command("tool-parsing/correct", params).await;
        assert!(result.is_ok());
        if let Ok(CommandResult::Json(json)) = result {
            assert_eq!(json["tool_name"], "code/tree");
            assert_eq!(json["name_changed"], true);
            assert_eq!(json["parameters"]["path"], "./src");
        }
    }

    #[tokio::test]
    async fn test_register_and_decode() {
        let module = ToolParsingModule::new();

        // Register tools
        let reg_params = serde_json::json!({
            "tools": ["code/write", "code/read", "collaboration/chat/send"]
        });
        let reg_result = module.handle_command("tool-parsing/register-tools", reg_params).await;
        assert!(reg_result.is_ok());
        if let Ok(CommandResult::Json(json)) = reg_result {
            assert_eq!(json["registered"], 3);
            assert_eq!(json["total"], 3);
        }

        // Decode encoded name
        let dec_params = serde_json::json!({ "name": "code_write" });
        let dec_result = module.handle_command("tool-parsing/decode-name", dec_params).await;
        assert!(dec_result.is_ok());
        if let Ok(CommandResult::Json(json)) = dec_result {
            assert_eq!(json["decoded"], "code/write");
            assert_eq!(json["changed"], true);
        }

        // Decode with prefix
        let prefix_params = serde_json::json!({ "name": "$FUNCTIONS.code_write" });
        let prefix_result = module.handle_command("tool-parsing/decode-name", prefix_params).await;
        assert!(prefix_result.is_ok());
        if let Ok(CommandResult::Json(json)) = prefix_result {
            assert_eq!(json["decoded"], "code/write");
        }
    }

    #[tokio::test]
    async fn test_encode_command() {
        let module = ToolParsingModule::new();
        let params = serde_json::json!({ "name": "collaboration/chat/send" });
        let result = module.handle_command("tool-parsing/encode-name", params).await;
        assert!(result.is_ok());
        if let Ok(CommandResult::Json(json)) = result {
            assert_eq!(json["encoded"], "collaboration_chat_send");
        }
    }

    #[tokio::test]
    async fn test_unknown_command() {
        let module = ToolParsingModule::new();
        let result = module.handle_command("tool-parsing/nope", Value::Null).await;
        assert!(result.is_err());
    }
}
