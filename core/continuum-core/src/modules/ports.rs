//! The `ports/*` service module: owns the node's port leases (the lease domain is
//! [`crate::code::ports`], in the hands crate) and installs them for her grounding.

use std::any::Any;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use crate::code::ports::{install_node_leases, NodeIdentity, PortLeases};
use crate::orm::OrmStore;
use crate::runtime::{CommandResult, LateBound, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use crate::sdk_codegen::DynCommand;

/// Owns the node's [`PortLeases`] and the `ports/*` verbs.
pub struct PortsModule {
    leases: Arc<LateBound<PortLeases>>,
    node: NodeIdentity,
}

impl PortsModule {
    /// `node` names this node once its core's airc handle is attached.
    pub fn new(node: NodeIdentity) -> Self {
        Self { leases: Arc::new(LateBound::new("port leases")), node }
    }
}

#[async_trait]
impl ServiceModule for PortsModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "ports",
            priority: ModulePriority::Normal,
            command_prefixes: &["ports/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(&self, ctx: &ModuleContext) -> Result<(), String> {
        let adapter = ctx.storage("ports")?.adapter("main").await?;
        let store = OrmStore::new(adapter).await.map_err(|e| e.to_string())?;
        let leases = Arc::new(PortLeases::load(store, Arc::clone(&self.node)).await.map_err(|e| e.to_string())?);
        install_node_leases(Arc::clone(&leases));
        self.leases.install(leases);
        Ok(())
    }

    fn commands(&self) -> Vec<Arc<dyn DynCommand>> {
        crate::commands::ports::command_objects(Arc::clone(&self.leases))
    }

    async fn handle_command(&self, command: &str, _params: Value) -> Result<CommandResult, String> {
        Err(format!("ports commands are on the typed registry; '{command}' has no legacy handler"))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::PeerId;
    use uuid::Uuid;

    // Regression: the production boot guard must accept every ports command.
    // Lease-store tests alone missed missing descriptors and shipped a startup panic.
    #[test]
    fn ports_module_registers_through_the_production_boot_guard() {
        let node: NodeIdentity = Arc::new(|| Some(PeerId::from_uuid(Uuid::from_u128(1))));
        crate::runtime::ModuleRegistry::new().register(Arc::new(PortsModule::new(node)));
    }
}
