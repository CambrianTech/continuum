//! GridModule — the Grid transport and routing ServiceModule.
//!
//! Transport layer is polymorphic (OpenCV-style):
//! - TailscaleTransport: TCP over WireGuard mesh (working NOW)
//! - ReticulumTransport: Encrypted identity-based mesh (future)
//!
//! File organization:
//! - mod.rs: GridState, GridModule, ServiceModule impl (this file)
//! - commands.rs: Command name constants + schemas (single source of truth)
//! - handlers.rs: One function per grid/* command
//! - connection.rs: Accept loop + incoming request processing
//! - helpers.rs: Shared utilities
//! - transport.rs: GridTransport + GridConnection traits
//! - frame.rs: Wire protocol types
//! - node.rs: Node identity, capability, addressing types
//! - router.rs: Routing decisions (local vs remote)
//! - registry.rs: Known node registry (persistence)
//! - acl.rs: Command access control
//! - audit.rs: Audit trail logging
//! - transports/: Transport implementations

#[cfg(test)]
mod tests;

pub mod acl;
pub mod audit;
pub mod commands;
pub mod connection;
pub mod frame;
pub mod handlers;
pub mod helpers;
pub mod node;
pub mod registry;
pub mod router;
pub mod transport;
pub mod transports;

use crate::runtime::{
    CommandResult, CommandSchema, ModuleConfig, ModuleContext, ModulePriority, ServiceModule,
};
use audit::AuditLog;
use dashmap::DashMap;
use frame::GridFrame;
use node::NodeCapability;
use registry::NodeRegistry;
use router::{GridRouter, RouteDecision};
use transport::GridTransport;
use transports::reticulum::ReticulumTransport;
use transports::tailscale::TailscaleTransport;

use async_trait::async_trait;
use serde_json::Value;
use std::any::Any;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{oneshot, Mutex, RwLock};

// ============================================================================
// GridState — shared state for all grid operations
// ============================================================================

/// State shared across GridModule handlers and connection tasks.
pub struct GridState {
    pub(crate) transports: Vec<Arc<dyn GridTransport>>,
    pub(crate) registry: Arc<NodeRegistry>,
    pub(crate) router: GridRouter,
    pub(crate) audit: Arc<AuditLog>,
    pub(crate) pending: DashMap<String, oneshot::Sender<GridFrame>>,
    pub(crate) grid_dir: PathBuf,
    pub(crate) runtime_registry: Mutex<Option<Arc<crate::runtime::ModuleRegistry>>>,
    pub(crate) bus: Mutex<Option<Arc<crate::runtime::MessageBus>>>,
    /// Substrate-wide command executor — installed by `start_server`
    /// after the executor is built. Inbound grid requests for commands
    /// that no Rust module owns fall through to `executor.execute_ts_json`
    /// (task #224 replaced the deleted free-function helper).
    pub(crate) executor: Mutex<Option<Arc<crate::runtime::CommandExecutor>>>,
    /// This node's capabilities (GPU, storage, inference, training).
    /// Populated at init from constructor params, enriched after GpuModule responds.
    pub(crate) local_capabilities: RwLock<Vec<NodeCapability>>,
}

// ============================================================================
// GridModule — the ServiceModule
// ============================================================================

pub struct GridModule {
    state: Arc<GridState>,
}

impl GridModule {
    pub fn new(grid_dir: PathBuf, local_has_gpu: bool, local_vram_mb: u64) -> Self {
        let registry = Arc::new(NodeRegistry::new(&grid_dir));
        let router = GridRouter::new(local_has_gpu, local_vram_mb);
        let audit = Arc::new(AuditLog::new(&grid_dir));

        let tailscale: Arc<dyn GridTransport> = Arc::new(TailscaleTransport::with_default_port());
        let reticulum: Arc<dyn GridTransport> = Arc::new(ReticulumTransport::new(grid_dir.clone()));

        // Build initial capabilities from constructor params.
        // Enriched with GPU name after GpuModule responds (in initialize).
        let mut caps = Vec::new();
        if local_has_gpu {
            caps.push(NodeCapability::Compute {
                gpu: None, // Enriched later with actual GPU name
                vram_mb: Some(local_vram_mb),
            });
        }

        Self {
            state: Arc::new(GridState {
                transports: vec![tailscale, reticulum],
                registry,
                router,
                audit,
                pending: DashMap::new(),
                grid_dir,
                runtime_registry: Mutex::new(None),
                bus: Mutex::new(None),
                executor: Mutex::new(None),
                local_capabilities: RwLock::new(caps),
            }),
        }
    }

