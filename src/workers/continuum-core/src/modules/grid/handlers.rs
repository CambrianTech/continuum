//! Grid command handlers — one function per grid/* command.
//!
//! Each handler takes GridState + params, returns CommandResult.
//! Separated from the ServiceModule impl for testability and clarity.

use super::audit::{AuditDirection, AuditEntry, AuditOutcome};
use super::frame::{self, GridFrame, GridPayload};
use super::helpers::{correlation_id, find_transport_for_address, parse_trust_level};
use super::node::{GridNode, TransportAddress};
use super::router::RouteDecision;
use super::GridState;
use crate::runtime::CommandResult;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;

/// grid/status — transport status and local identity.
pub async fn handle_status(state: &Arc<GridState>) -> Result<CommandResult, String> {
    let mut transport_status = Vec::new();
    for t in &state.transports {
        transport_status.push(json!({
            "name": t.name(),
            "connected": t.local_address().is_some(),
            "address": t.local_address().map(|a| a.display_address()),
            "encrypted": t.provides_encryption(),
        }));
    }

    let nodes = state.registry.all_nodes();
    let online = state.registry.online_nodes();

    Ok(CommandResult::Json(json!({
        "transports": transport_status,
        "totalNodes": nodes.len(),
        "onlineNodes": online.len(),
        "gridDir": state.grid_dir.to_string_lossy(),
    })))
}

/// grid/nodes — list known nodes.
pub async fn handle_nodes(state: &Arc<GridState>) -> Result<CommandResult, String> {
    let nodes = state.registry.all_nodes();
    CommandResult::json(&nodes)
}

/// grid/ping — round-trip latency to a remote node.
pub async fn handle_ping(state: &Arc<GridState>, params: Value) -> Result<CommandResult, String> {
    let node_id = params.get("nodeId").and_then(|v| v.as_str())
        .ok_or("nodeId parameter required")?;

    let node = state.registry.get(node_id)
        .ok_or_else(|| format!("Unknown node: {node_id}"))?;

    let address = node.addresses.first()
        .ok_or_else(|| format!("Node {node_id} has no addresses"))?;

    let transport = find_transport_for_address(&state.transports, address)
        .ok_or_else(|| format!("No transport available for {}", address.display_address()))?;

    let start = std::time::Instant::now();

    let conn = transport.connect(address).await
        .map_err(|e| format!("Connect failed: {e}"))?;

    let our_address = transport.local_address()
        .map(|a| a.display_address())
        .unwrap_or_else(|| "unknown".into());

    let ping_frame = GridFrame::command_request(
        format!("ping-{}", correlation_id()),
        our_address,
        node_id.to_string(),
        super::commands::PING.to_string(),
        json!({}),
    );

    conn.send_frame(&ping_frame).await
        .map_err(|e| format!("Send failed: {e}"))?;

    let response = tokio::time::timeout(
        Duration::from_secs(10),
        conn.recv_frame(),
    )
    .await
    .map_err(|_| "Ping timed out (10s)".to_string())?
    .map_err(|e| format!("Recv failed: {e}"))?;

    let latency_ms = start.elapsed().as_millis() as u64;
    let _ = conn.close().await;

    state.registry.update_latency(node_id, latency_ms);

    Ok(CommandResult::Json(json!({
        "nodeId": node_id,
        "nodeName": node.node_name,
        "latencyMs": latency_ms,
        "transport": transport.name(),
        "responseType": format!("{:?}", response.frame_type),
    })))
}

/// grid/send — execute a command on a remote node.
pub async fn handle_send(state: &Arc<GridState>, params: Value) -> Result<CommandResult, String> {
    let node_id = params.get("nodeId").and_then(|v| v.as_str())
        .ok_or("nodeId parameter required")?;
    // TS mixin sends as 'remoteCommand' to avoid collision with IPC 'command' field.
    // Also accept 'command' for direct Rust callers.
    let remote_command = params.get("remoteCommand").and_then(|v| v.as_str())
        .or_else(|| params.get("command").and_then(|v| v.as_str()))
        .ok_or("command or remoteCommand parameter required")?;
    let remote_params = params.get("params").cloned().unwrap_or(json!({}));

    let node = state.registry.get(node_id)
        .ok_or_else(|| format!("Unknown node: {node_id}"))?;

    let address = node.addresses.first()
        .ok_or_else(|| format!("Node {node_id} has no addresses"))?;

    let transport = find_transport_for_address(&state.transports, address)
        .ok_or_else(|| format!("No transport for {}", address.display_address()))?;

    let our_address = transport.local_address()
        .map(|a| a.display_address())
        .unwrap_or_else(|| "unknown".into());

    let corr_id = format!("cmd-{}", correlation_id());
    let frame = GridFrame::command_request(
        corr_id.clone(),
        our_address,
        node_id.to_string(),
        remote_command.to_string(),
        remote_params,
    );

    let start = std::time::Instant::now();

    let conn = transport.connect(address).await
        .map_err(|e| format!("Connect to {node_id} failed: {e}"))?;

    conn.send_frame(&frame).await
        .map_err(|e| format!("Send to {node_id} failed: {e}"))?;

    // 5 minute timeout for long operations (training, etc.)
    let response = tokio::time::timeout(
        Duration::from_secs(300),
        conn.recv_frame(),
    )
    .await
    .map_err(|_| format!("Command '{remote_command}' on {node_id} timed out (300s)"))?
    .map_err(|e| format!("Recv from {node_id} failed: {e}"))?;

    let duration_ms = start.elapsed().as_millis() as u64;
    let _ = conn.close().await;

    // Audit
    let outcome = match &response.payload {
        GridPayload::CommandResult { success: true, .. } => AuditOutcome::Success,
        _ => AuditOutcome::Error,
    };

    let _ = state.audit.log(&AuditEntry {
        timestamp: frame::now_millis(),
        direction: AuditDirection::Outbound,
        remote_node: node_id.to_string(),
        command: remote_command.to_string(),
        correlation_id: corr_id,
        outcome,
        duration_ms,
    }).await;

    match response.payload {
        GridPayload::CommandResult { success: true, result, .. } => {
            Ok(CommandResult::Json(result.unwrap_or(json!({"success": true}))))
        }
        GridPayload::CommandResult { success: false, error, .. } => {
            Err(format!("Remote command failed: {}", error.unwrap_or_default()))
        }
        _ => Err("Unexpected response frame type".into()),
    }
}

