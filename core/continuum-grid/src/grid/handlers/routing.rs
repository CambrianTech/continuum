//! Dry-run projection of the grid router decision.

use super::super::{router::RouteDecision, GridState};
use crate::runtime::CommandResult;
use serde_json::{json, Value};
use std::sync::Arc;

/// grid/route — dry-run routing check.
pub async fn handle_route(state: &Arc<GridState>, params: Value) -> Result<CommandResult, String> {
    let command = params
        .get("targetCommand")
        .and_then(|v| v.as_str())
        .or_else(|| params.get("command").and_then(|v| v.as_str()))
        .ok_or("command or targetCommand parameter required")?;

    let decision = state.router.route(command, &params, &state.registry);

    match decision {
        RouteDecision::Refused(reason) => Err(reason.to_string()),
        RouteDecision::Local => Ok(CommandResult::Json(json!({
            "route": "local",
            "reason": "default or local capability",
        }))),
        RouteDecision::Remote { node, reason } => Ok(CommandResult::Json(json!({
            "route": "remote",
            "nodeId": node.node_id,
            "nodeName": node.node_name,
            "reason": reason,
        }))),
    }
}