    /// Get a clone of the shared `Arc<GridState>` for use by external
    /// consumers (notably `runtime::grid_interceptor::GridInterceptor`).
    ///
    /// The state holds the router + node registry + transports — every
    /// piece needed to make a remote-routing decision. Exposing it as
    /// `Arc` lets the kernel install the GridInterceptor at startup
    /// without taking ownership of GridState (which is GridModule's).
    pub fn state(&self) -> Arc<GridState> {
        self.state.clone()
    }
}

impl GridState {
    /// Apply the routing policy to a command. If the policy decides
    /// this node should handle it locally, returns `Ok(None)` — the
    /// caller (typically `runtime::grid_interceptor::GridInterceptor`)
    /// declines so the kernel can fall through to local Rust + TS
    /// dispatch. If the policy picks a remote node, dispatches the
    /// command over the grid wire and returns `Ok(Some(result))`.
    ///
    /// Errors propagate; the interceptor surfaces them to the caller
    /// per the `CommandInterceptor` contract (no silent fallthrough
    /// on Err). Examples: transport unreachable, remote command timed
    /// out, remote returned error.
    ///
    /// This is the kernel's hook into grid routing — the SAME primitive
    /// the explicit `grid/send` command goes through, just driven by
    /// policy rather than by an explicit `nodeId` param. One dispatch
    /// path, two callers (explicit + implicit).
    pub async fn try_route_remote(
        self: &Arc<Self>,
        command: &str,
        params: &serde_json::Value,
    ) -> Result<Option<crate::runtime::CommandResult>, String> {
        match self.router.route(command, params, &self.registry) {
            RouteDecision::Local => Ok(None),
            RouteDecision::Remote { node, reason } => {
                tracing::debug!(
                    "GridState::try_route_remote: routing '{}' to {} (reason: {})",
                    command,
                    node.node_id,
                    reason
                );
                let result =
                    handlers::dispatch_to_node(self, &node, command, params.clone()).await?;
                Ok(Some(result))
            }
        }
    }
}

