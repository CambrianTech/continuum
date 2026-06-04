//! ServiceModule adapter for Rust-native AIRC commands.

use crate::airc::{
    spawn_daemon_attach, AircEventTransport, AircQueueClient, AircQueueListRequest,
    AircQueueScanParams, AircRealtimePublishParams, AircRealtimeReplayParams, AircRealtimeStore,
    CliAircQueueClient, DaemonAircEventTransport, InMemoryAircRealtimeStore,
    StoreAircEventTransport, TokioAircCommandRunner,
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
    /// Human-readable name of the default room (e.g. `"continuum"`).
    /// Used by the persona instance manager to join via
    /// `Airc::join(name)` — joining by the UUID-as-string derives a
    /// new channel from the string (caught by PR #1511 integration
    /// test: persona landed in derived channel `5d33e2a7…` when
    /// operator publishes go to `11c1a7ac…`). Discovered via
    /// [`discover_default_room_name`] alongside the socket + channel.
    attach_room_name: Option<String>,
}

impl AircModule {
    /// Construct without discovery — falls back to the deprecated local
    /// resolver. Used by back-compat callers (tests, legacy bootstrap)
    /// that rely on the sync signature. New callers should call
    /// `crate::airc::discover()` then `AircModule::from_discovery()`
    /// so the same typed state drives both module construction AND
    /// boot-state checks.
    pub fn new() -> Self {
        let airc_home = std::env::current_dir()
            .map(|dir| dir.join(".airc"))
            .unwrap_or_else(|_| std::path::PathBuf::from(".airc"));
        Self::with_daemon_home(airc_home)
    }

