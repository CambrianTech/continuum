//! ChannelModule — wraps per-persona ChannelRegistry + PersonaState DashMap state.
//!
//! Validates the ServiceModule trait handles stateful per-persona DashMap isolation —
//! together with CognitionModule, these two prove the most different pattern from
//! stateless HealthModule.
//!
//! Handles: channel/enqueue, channel/dequeue, channel/status,
//!          channel/service-cycle, channel/service-cycle-full, channel/clear

use crate::runtime::{ServiceModule, ModuleConfig, ModulePriority, CommandResult, ModuleContext};
use crate::persona::{
    ChannelRegistry, PersonaState, ChannelEnqueueRequest, ActivityDomain,
    PersonaCognitionEngine, InboxMessage, SenderType, Modality,
};
use crate::persona::channel_types::DOMAIN_PRIORITY_ORDER;
use crate::logging::TimingGuard;
use crate::utils::params::Params;
use crate::log_info;
use async_trait::async_trait;
use dashmap::DashMap;
use serde_json::Value;
use std::any::Any;
use std::sync::Arc;
use uuid::Uuid;

/// Shared state for channel module — per-persona registries and states.
pub struct ChannelState {
    /// Per-persona channel registries + states.
    pub registries: Arc<DashMap<Uuid, (ChannelRegistry, PersonaState)>>,
    /// Reference to cognition engines for service-cycle-full fast-path decision.
    pub cognition_engines: Arc<DashMap<Uuid, PersonaCognitionEngine>>,
}

impl ChannelState {
    pub fn new(cognition_engines: Arc<DashMap<Uuid, PersonaCognitionEngine>>) -> Self {
        Self {
            registries: Arc::new(DashMap::new()),
            cognition_engines,
        }
    }

    /// Create from existing DashMaps (for gradual migration from ServerState).
    pub fn from_existing(
        registries: Arc<DashMap<Uuid, (ChannelRegistry, PersonaState)>>,
        cognition_engines: Arc<DashMap<Uuid, PersonaCognitionEngine>>,
    ) -> Self {
        Self { registries, cognition_engines }
    }
}

pub struct ChannelModule {
    state: Arc<ChannelState>,
}

impl ChannelModule {
    pub fn new(state: Arc<ChannelState>) -> Self {
        Self { state }
    }
}