/// grid/discover — trigger transport-level discovery.
pub async fn handle_discover(state: &Arc<GridState>) -> Result<CommandResult, String> {
    let mut total_discovered = 0;
    let mut transport_results = Vec::new();

    for transport in &state.transports {
        match transport.discover().await {
            Ok(nodes) => {
                let count = nodes.len();
                for node in nodes {
                    state.registry.upsert_discovered(node);
                }
                transport_results.push(json!({
                    "transport": transport.name(),
                    "discovered": count,
                }));
                total_discovered += count;
            }
            Err(e) => {
                transport_results.push(json!({
                    "transport": transport.name(),
                    "error": e.to_string(),
                }));
            }
        }
    }

    let _ = state.registry.save_to_disk();

    Ok(CommandResult::Json(json!({
        "totalDiscovered": total_discovered,
        "transports": transport_results,
    })))
}

/// grid/pair — register a new node with trust level.
pub async fn handle_pair(state: &Arc<GridState>, params: Value) -> Result<CommandResult, String> {
    let address_str = params.get("address").and_then(|v| v.as_str())
        .ok_or("address parameter required")?;
    let name = params.get("name").and_then(|v| v.as_str());
    let trust_str = params.get("trust").and_then(|v| v.as_str()).unwrap_or("owner");

    let trust = parse_trust_level(trust_str)?;

    let address = TransportAddress::tailscale(address_str, name.map(String::from));
    let node_id = address_str.to_string();

    let node = GridNode {
        node_id: node_id.clone(),
        node_name: name.map(String::from),
        addresses: vec![address],
        capabilities: vec![],
        trust_level: trust,
        last_seen: frame::now_millis(),
        latency_ms: None,
    };

    state.registry.register_node(node);
    let _ = state.registry.save_to_disk();

    Ok(CommandResult::Json(json!({
        "paired": true,
        "nodeId": node_id,
        "trustLevel": trust_str,
    })))
}

/// grid/trust — update a node's trust level.
pub async fn handle_trust(state: &Arc<GridState>, params: Value) -> Result<CommandResult, String> {
    let node_id = params.get("nodeId").and_then(|v| v.as_str())
        .ok_or("nodeId parameter required")?;
    let trust_str = params.get("trust").and_then(|v| v.as_str())
        .ok_or("trust parameter required")?;

    let trust = parse_trust_level(trust_str)?;
    state.registry.set_trust(node_id, trust)?;
    let _ = state.registry.save_to_disk();

    Ok(CommandResult::Json(json!({
        "nodeId": node_id,
        "trustLevel": trust_str,
    })))
}

/// grid/audit — view remote command audit trail.
pub async fn handle_audit(state: &Arc<GridState>, params: Value) -> Result<CommandResult, String> {
    let limit = params.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;
    let entries = state.audit.recent(limit).await?;
    CommandResult::json(&entries)
}

/// grid/route — dry-run routing check.
pub async fn handle_route(state: &Arc<GridState>, params: Value) -> Result<CommandResult, String> {
    let command = params.get("targetCommand").and_then(|v| v.as_str())
        .or_else(|| params.get("command").and_then(|v| v.as_str()))
        .ok_or("command or targetCommand parameter required")?;

    let decision = state.router.route(command, &params, &state.registry);

    match decision {
        RouteDecision::Local => {
            Ok(CommandResult::Json(json!({
                "route": "local",
                "reason": "default or local capability",
            })))
        }
        RouteDecision::Remote { node, reason } => {
            Ok(CommandResult::Json(json!({
                "route": "remote",
                "nodeId": node.node_id,
                "nodeName": node.node_name,
                "reason": reason,
            })))
        }
    }
}