    /// Construct an `AircModule` from a typed `AircDiscovery`. The
    /// A.2 entry point: callers call `crate::airc::discover()` once
    /// and hand the result to `from_discovery()`. The same
    /// `AircDiscovery` value drives both module construction AND
    /// boot-state checks (`verify_registration`, persona-hosting
    /// gate), so the substrate has ONE answer to "what state is AIRC
    /// in" rather than three drifting representations.
    ///
    /// ### [[no-fallbacks-ever]] — Degraded ALWAYS collapses to queue-only
    ///
    /// Slice A's review caught the soft-fallback this method initially
    /// retained: a `Degraded { partial: { socket: Some(stale), peer_id:
    /// None } }` was constructing a real `DaemonAircEventTransport`
    /// against the stale socket with `Uuid::nil()` substituted for the
    /// missing peer_id. The substrate then "looked healthy" while
    /// every realtime publish either ECONNREFUSEd or went out
    /// unattributed. That IS the silent-substitution pattern
    /// [[no-fallbacks-ever]] forbids; the failure mode just shifted
    /// one frame deeper.
    ///
    /// After review: `Degraded` ALWAYS collapses to `with_queue_client`,
    /// regardless of what `partial` carries. The substrate refuses to
    /// build a real daemon transport against state discovery has
    /// declared not Healthy. The `partial` field stays on the
    /// `AircDiscovery::Degraded` variant for operator observability
    /// (the boot banner / log output can show what was resolved
    /// before the failure), but the module construction does not
    /// pretend.
    pub fn from_discovery(discovery: &crate::airc::AircDiscovery) -> Self {
        use crate::airc::AircDiscovery;
        match discovery {
            AircDiscovery::Healthy {
                socket,
                default_room,
                room_name,
                peer_id,
            } => {
                let from_client = uuid::Uuid::new_v4();
                Self {
                    queue_client: Arc::new(CliAircQueueClient::new(TokioAircCommandRunner)),
                    event_transport: Arc::new(DaemonAircEventTransport::with_identity(
                        Arc::new(airc_ipc::DaemonClient::new(socket.clone())),
                        *peer_id,
                        from_client,
                    )),
                    attach_socket_path: Some(socket.clone()),
                    attach_channel: Some(*default_room),
                    attach_room_name: Some(room_name.clone()),
                }
            }
            // Discovery declared not-Healthy → queue-only, no daemon
            // transport, no Uuid::nil fallback. Downstream realtime
            // commands return an actionable error referencing the
            // discovery reason, not a stale ECONNREFUSED.
            AircDiscovery::Degraded { .. } | AircDiscovery::Unreachable { .. } => {
                Self::with_queue_client(Arc::new(CliAircQueueClient::new(TokioAircCommandRunner)))
            }
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
            attach_room_name: None,
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
            attach_room_name: None,
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
            attach_room_name: None,
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
            attach_room_name: None,
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

    /// The human-readable name of the default room (e.g. `"continuum"`),
    /// if discovered. Used by the persona instance manager to join via
    /// `Airc::join(name)` — joining by the channel's UUID-as-string
    /// would derive a NEW channel and land the persona in the wrong
    /// room. Per PR #1511 integration trace.
    pub fn default_room_name(&self) -> Option<&str> {
        self.attach_room_name.as_deref()
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
mod from_discovery_tests {
    //! Lock in the structural invariants of `AircModule::from_discovery`
    //! that R1+R2 BLOCKed Slice A.2.1's first attempt on.
    //!
    //! The R2#1 BLOCK that drove the redesign: `Degraded` constructions
    //! must NOT build a `DaemonAircEventTransport` against the stale
    //! socket with a substituted `Uuid::nil()` peer_id. Doctrinally
    //! [[no-fallbacks-ever]]: the substrate refuses to construct an
    //! attribution-less daemon transport that would silently send
    //! ECONNREFUSED-bound or unattributed frames.

    use super::*;
    use crate::airc::{AircDiscovery, DiscoveryFailure, PartialDiscovery};
    use airc_core::RoomId;
    use std::path::PathBuf;
    use uuid::Uuid;

    /// `Healthy` produces a fully-configured module: socket present,
    /// channel present, room name present. The substrate has full
    /// attribution and a live daemon transport.
    #[test]
    fn healthy_produces_fully_configured_module() {
        let socket = PathBuf::from("/tmp/healthy.sock");
        let room = RoomId::from_uuid(Uuid::new_v4());
        let peer = Uuid::new_v4();
        let discovery = AircDiscovery::Healthy {
            socket: socket.clone(),
            default_room: room,
            room_name: "general".into(),
            peer_id: peer,
        };

        let module = AircModule::from_discovery(&discovery);

        assert_eq!(
            module.daemon_socket(),
            Some(socket.as_path()),
            "Healthy must preserve the discovered socket"
        );
        assert_eq!(
            module.default_room(),
            Some(room),
            "Healthy must preserve the discovered channel"
        );
        assert_eq!(
            module.default_room_name(),
            Some("general"),
            "Healthy must preserve the discovered room name"
        );
    }

    /// `Degraded` with rich partial state (socket discovered, peer_id
    /// missing because Status RPC failed) collapses to queue-only.
    /// This is the explicit fix for R2#1 — the same pre-fix code path
    /// constructed a real `DaemonAircEventTransport` against the stale
    /// socket with `Uuid::nil()` substituted for the missing peer_id.
    /// After the fix the substrate refuses to construct that
    /// attribution-less transport.
    #[test]
    fn degraded_with_partial_socket_collapses_to_queue_only() {
        let stale_socket = PathBuf::from("/tmp/stale.sock");
        let discovery = AircDiscovery::Degraded {
            reason: DiscoveryFailure::StaleSocket(
                stale_socket.clone(),
                "ECONNREFUSED".into(),
            ),
            partial: PartialDiscovery {
                socket: Some(stale_socket.clone()),
                peer_id: None,
                default_room: None,
                room_name: None,
            },
        };

        let module = AircModule::from_discovery(&discovery);

        assert_eq!(
            module.daemon_socket(),
            None,
            "[[no-fallbacks-ever]] — Degraded MUST NOT expose a daemon \
             socket; that's the R2#1 stale-socket bug"
        );
        assert_eq!(module.default_room(), None);
        assert_eq!(module.default_room_name(), None);
    }

    /// `Degraded` with full partial state (everything discovered EXCEPT
    /// the failed step) ALSO collapses to queue-only. Even if discovery
    /// got far enough to know the room name, once one sub-step failed
    /// the substrate refuses to build a daemon transport. Discovery's
    /// "not Healthy" verdict is the gate.
    #[test]
    fn degraded_with_full_partial_state_still_collapses_to_queue_only() {
        let socket = PathBuf::from("/tmp/maybe.sock");
        let room = RoomId::from_uuid(Uuid::new_v4());
        let discovery = AircDiscovery::Degraded {
            reason: DiscoveryFailure::NoDefaultRoom,
            partial: PartialDiscovery {
                socket: Some(socket),
                peer_id: Some(Uuid::new_v4()),
                default_room: Some(room),
                room_name: Some("partial".into()),
            },
        };

        let module = AircModule::from_discovery(&discovery);

        assert_eq!(
            module.daemon_socket(),
            None,
            "Degraded verdict is binding even when partial is rich — \
             the substrate trusts discovery's classification"
        );
        assert_eq!(module.default_room(), None);
        assert_eq!(module.default_room_name(), None);
    }

    /// `Unreachable` collapses to queue-only. The substrate stays alive
    /// for non-AIRC commands; `airc/*` realtime commands return
    /// actionable errors based on `discovery.reason()`.
    #[test]
    fn unreachable_collapses_to_queue_only() {
        let discovery = AircDiscovery::Unreachable {
            reason: DiscoveryFailure::AutoInstallDisabled,
        };

        let module = AircModule::from_discovery(&discovery);

        assert_eq!(module.daemon_socket(), None);
        assert_eq!(module.default_room(), None);
        assert_eq!(module.default_room_name(), None);
    }

    /// Cross-variant invariant: `from_discovery` exposes a `daemon_socket()`
    /// ONLY for `Healthy`. Iterate every variant; only Healthy may
    /// return Some. This is the R2#1 fix made into a structural test —
    /// any future refactor that adds a fourth code path returning a
    /// real socket from a non-Healthy state will fail this test.
    #[test]
    fn only_healthy_exposes_a_daemon_socket() {
        let cases = [
            (
                AircDiscovery::Healthy {
                    socket: PathBuf::from("/tmp/h.sock"),
                    default_room: RoomId::from_uuid(Uuid::new_v4()),
                    room_name: "general".into(),
                    peer_id: Uuid::new_v4(),
                },
                true,
            ),
            (
                AircDiscovery::Degraded {
                    reason: DiscoveryFailure::NoDefaultRoom,
                    partial: PartialDiscovery {
                        socket: Some(PathBuf::from("/tmp/d.sock")),
                        ..Default::default()
                    },
                },
                false,
            ),
            (
                AircDiscovery::Unreachable {
                    reason: DiscoveryFailure::EmptyPath,
                },
                false,
            ),
        ];
        for (discovery, expect_socket_some) in cases {
            let module = AircModule::from_discovery(&discovery);
            assert_eq!(
                module.daemon_socket().is_some(),
                expect_socket_some,
                "daemon_socket() polarity wrong for {:?}",
                discovery.kind()
            );
        }
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
