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
use router::GridRouter;
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
                local_capabilities: RwLock::new(caps),
            }),
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
            if let Ok(CommandResult::Json(gpu_json)) = module.handle_command(&cmd, serde_json::json!({})).await {
                let gpu_name = gpu_json.get("gpu_name")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let vram = gpu_json.get("total_vram_mb")
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
                    eprintln!("[grid] Local capabilities: GPU={}, VRAM={}MB",
                        gpu_name.as_deref().unwrap_or("none"),
                        vram.unwrap_or(0));
                }
            }
        }

        for transport in &self.state.transports {
            match transport.start().await {
                Ok(()) => {
                    let addr = transport.local_address()
                        .map(|a| a.display_address())
                        .unwrap_or_else(|| "unknown".into());
                    eprintln!("[grid] Transport '{}' started: {}", transport.name(), addr);
                }
                Err(e) => {
                    eprintln!("[grid] Transport '{}' failed to start: {e} (non-fatal)", transport.name());
                }
            }
        }

        // Announce our capabilities on each started transport
        let caps = self.state.local_capabilities.read().await.clone();
        for transport in &self.state.transports {
            if transport.local_address().is_some() {
                let _ = transport.announce(&caps).await;

                // Spawn accept loop for incoming connections
                let transport = transport.clone();
                let state = self.state.clone();
                tokio::spawn(async move {
                    connection::accept_loop(transport, state).await;
                });
            }
        }

        Ok(())
    }

    async fn handle_command(&self, command: &str, params: Value) -> Result<CommandResult, String> {
        // Dispatch uses constants from commands.rs — no magic strings.
        match command {
            commands::STATUS   => handlers::handle_status(&self.state).await,
            commands::NODES    => handlers::handle_nodes(&self.state).await,
            commands::PING     => handlers::handle_ping(&self.state, params).await,
            commands::SEND     => handlers::handle_send(&self.state, params).await,
            commands::DISCOVER => handlers::handle_discover(&self.state).await,
            commands::PAIR     => handlers::handle_pair(&self.state, params).await,
            commands::TRUST    => handlers::handle_trust(&self.state, params).await,
            commands::AUDIT    => handlers::handle_audit(&self.state, params).await,
            commands::ROUTE    => handlers::handle_route(&self.state, params).await,
            commands::NODE_STATUS => handlers::handle_node_status(&self.state, params).await,
            commands::JOB_SUBMIT  => handlers::handle_job_submit(&self.state, params).await,
            commands::JOB_CONTROL => handlers::handle_job_control(&self.state, params).await,
            commands::JOB_QUEUE   => handlers::handle_job_queue(&self.state, params).await,
            _ => Err(format!("Unknown grid command: {command}")),
        }
    }

    async fn tick(&self) -> Result<(), String> {
        for transport in &self.state.transports {
            if let Ok(discovered) = transport.discover().await {
                for node in discovered {
                    self.state.registry.upsert_discovered(node);
                }
            }
        }
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

    fn as_any(&self) -> &dyn Any {
        self
    }
}