#[async_trait]
impl ServiceModule for GridModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "grid",
            priority: ModulePriority::Normal,
            command_prefixes: &["grid/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: Some(Duration::from_secs(60)),
        }
    }

    async fn initialize(&self, ctx: &ModuleContext) -> Result<(), String> {
        *self.state.runtime_registry.lock().await = Some(ctx.registry.clone());
        *self.state.bus.lock().await = Some(ctx.bus.clone());

        // Enrich local capabilities by querying GPU module for hardware details
        if let Some((module, cmd)) = ctx.registry.route_command("gpu/stats") {
            if let Ok(CommandResult::Json(gpu_json)) =
                module.handle_command(&cmd, serde_json::json!({})).await
            {
                let gpu_name = gpu_json
                    .get("gpu_name")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let vram = gpu_json
                    .get("total_vram_mb")
                    .and_then(|v| v.as_f64())
                    .map(|v| v as u64);

                if gpu_name.is_some() || vram.is_some() {
                    let mut caps = self.state.local_capabilities.write().await;
                    // Replace the placeholder Compute capability with enriched data
                    caps.retain(|c| !matches!(c, NodeCapability::Compute { .. }));
                    if vram.unwrap_or(0) > 0 || gpu_name.is_some() {
                        caps.push(NodeCapability::Compute {
                            gpu: gpu_name.clone(),
                            vram_mb: vram,
                        });
                    }
                    eprintln!(
                        "[grid] Local capabilities: GPU={}, VRAM={}MB",
                        gpu_name.as_deref().unwrap_or("none"),
                        vram.unwrap_or(0)
                    );
                }
            }
        }

        // Enrich with a local forge custodian if one is reachable (Contract C §5,
        // Pass 5b). Capability is OBSERVED — we probe the custodian's /health and
        // only advertise forge when it answered. No custodian ⇒ no forge cap (an
        // honest absence, not a fallback). Bounded so a hung port can't stall grid
        // bringup; forge is optional infra. The fabric re-probes for live health.
        match tokio::time::timeout(
            Duration::from_secs(2),
            crate::forge::endpoint::ForgeEndpoint::probe_local(),
        )
        .await
        {
            Ok(Some(endpoint)) => {
                let mut caps = self.state.local_capabilities.write().await;
                caps.retain(|c| !matches!(c, NodeCapability::Forge { .. }));
                eprintln!(
                    "[grid] Local capabilities: forge custodian reachable ({:?}, {} slots)",
                    endpoint.health, endpoint.capacity
                );
                caps.push(NodeCapability::Forge { endpoint });
            }
            Ok(None) => {} // no local custodian — correctly advertise no forge cap
            Err(_) => eprintln!("[grid] Local forge custodian probe timed out — not advertised"),
        }

        for transport in &self.state.transports {
            match transport.start().await {
                Ok(()) => {
                    let addr = transport
                        .local_address()
                        .map(|a| a.display_address())
                        .unwrap_or_else(|| "unknown".into());
                    eprintln!("[grid] Transport '{}' started: {}", transport.name(), addr);
                }
                Err(e) => {
                    let hint = match transport.name() {
                        "tailscale" => " — install Tailscale and run 'tailscale up' to enable grid",
                        "reticulum" => " — Reticulum transport not yet implemented",
                        _ => "",
                    };
                    eprintln!(
                        "[grid] Transport '{}' not available: {e}{hint}",
                        transport.name()
                    );
                }
            }
        }

        // Announce our capabilities on each started transport
        let caps = self.state.local_capabilities.read().await.clone();
        let mut active_transports = 0;
        for transport in &self.state.transports {
            if transport.local_address().is_some() {
                active_transports += 1;
                let _ = transport.announce(&caps).await;

                // Spawn accept loop for incoming connections
                let transport = transport.clone();
                let state = self.state.clone();
                tokio::spawn(async move {
                    connection::accept_loop(transport, state).await;
                });
            }
        }

        let known = self.state.registry.all_nodes().len();
        let online = self.state.registry.online_nodes().len();
        eprintln!("[grid] Ready: {active_transports} transport(s), {known} known node(s) ({online} online). Run 'grid/setup-check' for diagnostics.");

        Ok(())
    }

    async fn handle_command(&self, command: &str, params: Value) -> Result<CommandResult, String> {
        // Dispatch uses constants from commands.rs — no magic strings.
        match command {
            commands::STATUS => handlers::handle_status(&self.state).await,
            commands::NODES => handlers::handle_nodes(&self.state).await,
            commands::PING => handlers::handle_ping(&self.state, params).await,
            commands::SEND => handlers::handle_send(&self.state, params).await,
            commands::DISCOVER => handlers::handle_discover(&self.state).await,
            commands::PAIR => handlers::handle_pair(&self.state, params).await,
            commands::TRUST => handlers::handle_trust(&self.state, params).await,
            commands::AUDIT => handlers::handle_audit(&self.state, params).await,
            commands::ROUTE => handlers::handle_route(&self.state, params).await,
            commands::NODE_STATUS => handlers::handle_node_status(&self.state, params).await,
            commands::JOB_SUBMIT => handlers::handle_job_submit(&self.state, params).await,
            commands::JOB_CONTROL => handlers::handle_job_control(&self.state, params).await,
            commands::JOB_QUEUE => handlers::handle_job_queue(&self.state, params).await,
            commands::SETUP_CHECK => handlers::handle_setup_check(&self.state).await,
            _ => Err(format!("Unknown grid command: {command}")),
        }
    }

    async fn tick(&self) -> Result<(), String> {
        // Discover peers from transports — just read the peer list, no blocking probes.
        // Probing happens in a spawned background task to avoid blocking IPC.
        for transport in &self.state.transports {
            if let Ok(discovered) = transport.discover().await {
                for node in discovered {
                    let addr = &node.address;
                    if let node::TransportAddress::Tailscale { ip, .. } = addr {
                        // Skip blocked nodes
                        if let Some(existing) = self.state.registry.get(ip) {
                            if existing.trust_level == node::TrustLevel::Blocked {
                                continue;
                            }
                        }
                    }
                    // Register discovered nodes optimistically.
                    // They'll be probed in background and removed if unreachable.
                    self.state.registry.upsert_discovered(node);
                }
            }
        }

        // #2228: fold the auto-discovered gossip peers into the registry by their DURABLE
        // identity. The capacity beacon already self-registers each peer in the global ledger
        // (PeerId-keyed, live capacity); this CONSUMES that correlation so a beaconing peer
        // becomes a routable node with its `peer_id` set — the grid figures out node identities
        // automatically, no manual `grid/pair`. Trust stays default (discovery ≠ authorization,
        // #38), so the node is visible to pricing but not sent work until trusted.
        for (peer_uuid, offer) in crate::capacity::gossip::global_ledger().heard_offers() {
            let vram_mb = (offer.gpu_total_bytes / (1024 * 1024)).max(1);
            if self
                .state
                .registry
                .ensure_peer_node(crate::identity::PeerId::from_uuid(peer_uuid), Some(vram_mb))
            {
                crate::probe!(
                    class = "grid.peer.autocorrelated",
                    peer = %peer_uuid,
                    vram_mb = vram_mb,
                    "auto-registered a beaconing grid peer by its durable PeerId (#2228)",
                );
            }
        }

        // Background probe: check which nodes are actually reachable.
        // Spawned so it doesn't block IPC command handling.
        let registry = Arc::clone(&self.state.registry);
        let bus = self.state.bus.lock().await.clone();
        tokio::spawn(async move {
            let nodes = registry.all_nodes();
            for node in &nodes {
                if node.trust_level == node::TrustLevel::Blocked {
                    continue;
                }
                for addr in &node.addresses {
                    if let node::TransportAddress::Tailscale { ip, port, .. } = addr {
                        let target = format!("{ip}:{port}");
                        match tokio::time::timeout(
                            Duration::from_secs(2),
                            tokio::net::TcpStream::connect(&target),
                        )
                        .await
                        {
                            Ok(Ok(_)) => {
                                registry.update_latency(&node.node_id, 0);
                            }
                            _ => {
                                // Unreachable — only remove auto-discovered (default trust) nodes.
                                // Owner/Trusted nodes stay but age out of online_nodes().
                                if node.trust_level == node::TrustLevel::default() {
                                    registry.remove(&node.node_id);
                                    eprintln!(
                                        "[grid] Removed unreachable node {} ({})",
                                        node.node_name.as_deref().unwrap_or("?"),
                                        node.node_id
                                    );
                                    if let Some(bus) = &bus {
                                        bus.publish_async_only(
                                            "grid:node:left",
                                            serde_json::json!({
                                                "nodeId": node.node_id,
                                            }),
                                        );
                                    }
                                }
                            }
                        }
                        break;
                    }
                }
            }
            let _ = registry.save_to_disk();
        });

        let _ = self.state.registry.save_to_disk();
        Ok(())
    }

    async fn shutdown(&self) -> Result<(), String> {
        for transport in &self.state.transports {
            let _ = transport.shutdown().await;
        }
        let _ = self.state.registry.save_to_disk();
        Ok(())
    }

    fn command_schemas(&self) -> Vec<CommandSchema> {
        // Schemas defined in commands.rs alongside the constants — single source of truth.
        commands::schemas()
    }

    fn install_executor(&self, executor: Arc<crate::runtime::CommandExecutor>) {
        // Mutex::lock blocks briefly; called once at boot, never on hot path.
        if let Ok(mut guard) = self.state.executor.try_lock() {
            *guard = Some(executor);
        } else {
            // Should not happen — install_executor is called exactly once during start_server
            // before any inbound command lands. If we ever contend here, surface the lost
            // executor install loudly.
            tracing::error!("GridModule::install_executor lost mutex contention at boot");
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
