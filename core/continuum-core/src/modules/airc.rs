//! ServiceModule adapter for Rust-native AIRC commands.

use crate::airc::{
    spawn_daemon_attach, AircEventTransport, AircQueueClient, AircRealtimeStore, CliAircQueueClient,
    DaemonAircEventTransport, InMemoryAircRealtimeStore, StoreAircEventTransport,
    TokioAircCommandRunner,
};
// `default_socket_path_in` retained for back-compat callers; deprecated,
// see `crate::airc::daemon_endpoint` module docs.
#[allow(deprecated)]
use crate::airc::default_socket_path_in;
use airc_core::RoomId;
use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use crate::sdk_codegen::DynCommand;
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

    /// The `airc/*` command surface is migrated to the typed `DynCommand`
    /// registry (see `commands()` / `crate::commands::airc`). No legacy arms
    /// remain; an unrouted name here is a registry/dispatch bug, so we fail
    /// loud naming the command rather than silently swallowing it.
    async fn handle_command(&self, command: &str, _params: Value) -> Result<CommandResult, String> {
        Err(format!(
            "airc command surface is migrated to the typed registry; \
             '{command}' has no legacy handler (this should route via route_object)"
        ))
    }

    /// Contribute the dep-holding `airc/*` family — the two seams this module
    /// owns (queue client + realtime event transport), wired into the three
    /// typed commands.
    fn commands(&self) -> Vec<Arc<dyn DynCommand>> {
        crate::commands::airc::command_objects(
            self.queue_client.clone(),
            self.event_transport.clone(),
        )
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
    //! The `airc/*` command BEHAVIOR is owned by the typed command files
    //! (`crate::commands::airc::{queue_scan, realtime_publish, realtime_replay}`)
    //! and tested there against the same seams. These module tests only lock
    //! the migration contract: the legacy handler fails loud, and `commands()`
    //! contributes the full family to the registry.

    use super::*;
    use crate::airc::{AircQueueListRequest, AircQueueScanResult};
    use serde_json::json;

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

    // what this catches: the airc/* surface is fully migrated to the typed
    // DynCommand registry — the legacy handle_command routes no arm and fails
    // loud naming the command (no silent fallback, [[no-fallbacks-ever]]) if
    // dispatch ever lands here instead of route_object.
    #[tokio::test]
    async fn legacy_handle_command_fails_loud() {
        let module = AircModule::with_queue_client(Arc::new(FakeQueueClient));
        let err = module
            .handle_command("airc/queue-scan", json!({}))
            .await
            .expect_err("migrated surface must not route via the legacy handler");
        assert!(
            err.contains("airc/queue-scan"),
            "error must name the command: {err}"
        );
        assert!(
            err.contains("migrated"),
            "error must explain the migration: {err}"
        );
    }

    // what this catches: the module contributes exactly the three typed airc
    // commands via commands() — the family the persona tool surface, the ACL,
    // codegen, and uu all read from the one registry.
    #[test]
    fn contributes_the_three_airc_commands() {
        let module = AircModule::with_queue_client(Arc::new(FakeQueueClient));
        let names: Vec<String> = module
            .commands()
            .iter()
            .map(|c| c.name().to_string())
            .collect();
        assert_eq!(
            names,
            vec!["airc/queue-scan", "airc/realtime-publish", "airc/realtime-replay"]
        );
    }
}
