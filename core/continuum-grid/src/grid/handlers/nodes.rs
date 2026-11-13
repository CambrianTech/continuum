//! Fleet discovery and local inbound AIRC policy are distinct facts.
use super::{CommandResult, GridNode, GridState, Arc};
use crate::grid::access_decision::{self, AccessPolicy, LocalInboundAircAuthorization};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub(crate) struct GridNodeView {
    /// Existing fields, including trust_level, remain legacy routing facts.
    #[serde(flatten)]
    pub node: GridNode,
    /// Local tier policy if this correlated peer arrives through authenticated AIRC.
    /// Not proof of enrollment, a command grant, or permission on the remote node.
    pub local_inbound_airc_authorization: LocalInboundAircAuthorization,
}

impl GridNodeView {
    pub(crate) fn from_policy(node: GridNode, policy: &AccessPolicy) -> Self {
        let authorization = policy.local_inbound_airc_authorization(node.peer_id.map(|peer| peer.as_uuid()));
        Self { node, local_inbound_airc_authorization: authorization }
    }
}

pub async fn handle_nodes(state: &Arc<GridState>) -> Result<CommandResult, String> {
    let policy = access_decision::policy();
    let nodes: Vec<_> = state.registry.all_nodes().into_iter()
        .map(|node| GridNodeView::from_policy(node, &policy)).collect();
    CommandResult::json(&nodes)
}
