//! ServiceModule adapter for Rust-native AIRC commands.

use crate::airc::{
    discover_airc_socket, discover_default_channel, discover_peer_id, spawn_daemon_attach,
    AircEventTransport, AircQueueClient, AircQueueListRequest, AircQueueScanParams,
    AircRealtimePublishParams, AircRealtimeReplayParams, AircRealtimeStore, CliAircQueueClient,
    DaemonAircEventTransport, InMemoryAircRealtimeStore, StoreAircEventTransport,
    TokioAircCommandRunner,
};
// `default_socket_path_in` retained for back-compat callers; deprecated,
// see `crate::airc::daemon_endpoint` module docs.
#[allow(deprecated)]
use crate::airc::default_socket_path_in;
use airc_core::RoomId;
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
    attach_socket_path: Option<std::path::PathBuf>,
    /// Channel (room) to attach to at `initialize()`. Required by airc's
    /// owner-core router model (`airc-daemon/src/server.rs:274`); without
    /// a channel the daemon rejects attach with "attach requires a
    /// channel in the owner-core model". Discovered via
    /// [`discover_default_channel`] alongside the socket path.
    attach_channel: Option<RoomId>,
}

impl AircModule {
    /// Construct without discovery — falls back to the deprecated local
    /// resolver. **Prefer [`AircModule::discover_and_construct`]** for
    /// any new caller; this `new()` exists only because back-compat
    /// callers (tests, legacy bootstrap) rely on the sync signature.
    /// The headless boot path (`ipc::start_server`) is moving to the
    /// async constructor + canonical socket path.
    pub fn new() -> Self {
        let airc_home = std::env::current_dir()
            .map(|dir| dir.join(".airc"))
            .unwrap_or_else(|_| std::path::PathBuf::from(".airc"));
        Self::with_daemon_home(airc_home)
    }

    /// Discover the airc daemon socket via [`discover_airc_socket`] (asks
    /// `airc ipc-endpoint` per airc#1095; auto-installs airc if missing)
    /// AND the default channel via [`discover_default_channel`] (parses
    /// `airc room` for the scope's current room channel — required by
    /// airc's owner-core router model). On any discovery failure, returns
    /// a degraded module that responds to `airc/*` commands via the
    /// in-memory store but performs no daemon attach — so the rest of
    /// continuum-core boots even when airc is unreachable (e.g. CI
    /// without network for auto-install) or the scope has no current
    /// room (fresh install before `airc room <name>`).
    pub async fn discover_and_construct() -> Self {
        let socket_path = match discover_airc_socket().await {
            Ok(path) => {
                tracing::info!(
                    socket_path = ?path,
                    "Discovered airc daemon socket via `airc ipc-endpoint`"
                );
                path
            }
            Err(error) => {
                tracing::warn!(
                    %error,
                    "airc socket discovery failed — AIRC inbound attach disabled. Realtime \
                     commands will use in-memory store; queue commands will fail loudly. \
                     Resolve: install airc manually or set AIRC_DAEMON_SOCKET; see error \
                     above for the suggested remedy."
                );
                return Self::with_queue_client(Arc::new(CliAircQueueClient::new(
                    TokioAircCommandRunner,
                )));
            }
        };

        let attach_channel = match discover_default_channel().await {
            Ok(uuid) => {
                tracing::info!(
                    channel = %uuid,
                    "Discovered airc default channel via `airc room`"
                );
                Some(RoomId::from_uuid(uuid))
            }
            Err(error) => {
                // Socket reachable but no channel — boot continues with
                // queue + realtime commands, just no inbound attach. The
                // common case is "fresh install, scope not yet subscribed
                // to any room"; the operator runs `airc room <name>` and
                // restarts to wire up the attach.
                tracing::warn!(
                    %error,
                    "airc default-channel discovery failed — AIRC inbound attach disabled. \
                     Resolve: run `airc room <name>` to subscribe the scope to a room, \
                     or set AIRC_DEFAULT_CHANNEL=<uuid> to pin a channel explicitly, then \
                     restart continuum-core."
                );
                None
            }
        };

        // Identity discovery: query the daemon's Status response for
        // this scope's peer_id. Used as `PublishRequest.from_peer` so
        // continuum's publishes carry real attribution instead of the
        // anonymous Uuid::nil placeholder. Failure is non-fatal — the
        // module degrades to anonymous publishes and logs the remedy.
        let from_peer = match discover_peer_id(&socket_path).await {
            Ok(peer) => {
                tracing::info!(
                    peer_id = %peer,
                    "Discovered airc scope peer_id via daemon Status"
                );
                peer
            }
            Err(error) => {
                tracing::warn!(
                    %error,
                    "airc peer_id discovery failed — publishes will use anonymous \
                     Uuid::nil from_peer (attribution will read as `00000000-…`). \
                     Resolve: set AIRC_PEER_ID=<uuid> to pin identity, or check that \
                     the daemon's Status RPC is responding."
                );
                uuid::Uuid::nil()
            }
        };
        let from_client = uuid::Uuid::new_v4();

        Self {
            queue_client: Arc::new(CliAircQueueClient::new(TokioAircCommandRunner)),
            event_transport: Arc::new(DaemonAircEventTransport::with_identity(
                Arc::new(airc_ipc::DaemonClient::new(socket_path.clone())),
                from_peer,
                from_client,
            )),
            attach_socket_path: Some(socket_path),
            attach_channel,
        }
    }