#[async_trait]
impl ServiceModule for ChannelModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "channel",
            priority: ModulePriority::High,
            command_prefixes: &["channel/"],
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
        let p = Params::new(&params);

        match command {
            "channel/enqueue" => {
                let _timer = TimingGuard::new("module", "channel_enqueue");
                let persona_uuid = p.uuid("persona_id")?;
                let item = p.value("item").ok_or("Missing item")?;

                // Parse the item as ChannelEnqueueRequest
                let enqueue_request: ChannelEnqueueRequest = serde_json::from_value(item.clone())
                    .map_err(|e| format!("Invalid item: {e}"))?;

                let queue_item = enqueue_request.to_queue_item()?;

                let mut entry = self.state.registries
                    .entry(persona_uuid)
                    .or_insert_with(|| (ChannelRegistry::new(), PersonaState::new()));
                let (registry, _state) = entry.value_mut();

                match registry.route(queue_item) {
                    Ok(domain) => {
                        let status = registry.status();
                        Ok(CommandResult::Json(serde_json::json!({
                            "routed_to": domain,
                            "status": status,
                        })))
                    }
                    Err(e) => Err(e),
                }
            }

            "channel/dequeue" => {
                let _timer = TimingGuard::new("module", "channel_dequeue");
                let persona_uuid = p.uuid("persona_id")?;
                let domain_str = p.str_opt("domain");

                let mut entry = match self.state.registries.get_mut(&persona_uuid) {
                    Some(r) => r,
                    None => return Err(format!("No channel registry for {persona_uuid}")),
                };
                let (registry, _state) = entry.value_mut();

                // Parse optional domain filter
                let target_domain: Option<ActivityDomain> = match domain_str {
                    Some(d) => {
                        let domain: ActivityDomain = serde_json::from_value(serde_json::json!(d))
                            .map_err(|e| format!("Invalid domain '{d}': {e}"))?;
                        Some(domain)
                    }
                    None => None,
                };

                let item = match target_domain {
                    Some(d) => registry.get_mut(d).and_then(|ch| ch.pop()),
                    None => {
                        // Pop from highest-priority channel that has work
                        let mut popped = None;
                        for &d in DOMAIN_PRIORITY_ORDER {
                            if let Some(ch) = registry.get_mut(d) {
                                if let Some(item) = ch.pop() {
                                    popped = Some(item);
                                    break;
                                }
                            }
                        }
                        popped
                    }
                };

                match item {
                    Some(queue_item) => {
                        let json = queue_item.to_json();
                        Ok(CommandResult::Json(serde_json::json!({
                            "item": json,
                            "dequeued": true,
                        })))
                    }
                    None => {
                        Ok(CommandResult::Json(serde_json::json!({
                            "item": null,
                            "dequeued": false,
                        })))
                    }
                }
            }

            "channel/status" => {
                let _timer = TimingGuard::new("module", "channel_status");
                let persona_uuid = p.uuid("persona_id")?;

                let entry = match self.state.registries.get(&persona_uuid) {
                    Some(r) => r,
                    None => {
                        // Return empty status if no registry exists yet
                        return Ok(CommandResult::Json(serde_json::json!({
                            "channels": [],
                            "total_size": 0,
                            "has_urgent_work": false,
                            "has_work": false,
                        })));
                    }
                };
                let (registry, _state) = entry.value();

                let status = registry.status();
                Ok(CommandResult::Json(serde_json::to_value(&status).unwrap_or_default()))
            }

            "channel/service-cycle" => {
                let _timer = TimingGuard::new("module", "channel_service_cycle");
                let persona_uuid = p.uuid("persona_id")?;

                let mut entry = self.state.registries
                    .entry(persona_uuid)
                    .or_insert_with(|| (ChannelRegistry::new(), PersonaState::new()));
                let (registry, state) = entry.value_mut();

                let result = registry.service_cycle(state);
                Ok(CommandResult::Json(serde_json::to_value(&result).unwrap_or_default()))
            }

            "channel/service-cycle-full" => {
                let _timer = TimingGuard::new("module", "channel_service_cycle_full");
                let persona_uuid = p.uuid("persona_id")?;

                // Step 1: Service cycle — consolidate, schedule, return next item
                let service_result = {
                    let mut entry = self.state.registries
                        .entry(persona_uuid)
                        .or_insert_with(|| (ChannelRegistry::new(), PersonaState::new()));
                    let (registry, state) = entry.value_mut();
                    registry.service_cycle(state)
                };

                // Step 2: If item returned, run fast_path_decision in the SAME call
                let decision = if service_result.should_process {
                    if let Some(ref item_json) = service_result.item {
                        // Reconstruct InboxMessage from queue item JSON
                        let id = item_json.get("id")
                            .and_then(|v| v.as_str())
                            .and_then(|s| Uuid::parse_str(s).ok())
                            .unwrap_or_default();
                        let sender_id = item_json.get("senderId")
                            .and_then(|v| v.as_str())
                            .and_then(|s| Uuid::parse_str(s).ok())
                            .unwrap_or_default();
                        let room_id = item_json.get("roomId")
                            .and_then(|v| v.as_str())
                            .and_then(|s| Uuid::parse_str(s).ok())
                            .unwrap_or_default();
                        let sender_name = item_json.get("senderName")
                            .and_then(|v| v.as_str())
                            .unwrap_or("Unknown")
                            .to_string();
                        let sender_type_str = item_json.get("senderType")
                            .and_then(|v| v.as_str())
                            .unwrap_or("human");
                        let content = item_json.get("content")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let timestamp = item_json.get("timestamp")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0);
                        let priority = item_json.get("priority")
                            .and_then(|v| v.as_f64())
                            .map(|p| p as f32)
                            .unwrap_or(0.5);

                        let inbox_msg = InboxMessage {
                            id,
                            room_id,
                            sender_id,
                            sender_name,
                            sender_type: match sender_type_str {
                                "persona" => SenderType::Persona,
                                "agent" => SenderType::Agent,
                                "system" => SenderType::System,
                                _ => SenderType::Human,
                            },
                            content,
                            timestamp,
                            priority,
                            source_modality: item_json.get("itemType")
                                .and_then(|v| v.as_str())
                                .map(|t| if t == "voice" { Some(Modality::Voice) } else { None })
                                .flatten(),
                            voice_session_id: item_json.get("voiceSessionId")
                                .and_then(|v| v.as_str())
                                .and_then(|s| Uuid::parse_str(s).ok()),
                        };

                        // Get cognition engine for fast-path decision
                        if let Some(engine) = self.state.cognition_engines.get(&persona_uuid) {
                            let decision = engine.fast_path_decision(&inbox_msg);
                            Some(serde_json::json!({
                                "should_respond": decision.should_respond,
                                "confidence": decision.confidence,
                                "reason": decision.reason,
                                "decision_time_ms": decision.decision_time_ms,
                                "fast_path_used": decision.fast_path_used,
                            }))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };

                // Return flat structure matching TypeScript's expected format
                Ok(CommandResult::Json(serde_json::json!({
                    "should_process": service_result.should_process,
                    "item": service_result.item,
                    "channel": service_result.channel,
                    "wait_ms": service_result.wait_ms,
                    "stats": service_result.stats,
                    "decision": decision,
                })))
            }

            "channel/clear" => {
                let _timer = TimingGuard::new("module", "channel_clear");
                let persona_uuid = p.uuid("persona_id")?;

                if let Some(mut entry) = self.state.registries.get_mut(&persona_uuid) {
                    let (registry, _state) = entry.value_mut();
                    registry.clear_all();
                }

                log_info!("module", "channel", "Cleared channels for {}", persona_uuid);
                Ok(CommandResult::Json(serde_json::json!({ "cleared": true })))
            }

            _ => Err(format!("Unknown channel command: {command}")),
        }
    }

    fn as_any(&self) -> &dyn Any { self }
}
