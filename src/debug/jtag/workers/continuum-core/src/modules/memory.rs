//! MemoryModule — wraps PersonaMemoryManager for memory/recall operations.
//!
//! Handles: memory/load-corpus, memory/multi-layer-recall, memory/consciousness-context,
//!          memory/append-memory, memory/append-event
//!
//! All memory operations are pure compute on in-memory corpus data.
//! Data comes from TypeScript ORM via IPC. Zero SQL access.

use crate::runtime::{ServiceModule, ModuleConfig, ModulePriority, CommandResult, ModuleContext};
use crate::memory::{
    PersonaMemoryManager, CorpusMemory, CorpusTimelineEvent,
    MultiLayerRecallRequest, ConsciousnessContextRequest,
};
use crate::logging::TimingGuard;
use crate::{log_info, log_debug};
use async_trait::async_trait;
use serde_json::Value;
use std::any::Any;
use std::sync::Arc;

/// Shared state for memory module.
pub struct MemoryState {
    /// Per-persona memory manager — pure compute on in-memory MemoryCorpus.
    pub memory_manager: Arc<PersonaMemoryManager>,
}

impl MemoryState {
    pub fn new(memory_manager: Arc<PersonaMemoryManager>) -> Self {
        Self { memory_manager }
    }
}

pub struct MemoryModule {
    state: Arc<MemoryState>,
}

impl MemoryModule {
    pub fn new(state: Arc<MemoryState>) -> Self {
        Self { state }
    }
}

#[async_trait]
impl ServiceModule for MemoryModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "memory",
            priority: ModulePriority::Normal,
            command_prefixes: &["memory/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
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
        match command {
            "memory/load-corpus" => {
                let _timer = TimingGuard::new("module", "memory_load_corpus");

                let persona_id = params.get("persona_id")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing persona_id")?;

                let memories: Vec<CorpusMemory> = params.get("memories")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default();

                let events: Vec<CorpusTimelineEvent> = params.get("events")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default();

                let resp = self.state.memory_manager.load_corpus(persona_id, memories, events);

                log_info!(
                    "module", "memory_load_corpus",
                    "Loaded corpus for {}: {} memories ({} embedded), {} events ({} embedded), {:.1}ms",
                    persona_id, resp.memory_count, resp.embedded_memory_count,
                    resp.timeline_event_count, resp.embedded_event_count, resp.load_time_ms
                );

                Ok(CommandResult::Json(serde_json::to_value(&resp).unwrap_or_default()))
            }

            "memory/multi-layer-recall" => {
                let _timer = TimingGuard::new("module", "memory_multi_layer_recall");

                let persona_id = params.get("persona_id")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing persona_id")?;

                let query_text = params.get("query_text")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                let room_id = params.get("room_id")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing room_id")?
                    .to_string();

                let max_results = params.get("max_results")
                    .and_then(|v| v.as_u64())
                    .map(|n| n as usize)
                    .unwrap_or(10);

                let layers: Option<Vec<String>> = params.get("layers")
                    .and_then(|v| serde_json::from_value(v.clone()).ok());

                let req = MultiLayerRecallRequest {
                    query_text,
                    room_id,
                    max_results,
                    layers,
                };

                match self.state.memory_manager.multi_layer_recall(persona_id, &req) {
                    Ok(resp) => {
                        log_info!(
                            "module", "memory_multi_layer_recall",
                            "Multi-layer recall for {}: {} memories in {:.1}ms ({} candidates from {} layers)",
                            persona_id, resp.memories.len(), resp.recall_time_ms,
                            resp.total_candidates, resp.layer_timings.len()
                        );
                        Ok(CommandResult::Json(serde_json::to_value(&resp).unwrap_or_default()))
                    }
                    Err(e) => Err(format!("memory/multi-layer-recall failed: {e}")),
                }
            }

            "memory/consciousness-context" => {
                let _timer = TimingGuard::new("module", "memory_consciousness_context");

                let persona_id = params.get("persona_id")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing persona_id")?;

                let room_id = params.get("room_id")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing room_id")?
                    .to_string();

                let current_message = params.get("current_message")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                let skip_semantic_search = params.get("skip_semantic_search")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                let req = ConsciousnessContextRequest {
                    room_id,
                    current_message,
                    skip_semantic_search,
                };

                match self.state.memory_manager.consciousness_context(persona_id, &req) {
                    Ok(resp) => {
                        log_info!(
                            "module", "memory_consciousness_context",
                            "Consciousness context for {}: {:.1}ms, {} cross-context events, {} intentions",
                            persona_id, resp.build_time_ms, resp.cross_context_event_count, resp.active_intention_count
                        );
                        Ok(CommandResult::Json(serde_json::to_value(&resp).unwrap_or_default()))
                    }
                    Err(e) => Err(format!("memory/consciousness-context failed: {e}")),
                }
            }

            "memory/append-memory" => {
                let _timer = TimingGuard::new("module", "memory_append_memory");

                let persona_id = params.get("persona_id")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing persona_id")?;

                let memory: CorpusMemory = params.get("memory")
                    .ok_or("Missing memory")
                    .and_then(|v| serde_json::from_value(v.clone()).map_err(|_| "Invalid memory format"))?;

                match self.state.memory_manager.append_memory(persona_id, memory) {
                    Ok(()) => {
                        log_debug!("module", "memory_append_memory", "Appended memory to corpus for {}", persona_id);
                        Ok(CommandResult::Json(serde_json::json!({ "appended": true })))
                    }
                    Err(e) => Err(format!("memory/append-memory failed: {e}")),
                }
            }

            "memory/append-event" => {
                let _timer = TimingGuard::new("module", "memory_append_event");

                let persona_id = params.get("persona_id")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing persona_id")?;

                let event: CorpusTimelineEvent = params.get("event")
                    .ok_or("Missing event")
                    .and_then(|v| serde_json::from_value(v.clone()).map_err(|_| "Invalid event format"))?;

                match self.state.memory_manager.append_event(persona_id, event) {
                    Ok(()) => {
                        log_debug!("module", "memory_append_event", "Appended event to corpus for {}", persona_id);
                        Ok(CommandResult::Json(serde_json::json!({ "appended": true })))
                    }
                    Err(e) => Err(format!("memory/append-event failed: {e}")),
                }
            }

            _ => Err(format!("Unknown memory command: {command}")),
        }
    }

    fn as_any(&self) -> &dyn Any { self }
}