    pub fn with_daemon_home(airc_home: impl Into<std::path::PathBuf>) -> Self {
        let airc_home = airc_home.into();
        let socket_path = default_socket_path_in(&airc_home);
        Self {
            queue_client: Arc::new(CliAircQueueClient::new(TokioAircCommandRunner)),
            event_transport: Arc::new(DaemonAircEventTransport::new(socket_path.clone())),
            attach_socket_path: Some(socket_path),
            attach_channel: None,
        }
    }

    pub fn with_queue_client(queue_client: Arc<dyn AircQueueClient>) -> Self {
        Self {
            queue_client,
            event_transport: Arc::new(StoreAircEventTransport::new(Arc::new(
                InMemoryAircRealtimeStore::default(),
            ))),
            attach_socket_path: None,
            attach_channel: None,
        }
    }

    pub fn with_clients(
        queue_client: Arc<dyn AircQueueClient>,
        realtime_store: Arc<dyn AircRealtimeStore>,
    ) -> Self {
        Self {
            queue_client,
            event_transport: Arc::new(StoreAircEventTransport::new(realtime_store)),
            attach_socket_path: None,
            attach_channel: None,
        }
    }

    pub fn with_event_transport(
        queue_client: Arc<dyn AircQueueClient>,
        event_transport: Arc<dyn AircEventTransport>,
    ) -> Self {
        Self {
            queue_client,
            event_transport,
            attach_socket_path: None,
            attach_channel: None,
        }
    }

    /// The discovered airc daemon socket path, if discovery succeeded.
    /// Downstream modules (e.g. persona instance manager) read this to
    /// connect each citizen's `airc_lib::Airc` to the same per-machine
    /// daemon. `None` means the airc subsystem is in degraded mode
    /// (queue-only, no daemon attach) — citizens cannot be bootstrapped
    /// until socket discovery succeeds on a future server restart.
    pub fn daemon_socket(&self) -> Option<&std::path::Path> {
        self.attach_socket_path.as_deref()
    }

    /// The discovered default room (per `airc room` for this scope), if
    /// any. Used by the persona instance manager as the default landing
    /// room when bootstrapping a citizen — so a fresh persona shows up
    /// in the same room Joel publishes into, per the
    /// `personas-are-citizens-airc-is-identity-provider` doctrine ("I
    /// expect your general room and theirs to be the same room").
    pub fn default_room(&self) -> Option<RoomId> {
        self.attach_channel
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

    async fn initialize(&self, ctx: &ModuleContext) -> Result<(), String> {
        // Inbound attach requires BOTH a socket (where to connect) AND a
        // channel (what to subscribe to under airc's owner-core model).
        // Either being None disables the attach but lets the rest of
        // the module + the broader continuum-core boot — the operator
        // sees one of the warnings from `discover_and_construct` so the
        // remedy path is obvious.
        match (
            self.attach_socket_path.clone(),
            self.attach_channel,
        ) {
            (Some(socket_path), Some(channel)) => {
                spawn_daemon_attach(socket_path, channel, ctx.bus.clone(), &ctx.runtime);
            }
            (Some(_), None) | (None, Some(_)) | (None, None) => {
                // Already warned during construction; stay silent here
                // to avoid duplicate noise on every boot.
            }
        }
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
                let result = self.event_transport.publish(params).await?;
                CommandResult::json(&result)
            }
            "airc/realtime-replay" => {
                let params: AircRealtimeReplayParams = serde_json::from_value(params)
                    .map_err(|e| format!("invalid airc/realtime-replay params: {e}"))?;
                let result = self.event_transport.replay(params).await?;
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
                        name: "after_cursor",
                        param_type: "object",
                        required: false,
                        description: "Optional lamport cursor; replay starts strictly after (lamport, event_id).",
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

    #[async_trait]
    impl AircEventTransport for FakeEventTransport {
        async fn publish(
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

        async fn replay(
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
