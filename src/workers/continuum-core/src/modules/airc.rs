//! ServiceModule adapter for Rust-native AIRC commands.

use crate::airc::{
    AircEventTransport, AircQueueClient, AircQueueListRequest, AircQueueScanParams,
    AircRealtimePublishParams, AircRealtimeReplayParams, AircRealtimeStore, CliAircQueueClient,
    InMemoryAircRealtimeStore, StoreAircEventTransport, TokioAircCommandRunner,
};
use crate::runtime::{
    CommandResult, CommandSchema, ModuleConfig, ModuleContext, ModulePriority, ParamSchema,
    ServiceModule,
};
use async_trait::async_trait;
use serde_json::Value;
use std::any::Any;
use std::sync::Arc;

pub struct AircModule {
    queue_client: Arc<dyn AircQueueClient>,
    event_transport: Arc<dyn AircEventTransport>,
}

impl AircModule {
    pub fn new() -> Self {
        Self {
            queue_client: Arc::new(CliAircQueueClient::new(TokioAircCommandRunner)),
            event_transport: Arc::new(StoreAircEventTransport::new(Arc::new(
                InMemoryAircRealtimeStore::default(),
            ))),
        }
    }

    pub fn with_queue_client(queue_client: Arc<dyn AircQueueClient>) -> Self {
        Self {
            queue_client,
            event_transport: Arc::new(StoreAircEventTransport::new(Arc::new(
                InMemoryAircRealtimeStore::default(),
            ))),
        }
    }

    pub fn with_clients(
        queue_client: Arc<dyn AircQueueClient>,
        realtime_store: Arc<dyn AircRealtimeStore>,
    ) -> Self {
        Self {
            queue_client,
            event_transport: Arc::new(StoreAircEventTransport::new(realtime_store)),
        }
    }

    pub fn with_event_transport(
        queue_client: Arc<dyn AircQueueClient>,
        event_transport: Arc<dyn AircEventTransport>,
    ) -> Self {
        Self {
            queue_client,
            event_transport,
        }
    }
}

impl Default for AircModule {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ServiceModule for AircModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "airc",
            priority: ModulePriority::Normal,
            command_prefixes: &["airc/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 4,
            tick_interval: None,
        }
    }

    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
        Ok(())
    }

    async fn handle_command(&self, command: &str, params: Value) -> Result<CommandResult, String> {
        match command {
            "airc/queue-scan" => {
                let params: AircQueueScanParams = serde_json::from_value(params)
                    .map_err(|e| format!("invalid airc/queue-scan params: {e}"))?;
                let request = AircQueueListRequest::try_from(params)?;
                let result = self.queue_client.list_queue(request).await;
                CommandResult::json(&result)
            }
            "airc/realtime-publish" => {
                let params: AircRealtimePublishParams = serde_json::from_value(params)
                    .map_err(|e| format!("invalid airc/realtime-publish params: {e}"))?;
                let result = self.event_transport.publish(params)?;
                CommandResult::json(&result)
            }
            "airc/realtime-replay" => {
                let params: AircRealtimeReplayParams = serde_json::from_value(params)
                    .map_err(|e| format!("invalid airc/realtime-replay params: {e}"))?;
                let result = self.event_transport.replay(params)?;
                CommandResult::json(&result)
            }
            _ => Err(format!("Unknown airc command: {command}")),
        }
    }

    fn command_schemas(&self) -> Vec<CommandSchema> {
        vec![
            CommandSchema {
                name: "airc/queue-scan",
                description: "Rust-native AIRC queue scan for no-Node agent flywheel polling.",
                params: vec![
                    ParamSchema {
                        name: "repo",
                        param_type: "string",
                        required: true,
                        description: "GitHub repo in owner/name form, e.g. CambrianTech/continuum.",
                    },
                    ParamSchema {
                        name: "limit",
                        param_type: "number",
                        required: false,
                        description: "Maximum cards to return, 1..100.",
                    },
                    ParamSchema {
                        name: "owner",
                        param_type: "string",
                        required: false,
                        description: "Optional queue owner filter.",
                    },
                    ParamSchema {
                        name: "status",
                        param_type: "string",
                        required: false,
                        description: "Optional queue status filter.",
                    },
                    ParamSchema {
                        name: "airc_bin",
                        param_type: "string",
                        required: false,
                        description: "Optional AIRC binary path; defaults to PATH lookup.",
                    },
                    ParamSchema {
                        name: "timeout_ms",
                        param_type: "number",
                        required: false,
                        description: "Command timeout in milliseconds, 100..60000.",
                    },
                ],
            },
            CommandSchema {
                name: "airc/realtime-publish",
                description: "Publish a typed AIRC realtime envelope into the Rust replay/presence adapter.",
                params: vec![ParamSchema {
                    name: "envelope",
                    param_type: "object",
                    required: true,
                    description: "AircRealtimeEnvelope with delivery semantics matching its payload.",
                }],
            },
            CommandSchema {
                name: "airc/realtime-replay",
                description: "Replay bounded AIRC realtime envelopes for a room, optionally including active coalesced presence.",
                params: vec![
                    ParamSchema {
                        name: "room_id",
                        param_type: "string",
                        required: true,
                        description: "Room id to replay.",
                    },
                    ParamSchema {
                        name: "after_event_id",
                        param_type: "string",
                        required: false,
                        description: "Optional cursor event id; replay starts after this event when present.",
                    },
                    ParamSchema {
                        name: "limit",
                        param_type: "number",
                        required: false,
                        description: "Replay limit, clamped by the Rust adapter.",
                    },
                    ParamSchema {
                        name: "include_presence",
                        param_type: "boolean",
                        required: false,
                        description: "Include active coalesced presence in the response.",
                    },
                    ParamSchema {
                        name: "include_subscriptions",
                        param_type: "boolean",
                        required: false,
                        description: "Include active subscriber projections in the response.",
                    },
                    ParamSchema {
                        name: "include_peer_manifests",
                        param_type: "boolean",
                        required: false,
                        description: "Include active peer manifests for the room.",
                    },
                    ParamSchema {
                        name: "include_capability_index",
                        param_type: "boolean",
                        required: false,
                        description: "Include a capability-to-peer index derived from active peer manifests.",
                    },
                ],
            },
        ]
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::airc::{
        AircPresenceEvent, AircPresenceState, AircQueueScanResult, AircRealtimeDelivery,
        AircRealtimeEnvelope, AircRealtimePayload, AircRealtimePublishResult,
        AircRealtimeReplayResult,
    };
    use parking_lot::Mutex;
    use serde_json::json;
    use uuid::Uuid;

    const TEST_ROOM_ID: Uuid = Uuid::from_u128(0xA1);

    struct FakeQueueClient;

    #[async_trait]
    impl AircQueueClient for FakeQueueClient {
        async fn list_queue(&self, request: AircQueueListRequest) -> AircQueueScanResult {
            let command = request.args();
            AircQueueScanResult {
                ok: true,
                repo: request.repo,
                card_count: 0,
                statuses: Vec::new(),
                owners: Vec::new(),
                command,
                stdout_bytes: 0,
                stderr: String::new(),
                queue: None,
                error: None,
            }
        }
    }

    struct FakeEventTransport {
        published: Mutex<Vec<String>>,
    }

    impl FakeEventTransport {
        fn new() -> Self {
            Self {
                published: Mutex::new(Vec::new()),
            }
        }
    }

    impl AircEventTransport for FakeEventTransport {
        fn publish(
            &self,
            params: AircRealtimePublishParams,
        ) -> Result<AircRealtimePublishResult, String> {
            self.published.lock().push(params.envelope.event_id.clone());
            Ok(AircRealtimePublishResult {
                ok: true,
                event_id: params.envelope.event_id,
                room_id: params.envelope.room_id,
                delivery: AircRealtimeDelivery::Durable,
                stored_for_replay: true,
                coalesced_presence_key: None,
                replay_depth: 1,
                active_presence_count: 0,
                active_subscription_count: 0,
                active_peer_manifest_count: 0,
            })
        }

        fn replay(
            &self,
            params: AircRealtimeReplayParams,
        ) -> Result<AircRealtimeReplayResult, String> {
            Ok(AircRealtimeReplayResult {
                room_id: params.room_id,
                events: Vec::new(),
                cursor: None,
                active_presence: Vec::new(),
                active_subscriptions: Vec::new(),
                active_peer_manifests: Vec::new(),
                capability_index: Vec::new(),
            })
        }
    }

    #[tokio::test]
    async fn queue_scan_command_uses_queue_client() {
        let module = AircModule::with_queue_client(Arc::new(FakeQueueClient));
        let result = module
            .handle_command(
                "airc/queue-scan",
                json!({
                    "repo": "CambrianTech/continuum",
                    "limit": 2
                }),
            )
            .await
            .unwrap();

        let CommandResult::Json(value) = result else {
            panic!("expected JSON result");
        };
        assert_eq!(value["ok"], true);
        assert_eq!(value["repo"], "CambrianTech/continuum");
        assert_eq!(value["command"][0], "queue");
        assert_eq!(value["command"][1], "list");
    }

    #[tokio::test]
    async fn realtime_publish_and_replay_roundtrip_through_module() {
        let module = AircModule::with_queue_client(Arc::new(FakeQueueClient));
        let envelope = AircRealtimeEnvelope::new(
            "typing-1".to_string(),
            TEST_ROOM_ID,
            "persona-1".to_string(),
            100,
            AircRealtimePayload::Presence {
                event: AircPresenceEvent {
                    room_id: TEST_ROOM_ID,
                    subject_id: "persona-1".to_string(),
                    display_name: None,
                    state: AircPresenceState::Typing,
                    started_at_ms: 100,
                    expires_at_ms: Some(500),
                    call_id: None,
                },
            },
        );

        let publish = module
            .handle_command("airc/realtime-publish", json!({ "envelope": envelope }))
            .await
            .unwrap();
        let CommandResult::Json(publish_value) = publish else {
            panic!("expected JSON publish result");
        };
        assert_eq!(publish_value["storedForReplay"], false);
        assert_eq!(publish_value["activePresenceCount"], 1);

        let replay = module
            .handle_command(
                "airc/realtime-replay",
                json!({
                    "roomId": TEST_ROOM_ID.to_string(),
                    "includePresence": true,
                    "nowMs": 499
                }),
            )
            .await
            .unwrap();
        let CommandResult::Json(replay_value) = replay else {
            panic!("expected JSON replay result");
        };
        assert_eq!(replay_value["events"].as_array().unwrap().len(), 0);
        assert_eq!(replay_value["activePresence"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn realtime_publish_uses_event_transport_seam() {
        let transport = Arc::new(FakeEventTransport::new());
        let module = AircModule::with_event_transport(Arc::new(FakeQueueClient), transport.clone());
        let envelope = AircRealtimeEnvelope::new(
            "evt-through-transport".to_string(),
            TEST_ROOM_ID,
            "persona-1".to_string(),
            100,
            AircRealtimePayload::Presence {
                event: AircPresenceEvent {
                    room_id: TEST_ROOM_ID,
                    subject_id: "persona-1".to_string(),
                    display_name: None,
                    state: AircPresenceState::Online,
                    started_at_ms: 100,
                    expires_at_ms: None,
                    call_id: None,
                },
            },
        );

        let result = module
            .handle_command("airc/realtime-publish", json!({ "envelope": envelope }))
            .await
            .unwrap();

        let CommandResult::Json(value) = result else {
            panic!("expected JSON result");
        };
        assert_eq!(value["eventId"], "evt-through-transport");
        assert_eq!(transport.published.lock()[0], "evt-through-transport");
    }
}
