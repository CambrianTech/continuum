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
    let local_caps = state.local_capabilities.read().await.clone();

    Ok(CommandResult::Json(json!({
        "transports": transport_status,
        "totalNodes": nodes.len(),
        "onlineNodes": online.len(),
        "gridDir": state.grid_dir.to_string_lossy(),
        "localCapabilities": local_caps,
    })))
}

/// grid/nodes — list known nodes.
pub async fn handle_nodes(state: &Arc<GridState>) -> Result<CommandResult, String> {
    let nodes = state.registry.all_nodes();
    CommandResult::json(&nodes)
}

/// grid/ping — round-trip latency to a remote node.
pub async fn handle_ping(state: &Arc<GridState>, params: Value) -> Result<CommandResult, String> {
    let node_id = params
        .get("nodeId")
        .and_then(|v| v.as_str())
        .ok_or("nodeId parameter required")?;

    let node = state
        .registry
        .get(node_id)
        .ok_or_else(|| format!("Unknown node: {node_id}"))?;

    let address = node
        .addresses
        .first()
        .ok_or_else(|| format!("Node {node_id} has no addresses"))?;

    let transport = find_transport_for_address(&state.transports, address)
        .ok_or_else(|| format!("No transport available for {}", address.display_address()))?;

    let start = std::time::Instant::now();

    let conn = transport
        .connect(address)
        .await
        .map_err(|e| format!("Connect failed: {e}"))?;

    let our_address = transport
        .local_address()
        .map(|a| a.display_address())
        .unwrap_or_else(|| "unknown".into());

    let ping_frame = GridFrame::command_request(
        format!("ping-{}", correlation_id()),
        our_address,
        node_id.to_string(),
        super::commands::PING.to_string(),
        json!({}),
    );

    conn.send_frame(&ping_frame)
        .await
        .map_err(|e| format!("Send failed: {e}"))?;

    let response = tokio::time::timeout(Duration::from_secs(10), conn.recv_frame())
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
/// grid/send — dispatch a command to a specific node by id.
///
/// Thin wrapper around the lower-level [`dispatch_to_node`] primitive:
/// parses params, looks up the node, then delegates. The send-frame
/// dance + audit + result mapping lives in `dispatch_to_node` so the
/// new `GridInterceptor` (runtime/grid_interceptor.rs) can reuse it
/// for capability-based routing without re-parsing param shapes.
pub async fn handle_send(state: &Arc<GridState>, params: Value) -> Result<CommandResult, String> {
    let node_id = params
        .get("nodeId")
        .and_then(|v| v.as_str())
        .ok_or("nodeId parameter required")?;
    // TS mixin sends as 'remoteCommand' to avoid collision with IPC 'command' field.
    // Also accept 'command' for direct Rust callers.
    let remote_command = params
        .get("remoteCommand")
        .and_then(|v| v.as_str())
        .or_else(|| params.get("command").and_then(|v| v.as_str()))
        .ok_or("command or remoteCommand parameter required")?;
    let remote_params = params.get("params").cloned().unwrap_or(json!({}));

    let node = state
        .registry
        .get(node_id)
        .ok_or_else(|| format!("Unknown node: {node_id}"))?;

    dispatch_to_node(state, &node, remote_command, remote_params).await
}

/// Dispatch a command to a specific (already-resolved) [`GridNode`].
///
/// This is the core send-frame primitive — open a transport connection,
/// send a CommandRequest frame, await the matching CommandResult frame,
/// audit the round-trip, return the result.
///
/// Pulled out of [`handle_send`] in this PR so the new `GridInterceptor`
/// (runtime/grid_interceptor.rs) can reuse the same dispatch path when
/// the [`super::router::GridRouter`] decides a command should hop to a
/// remote node. Both callers — the explicit `grid/send` command and the
/// implicit capability-based interceptor — go through this function, so
/// there is exactly one place that knows how to send a Continuum command
/// over the grid wire.
pub async fn dispatch_to_node(
    state: &Arc<GridState>,
    node: &GridNode,
    remote_command: &str,
    remote_params: Value,
) -> Result<CommandResult, String> {
    let address = node
        .addresses
        .first()
        .ok_or_else(|| format!("Node {} has no addresses", node.node_id))?;

    let transport = find_transport_for_address(&state.transports, address)
        .ok_or_else(|| format!("No transport for {}", address.display_address()))?;

    let our_address = transport
        .local_address()
        .map(|a| a.display_address())
        .unwrap_or_else(|| "unknown".into());

    let corr_id = format!("cmd-{}", correlation_id());
    let frame = GridFrame::command_request(
        corr_id.clone(),
        our_address,
        node.node_id.clone(),
        remote_command.to_string(),
        remote_params,
    );

    let start = std::time::Instant::now();

    let conn = transport
        .connect(address)
        .await
        .map_err(|e| format!("Connect to {} failed: {e}", node.node_id))?;

    conn.send_frame(&frame)
        .await
        .map_err(|e| format!("Send to {} failed: {e}", node.node_id))?;

    // 5 minute timeout for long operations (training, etc.)
    let response = tokio::time::timeout(Duration::from_secs(300), conn.recv_frame())
        .await
        .map_err(|_| {
            format!(
                "Command '{remote_command}' on {} timed out (300s)",
                node.node_id
            )
        })?
        .map_err(|e| format!("Recv from {} failed: {e}", node.node_id))?;

    let duration_ms = start.elapsed().as_millis() as u64;
    let _ = conn.close().await;

    // Audit
    let outcome = match &response.payload {
        GridPayload::CommandResult { success: true, .. } => AuditOutcome::Success,
        _ => AuditOutcome::Error,
    };

    let _ = state
        .audit
        .log(&AuditEntry {
            timestamp: frame::now_millis(),
            direction: AuditDirection::Outbound,
            remote_node: node.node_id.clone(),
            command: remote_command.to_string(),
            correlation_id: corr_id,
            outcome,
            duration_ms,
        })
        .await;

    match response.payload {
        GridPayload::CommandResult {
            success: true,
            result,
            ..
        } => Ok(CommandResult::Json(
            result.unwrap_or(json!({"success": true})),
        )),
        GridPayload::CommandResult {
            success: false,
            error,
            ..
        } => Err(format!(
            "Remote command failed: {}",
            error.unwrap_or_default()
        )),
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

/// grid/pair — register a new node with trust level and optional capabilities.
pub async fn handle_pair(state: &Arc<GridState>, params: Value) -> Result<CommandResult, String> {
    let address_str = params
        .get("address")
        .and_then(|v| v.as_str())
        .ok_or("address parameter required")?;
    let name = params.get("name").and_then(|v| v.as_str());
    let trust_str = params
        .get("trust")
        .and_then(|v| v.as_str())
        .unwrap_or("owner");

    let trust = parse_trust_level(trust_str)?;

    // Parse optional capabilities: gpu name and vram
    let gpu = params.get("gpu").and_then(|v| v.as_str()).map(String::from);
    let vram_mb = params.get("vramMb").and_then(|v| v.as_u64());

    let mut capabilities = Vec::new();
    if gpu.is_some() || vram_mb.is_some() {
        capabilities.push(super::node::NodeCapability::Compute { gpu, vram_mb });
    }

    let address = TransportAddress::tailscale(address_str, name.map(String::from));
    let node_id = address_str.to_string();

    // The durable airc identity, if the caller knows it (#2228 — a NODE is an airc peer,
    // the sibling of persona_id == peer_id). Optional: manual pairing by address alone
    // won't carry it, and the gossip correlation supplies it later via set_peer_id.
    let peer_id = params
        .get("peerId")
        .and_then(|v| v.as_str())
        .and_then(|s| uuid::Uuid::parse_str(s).ok())
        .map(crate::identity::PeerId::from_uuid);

    let node = GridNode {
        node_id: node_id.clone(),
        node_name: name.map(String::from),
        addresses: vec![address],
        capabilities: capabilities.clone(),
        trust_level: trust,
        last_seen: frame::now_millis(),
        latency_ms: None,
        peer_id,
    };

    state.registry.register_node(node);
    let _ = state.registry.save_to_disk();

    Ok(CommandResult::Json(json!({
        "paired": true,
        "nodeId": node_id,
        "peerId": peer_id.map(|p| p.to_string()),
        "trustLevel": trust_str,
        "capabilities": capabilities,
    })))
}

/// grid/trust — update a node's trust level.
pub async fn handle_trust(state: &Arc<GridState>, params: Value) -> Result<CommandResult, String> {
    let node_id = params
        .get("nodeId")
        .and_then(|v| v.as_str())
        .ok_or("nodeId parameter required")?;
    let trust_str = params
        .get("trust")
        .and_then(|v| v.as_str())
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

/// grid/node-status — query local GPU, running jobs, queue depth.
/// When called remotely via grid/send, this executes on the TARGET node.
pub async fn handle_node_status(
    state: &Arc<GridState>,
    params: Value,
) -> Result<CommandResult, String> {
    let node_id = params.get("nodeId").and_then(|v| v.as_str());

    // If nodeId targets a remote node, delegate via handle_send
    if let Some(nid) = node_id {
        if !nid.is_empty() {
            if !is_local_node(state, nid) {
                let send_params = json!({
                    "nodeId": nid,
                    "remoteCommand": super::commands::NODE_STATUS,
                    "params": {}
                });
                return handle_send(state, send_params).await;
            }
        }
    }

    // Local: query this machine
    let gpu = query_gpu_info();
    let jobs = query_forge_processes();
    let queue = query_job_queue(&state.grid_dir);
    let hostname = gethostname();

    let has_running = !jobs.is_empty();
    let node_state = if has_running { "busy" } else { "ready" };

    Ok(CommandResult::Json(json!({
        "success": true,
        "state": node_state,
        "gpu": gpu,
        "jobs": jobs,
        "queue": queue,
        "nodeId": hostname,
        "timestamp": chrono_now_iso(),
    })))
}

/// grid/job-submit — write alloy to disk, start forge pipeline.
/// If nodeId targets a remote node, delegates via grid/send.
pub async fn handle_job_submit(
    state: &Arc<GridState>,
    params: Value,
) -> Result<CommandResult, String> {
    // Remote delegation — route to target node if specified and not local
    if let Some(nid) = params.get("nodeId").and_then(|v| v.as_str()) {
        if !nid.is_empty() {
            if !is_local_node(state, nid) {
                let send_params = json!({
                    "nodeId": nid,
                    "remoteCommand": super::commands::JOB_SUBMIT,
                    "params": {
                        "alloy": params.get("alloy"),
                        "priority": params.get("priority"),
                    }
                });
                return handle_send(state, send_params).await;
            }
        }
    }

    let alloy = params.get("alloy").ok_or("alloy parameter required")?;
    let priority = params.get("priority").and_then(|v| v.as_u64()).unwrap_or(5);

    let jobs_dir = state.grid_dir.join("jobs");
    let running_dir = jobs_dir.join("running");
    std::fs::create_dir_all(&running_dir).map_err(|e| format!("Failed to create jobs dir: {e}"))?;

    let job_id = format!(
        "job-{}-{:06x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
        rand_u32() & 0xFFFFFF
    );

    let alloy_path = running_dir.join(format!("{job_id}.alloy.json"));
    let log_path = running_dir.join(format!("{job_id}.log"));
    let meta_path = running_dir.join(format!("{job_id}.meta.json"));

    // Write alloy
    std::fs::write(
        &alloy_path,
        serde_json::to_string_pretty(alloy).unwrap_or_default(),
    )
    .map_err(|e| format!("Failed to write alloy: {e}"))?;

    // Find alloy_executor.py
    // The forge executor is the core's ONE remaining runtime Python dependency, and it
    // lives in a SIBLING repo (`../sentinel-ai/scripts/alloy_executor.py`) that a fresh
    // clone of this repo does not have. Not finding it used to return pid 0 and write
    // `state: "queued"` — indistinguishable from a job legitimately waiting its turn, so
    // `forge/start` reported SUCCESS for work that would never run, on every machine that
    // had only cloned continuum. Fail loud and name what is missing.
    // Tracked for excision to Rust by #52 / #99.
    let exec_path = find_alloy_executor().ok_or_else(|| {
        "forge/start cannot run: the alloy executor was not found. It is a Python script in \
         the sibling sentinel-ai repo, which this clone does not have. Set ALLOY_EXECUTOR to \
         its path, or clone sentinel-ai next to this repo. (The job was NOT queued — nothing \
         would have run it.)"
            .to_string()
    })?;

    let pid = {
        // Start forge pipeline
        let log_file =
            std::fs::File::create(&log_path).map_err(|e| format!("Failed to create log: {e}"))?;
        let log_err = log_file
            .try_clone()
            .map_err(|e| format!("Failed to clone log fd: {e}"))?;

        let child = std::process::Command::new("python3")
            .arg(&exec_path)
            .arg(&alloy_path)
            .arg("--output-dir")
            .arg(running_dir.join(&job_id))
            .current_dir(
                exec_path
                    .parent()
                    .and_then(|p| p.parent())
                    .unwrap_or(std::path::Path::new(".")),
            )
            .stdout(log_file)
            .stderr(log_err)
            .spawn()
            .map_err(|e| format!("Failed to start forge: {e}"))?;
        child.id()
    };

    let alloy_name = alloy
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or(&job_id);

    // Write meta
    let meta = json!({
        "jobId": job_id,
        "pid": pid,
        "alloyPath": alloy_path.to_string_lossy(),
        "logPath": log_path.to_string_lossy(),
        "state": if pid > 0 { "running" } else { "queued" },
        "priority": priority,
        "startedAt": chrono_now_iso(),
        "alloyName": alloy_name,
    });
    std::fs::write(
        &meta_path,
        serde_json::to_string_pretty(&meta).unwrap_or_default(),
    )
    .map_err(|e| format!("Failed to write meta: {e}"))?;

    Ok(CommandResult::Json(json!({
        "success": true,
        "jobId": job_id,
        "position": 0,
        "nodeId": gethostname(),
        "estimatedStart": chrono_now_iso(),
    })))
}

/// grid/job-control — pause/resume/cancel a running job.
/// If nodeId targets a remote node, delegates via grid/send.
pub async fn handle_job_control(
    state: &Arc<GridState>,
    params: Value,
) -> Result<CommandResult, String> {
    // Remote delegation
    if let Some(nid) = params.get("nodeId").and_then(|v| v.as_str()) {
        if !nid.is_empty() {
            if !is_local_node(state, nid) {
                let send_params = json!({
                    "nodeId": nid,
                    "remoteCommand": super::commands::JOB_CONTROL,
                    "params": {
                        "jobId": params.get("jobId"),
                        "action": params.get("action"),
                    }
                });
                return handle_send(state, send_params).await;
            }
        }
    }

    let job_id = params
        .get("jobId")
        .and_then(|v| v.as_str())
        .ok_or("jobId parameter required")?;
    let action = params
        .get("action")
        .and_then(|v| v.as_str())
        .ok_or("action parameter required")?;

    let jobs_dir = state.grid_dir.join("jobs");
    let meta =
        find_job_meta(&jobs_dir, job_id).ok_or_else(|| format!("Job '{job_id}' not found"))?;

    let pid = meta.get("pid").and_then(|v| v.as_u64()).unwrap_or(0) as i32;
    let previous_state = meta
        .get("state")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let new_state = match action {
        "pause" => {
            #[cfg(unix)]
            unsafe {
                libc::kill(pid, libc::SIGSTOP);
            }
            "paused"
        }
        "resume" => {
            #[cfg(unix)]
            unsafe {
                libc::kill(pid, libc::SIGCONT);
            }
            "running"
        }
        "cancel" => {
            #[cfg(unix)]
            unsafe {
                libc::kill(pid, libc::SIGTERM);
            }
            // Move to failed
            let _ = move_job_files(&jobs_dir, job_id, "running", "failed");
            "cancelled"
        }
        _ => {
            return Err(format!(
                "Invalid action: {action}. Must be pause, resume, or cancel."
            ))
        }
    };

    // Update meta
    let mut updated = meta.clone();
    updated["state"] = json!(new_state);
    let state_dir = if new_state == "cancelled" {
        "failed"
    } else {
        new_state
    };
    let meta_path = jobs_dir.join(state_dir).join(format!("{job_id}.meta.json"));
    let _ = std::fs::create_dir_all(meta_path.parent().unwrap_or(std::path::Path::new(".")));
    let _ = std::fs::write(
        &meta_path,
        serde_json::to_string_pretty(&updated).unwrap_or_default(),
    );

    Ok(CommandResult::Json(json!({
        "success": true,
        "jobId": job_id,
        "previousState": previous_state,
        "newState": new_state,
        "checkpoint": meta.get("checkpoint").cloned().unwrap_or(json!({})),
    })))
}

/// grid/job-queue — list jobs from filesystem.
/// If nodeId targets a remote node, delegates via grid/send.
pub async fn handle_job_queue(
    state: &Arc<GridState>,
    params: Value,
) -> Result<CommandResult, String> {
    // Remote delegation
    if let Some(nid) = params.get("nodeId").and_then(|v| v.as_str()) {
        if !nid.is_empty() {
            if !is_local_node(state, nid) {
                let send_params = json!({
                    "nodeId": nid,
                    "remoteCommand": super::commands::JOB_QUEUE,
                    "params": {
                        "state": params.get("state"),
                        "limit": params.get("limit"),
                    }
                });
                return handle_send(state, send_params).await;
            }
        }
    }

    let state_filter = params
        .get("state")
        .and_then(|v| v.as_str())
        .unwrap_or("all");
    let limit = params.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as usize;

    let jobs_dir = state.grid_dir.join("jobs");
    let hostname = gethostname();
    let dirs = ["queued", "running", "paused", "completed", "failed"];

    let mut summary =
        json!({ "queued": 0, "running": 0, "paused": 0, "completed": 0, "failed": 0 });
    let mut jobs = Vec::new();

    for dir_name in &dirs {
        let dir_path = jobs_dir.join(dir_name);
        let metas = list_meta_files(&dir_path);
        summary[*dir_name] = json!(metas.len());

        if state_filter != "all" && state_filter != *dir_name {
            continue;
        }
        if jobs.len() >= limit {
            continue;
        }

        for meta_path in metas {
            if jobs.len() >= limit {
                break;
            }
            if let Ok(content) = std::fs::read_to_string(&meta_path) {
                if let Ok(meta) = serde_json::from_str::<Value>(&content) {
                    let pid = meta.get("pid").and_then(|v| v.as_u64()).unwrap_or(0) as i32;
                    let is_alive = pid > 0 && is_process_alive(pid);
                    let effective_state = if *dir_name == "running" && !is_alive {
                        "completed"
                    } else {
                        *dir_name
                    };

                    jobs.push(json!({
                        "jobId": meta.get("jobId").and_then(|v| v.as_str()).unwrap_or(""),
                        "alloyName": meta.get("alloyName").and_then(|v| v.as_str()).unwrap_or("unknown"),
                        "state": effective_state,
                        "progress": meta.get("progress").cloned().unwrap_or(json!({
                            "cycle": 0, "totalCycles": 0, "step": 0, "totalSteps": 0
                        })),
                        "startedAt": meta.get("startedAt").and_then(|v| v.as_str()).unwrap_or(""),
                        "estimatedCompletion": meta.get("estimatedCompletion").and_then(|v| v.as_str()).unwrap_or(""),
                        "nodeId": hostname,
                    }));
                }
            }
        }
    }

    Ok(CommandResult::Json(json!({
        "success": true,
        "jobs": jobs,
        "summary": summary,
    })))
}

// ── Remote delegation helper ────────────────────────────────────────────

/// Check if a nodeId refers to the local machine.
fn is_local_node(state: &Arc<GridState>, node_id: &str) -> bool {
    if node_id.is_empty() || node_id == "local" {
        return true;
    }
    let hostname = gethostname();
    if hostname.contains(node_id) || node_id.contains(&hostname) {
        return true;
    }
    state.transports.iter().any(|t| {
        t.local_address()
            .map(|a| a.display_address().contains(node_id))
            .unwrap_or(false)
    })
}

// ── Helper functions for job management ─────────────────────────────────

/// `system_profiler`'s VRAM string ("1536 MB", "8 GB") in MEGABYTES.
///
/// READS the unit rather than assuming it. The previous code took the leading
/// number and multiplied by 1024 unconditionally — correct for Apple Silicon,
/// which reports GB, and wrong by exactly 1024x on an Intel Mac, which reports
/// MB. Measured 2026-09-05: `spdisplays_vram_shared` = "1536 MB" on an Intel
/// UHD 630 became `memoryTotalMb: 1572864`, so the weakest node on the grid
/// advertised 1.5 TB of VRAM.
///
/// Direction of the error is why this is worth a named function: a capability
/// report that OVERSTATES makes a node claim work it cannot do, and the failure
/// lands far from the cause — a lane dying on a box that "had plenty of memory".
/// Understating only costs throughput. So an unrecognised unit returns `None`
/// (surfacing as 0 / unknown) rather than guessing a multiplier
/// (`[[no-fallbacks-ever]]`): silence is honest, a guess is a confident lie.
///
/// Deliberately NOT `#[cfg(target_os = "macos")]` even though only the macOS
/// arm calls it: the parsing is platform-independent, so leaving it compiled
/// everywhere lets the Linux and Windows CI runners execute its tests too. A
/// macOS-only unit would be verified by exactly one runner.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn vram_string_to_mb(s: &str) -> Option<u32> {
    let mut parts = s.split_whitespace();
    let value: u32 = parts.next()?.parse().ok()?;
    match parts.next()?.to_ascii_uppercase().as_str() {
        u if u.starts_with("GB") => value.checked_mul(1024),
        u if u.starts_with("MB") => Some(value),
        _ => None,
    }
}

fn query_gpu_info() -> Value {
    // NVIDIA: Try WSL2 path first, then standard
    let nvidia_smi = if std::path::Path::new("/usr/lib/wsl/lib/nvidia-smi").exists() {
        Some("/usr/lib/wsl/lib/nvidia-smi")
    } else {
        // Check if nvidia-smi exists in PATH
        std::process::Command::new("which")
            .arg("nvidia-smi")
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|_| "nvidia-smi")
    };

    if let Some(smi) = nvidia_smi {
        let output = std::process::Command::new(smi)
            .args([
                "--query-gpu=name,utilization.gpu,memory.used,memory.total,temperature.gpu",
                "--format=csv,noheader,nounits",
            ])
            .output();

        if let Ok(o) = output {
            if o.status.success() {
                let s = String::from_utf8_lossy(&o.stdout);
                let parts: Vec<&str> = s.trim().split(',').map(|p| p.trim()).collect();
                return json!({
                    "name": parts.first().unwrap_or(&""),
                    "utilization": parts.get(1).and_then(|v| v.parse::<u32>().ok()).unwrap_or(0),
                    "memoryUsedMb": parts.get(2).and_then(|v| v.parse::<u32>().ok()).unwrap_or(0),
                    "memoryTotalMb": parts.get(3).and_then(|v| v.parse::<u32>().ok()).unwrap_or(0),
                    "temperatureC": parts.get(4).and_then(|v| v.parse::<u32>().ok()).unwrap_or(0),
                });
            }
        }
    }

    // Apple Silicon: check for Metal GPU via system_profiler
    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("system_profiler")
            .args(["SPDisplaysDataType", "-json"])
            .output();
        if let Ok(o) = output {
            if o.status.success() {
                if let Ok(data) = serde_json::from_slice::<Value>(&o.stdout) {
                    if let Some(gpu) = data
                        .get("SPDisplaysDataType")
                        .and_then(|v| v.as_array())
                        .and_then(|a| a.first())
                    {
                        let name = gpu
                            .get("sppci_model")
                            .and_then(|v| v.as_str())
                            .unwrap_or("Apple GPU");
                        let vram = gpu
                            .get("spdisplays_vram_shared")
                            .or_else(|| gpu.get("spdisplays_vram"))
                            .and_then(|v| v.as_str())
                            .and_then(vram_string_to_mb)
                            .unwrap_or(0);
                        return json!({
                            "name": name,
                            "utilization": 0,
                            "memoryUsedMb": 0,
                            "memoryTotalMb": vram,
                            "temperatureC": 0,
                            "type": "metal",
                        });
                    }
                }
            }
        }
    }

    json!({
        "name": "No GPU detected",
        "utilization": 0,
        "memoryUsedMb": 0,
        "memoryTotalMb": 0,
        "temperatureC": 0,
        "type": "none",
    })
}

fn query_forge_processes() -> Vec<Value> {
    let output = std::process::Command::new("sh")
        .args(["-c", "ps aux | grep -E '(forge_pipeline|forge_model|alloy_executor|train|fine.?tun)' | grep -v grep"])
        .output();

    match output {
        Ok(o) => {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .filter(|l| !l.is_empty())
                .filter_map(|line| {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    let pid = parts.get(1)?.parse::<u32>().ok()?;
                    let cpu = parts.get(2).unwrap_or(&"0").to_string();
                    let mem = parts.get(3).unwrap_or(&"0").to_string();
                    let cmd: String = parts.get(10..).map(|p| p.join(" ")).unwrap_or_default();

                    let job_type = if cmd.contains("forge_pipeline") || cmd.contains("alloy_executor") { "forge" }
                        else if cmd.contains("train") || cmd.contains("fine") { "training" }
                        else { "unknown" };

                    Some(json!({ "pid": pid, "type": job_type, "detail": crate::utils::str_truncate::truncate_at_char_boundary(&cmd, 120), "cpu": cpu, "mem": mem }))
                })
                .collect()
        }
        _ => vec![],
    }
}

fn query_job_queue(grid_dir: &std::path::Path) -> Vec<Value> {
    let queue_dir = grid_dir.join("jobs/queued");
    list_meta_files(&queue_dir)
        .iter()
        .filter_map(|path| {
            let name = path.file_stem()?.to_string_lossy().replace(".meta", "");
            Some(json!({ "name": name, "path": path.to_string_lossy() }))
        })
        .collect()
}

fn gethostname() -> String {
    std::process::Command::new("hostname")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string())
}

fn chrono_now_iso() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    let millis = now.subsec_millis();
    // Manual ISO 8601 without chrono dependency
    let days_since_epoch = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;
    // Compute year/month/day from days since epoch (1970-01-01)
    let (year, month, day) = days_to_ymd(days_since_epoch);
    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}.{millis:03}Z")
}

fn days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    // Algorithm from https://howardhinnant.github.io/date_algorithms.html
    days += 719468;
    let era = days / 146097;
    let doe = days - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

fn find_alloy_executor() -> Option<std::path::PathBuf> {
    // 1. Explicit env var override (highest priority)
    if let Ok(path) = std::env::var("ALLOY_EXECUTOR") {
        let p = std::path::PathBuf::from(&path);
        if p.exists() {
            return Some(p);
        }
        eprintln!("[grid] ALLOY_EXECUTOR={path} does not exist");
    }

    // 2. Search relative to this binary (sibling sentinel-ai repo)
    if let Ok(exe) = std::env::current_exe() {
        if let Some(base) = exe
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.parent())
        {
            let candidate = base.join("sentinel-ai/scripts/alloy_executor.py");
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }

    // 3. Search common locations relative to HOME
    let home = std::env::var("HOME").unwrap_or_default();
    let candidates = [
        "sentinel-ai/scripts/alloy_executor.py",
        "Development/cambrian/sentinel-ai/scripts/alloy_executor.py",
        "cambrian/sentinel-ai/scripts/alloy_executor.py",
        ".continuum/sentinel-ai/scripts/alloy_executor.py",
    ];
    for rel in &candidates {
        let p = std::path::PathBuf::from(&home).join(rel);
        if p.exists() {
            return Some(p);
        }
    }

    // 4. Search PATH for alloy_executor.py
    if let Ok(output) = std::process::Command::new("which")
        .arg("alloy_executor.py")
        .output()
    {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let p = std::path::PathBuf::from(&path);
            if p.exists() {
                return Some(p);
            }
        }
    }

    eprintln!(
        "[grid] alloy_executor.py not found. Set ALLOY_EXECUTOR env var or install sentinel-ai."
    );
    None
}

fn find_job_meta(jobs_dir: &std::path::Path, job_id: &str) -> Option<Value> {
    for sub in ["running", "queued", "paused"] {
        let path = jobs_dir.join(sub).join(format!("{job_id}.meta.json"));
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(meta) = serde_json::from_str::<Value>(&content) {
                return Some(meta);
            }
        }
    }
    None
}

fn move_job_files(
    jobs_dir: &std::path::Path,
    job_id: &str,
    from: &str,
    to: &str,
) -> std::io::Result<()> {
    let from_dir = jobs_dir.join(from);
    let to_dir = jobs_dir.join(to);
    std::fs::create_dir_all(&to_dir)?;
    if let Ok(entries) = std::fs::read_dir(&from_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            if name.to_string_lossy().starts_with(job_id) {
                std::fs::rename(entry.path(), to_dir.join(&name))?;
            }
        }
    }
    Ok(())
}

fn list_meta_files(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    std::fs::read_dir(dir)
        .ok()
        .map(|entries| {
            let mut files: Vec<_> = entries
                .flatten()
                .filter(|e| e.file_name().to_string_lossy().ends_with(".meta.json"))
                .map(|e| e.path())
                .collect();
            files.sort();
            files.reverse();
            files
        })
        .unwrap_or_default()
}

fn is_process_alive(pid: i32) -> bool {
    #[cfg(unix)]
    {
        unsafe { libc::kill(pid, 0) == 0 }
    }
    #[cfg(not(unix))]
    {
        false
    }
}

fn rand_u32() -> u32 {
    // Simple random without pulling in rand crate
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos()
}

/// grid/setup-check — comprehensive grid setup diagnostics.
///
/// Checks Tailscale installation, connectivity, HTTPS certs, peer discovery,
/// and returns actionable fix steps for any issues found.
pub async fn handle_setup_check(state: &Arc<GridState>) -> Result<CommandResult, String> {
    let mut checks: Vec<Value> = Vec::new();
    let mut actions: Vec<String> = Vec::new();
    let mut ready = true;

    // ── Check 1: Tailscale CLI installed ──────────────────────
    let ts_installed = match tokio::process::Command::new("tailscale")
        .args(["version"])
        .output()
        .await
    {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
            checks.push(json!({
                "check": "tailscale_installed",
                "status": "pass",
                "detail": version,
            }));
            true
        }
        _ => {
            checks.push(json!({
                "check": "tailscale_installed",
                "status": "fail",
                "detail": "Tailscale CLI not found",
            }));
            actions.push("Install Tailscale: https://tailscale.com/download".into());
            ready = false;
            false
        }
    };

    if !ts_installed {
        return Ok(CommandResult::Json(json!({
            "ready": false,
            "checks": checks,
            "actions": actions,
            "summary": "Tailscale not installed. Install it first, then re-run setup-check.",
        })));
    }

    // ── Check 2: Tailscale connected to tailnet ──────────────
    let ts_status_output = tokio::process::Command::new("tailscale")
        .args(["status", "--json"])
        .output()
        .await;

    let (ts_connected, ts_self_ip, ts_dns_name, ts_peers) = match &ts_status_output {
        Ok(output) if output.status.success() => {
            match serde_json::from_slice::<Value>(&output.stdout) {
                Ok(status_json) => {
                    // Extract self IP
                    let self_ip = status_json
                        .get("TailscaleIPs")
                        .and_then(|v| v.as_array())
                        .and_then(|a| a.first())
                        .and_then(|v| v.as_str())
                        .map(String::from)
                        .or_else(|| {
                            status_json
                                .get("Self")
                                .and_then(|s| s.get("TailscaleIPs"))
                                .and_then(|v| v.as_array())
                                .and_then(|a| a.first())
                                .and_then(|v| v.as_str())
                                .map(String::from)
                        });

                    // Extract DNS name
                    let dns_name = status_json
                        .get("Self")
                        .and_then(|s| s.get("DNSName"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.trim_end_matches('.').to_string());

                    // Count online peers
                    let peers: Vec<Value> = status_json
                        .get("Peer")
                        .and_then(|p| p.as_object())
                        .map(|peers| {
                            peers
                                .values()
                                .filter(|p| {
                                    p.get("Online").and_then(|v| v.as_bool()).unwrap_or(false)
                                })
                                .cloned()
                                .collect()
                        })
                        .unwrap_or_default();

                    let connected = self_ip.is_some();
                    if connected {
                        checks.push(json!({
                            "check": "tailscale_connected",
                            "status": "pass",
                            "detail": format!("Connected as {}", self_ip.as_deref().unwrap_or("unknown")),
                        }));
                    } else {
                        checks.push(json!({
                            "check": "tailscale_connected",
                            "status": "fail",
                            "detail": "Tailscale running but no IP assigned",
                        }));
                        actions.push("Run: tailscale up".into());
                        ready = false;
                    }

                    (connected, self_ip, dns_name, peers)
                }
                Err(e) => {
                    checks.push(json!({
                        "check": "tailscale_connected",
                        "status": "fail",
                        "detail": format!("Failed to parse status: {e}"),
                    }));
                    actions.push("Check Tailscale status: tailscale status".into());
                    ready = false;
                    (false, None, None, vec![])
                }
            }
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            checks.push(json!({
                "check": "tailscale_connected",
                "status": "fail",
                "detail": if stderr.contains("not running") {
                    "Tailscale daemon not running"
                } else {
                    &stderr
                },
            }));
            actions.push("Start Tailscale: tailscale up".into());
            ready = false;
            (false, None, None, vec![])
        }
        Err(e) => {
            checks.push(json!({
                "check": "tailscale_connected",
                "status": "fail",
                "detail": format!("Failed to query status: {e}"),
            }));
            ready = false;
            (false, None, None, vec![])
        }
    };

    // ── Check 3: HTTPS certificates ──────────────────────────
    if ts_connected {
        let cert_check = tokio::process::Command::new("tailscale")
            .args(["cert", "--check", &ts_dns_name.clone().unwrap_or_default()])
            .output()
            .await;

        match cert_check {
            Ok(output) if output.status.success() => {
                checks.push(json!({
                    "check": "https_certificates",
                    "status": "pass",
                    "detail": format!("HTTPS enabled for {}", ts_dns_name.as_deref().unwrap_or("?")),
                }));
            }
            _ => {
                checks.push(json!({
                    "check": "https_certificates",
                    "status": "warn",
                    "detail": "HTTPS certificates not available (grid still works without them, but browsers need HTTPS for some features)",
                }));
                actions.push(
                    "Enable HTTPS: Tailscale admin → DNS → toggle 'HTTPS Certificates' ON".into(),
                );
                actions.push("URL: https://login.tailscale.com/admin/dns".into());
            }
        }
    }

    // ── Check 4: Peer discovery ──────────────────────────────
    if ts_connected {
        let peer_count = ts_peers.len();
        let peer_names: Vec<String> = ts_peers
            .iter()
            .filter_map(|p| p.get("HostName").and_then(|v| v.as_str()).map(String::from))
            .collect();

        if peer_count > 0 {
            checks.push(json!({
                "check": "peers_discovered",
                "status": "pass",
                "detail": format!("{} online peers: {}", peer_count, peer_names.join(", ")),
                "peers": peer_names,
            }));
        } else {
            checks.push(json!({
                "check": "peers_discovered",
                "status": "warn",
                "detail": "No online peers found. Other machines need Tailscale + Continuum installed.",
            }));
            actions.push("Install Continuum on another machine: git clone + ./setup.sh".into());
        }
    }

    // ── Check 5: Grid transport status ───────────────────────
    for t in &state.transports {
        let addr = t.local_address();
        if addr.is_some() {
            checks.push(json!({
                "check": format!("transport_{}", t.name()),
                "status": "pass",
                "detail": format!("{} listening at {}", t.name(), addr.unwrap().display_address()),
            }));
        } else {
            checks.push(json!({
                "check": format!("transport_{}", t.name()),
                "status": if t.name() == "tailscale" && !ts_connected { "skip" } else { "warn" },
                "detail": format!("{} not active", t.name()),
            }));
        }
    }

    // ── Check 6: Known nodes in registry ─────────────────────
    let known_nodes = state.registry.all_nodes();
    let online_nodes = state.registry.online_nodes();
    checks.push(json!({
        "check": "grid_registry",
        "status": if known_nodes.is_empty() && ts_connected { "warn" } else { "pass" },
        "detail": format!("{} known nodes ({} online)", known_nodes.len(), online_nodes.len()),
    }));

    if known_nodes.is_empty() && ts_connected {
        actions.push("Run grid/discover to find peers, or grid/pair to add a specific node".into());
    }

    // ── Check 7: Docker grid profile (.env) ──────────────────
    let env_path = std::path::Path::new(".env");
    let grid_profile_active = if env_path.exists() {
        match tokio::fs::read_to_string(env_path).await {
            Ok(contents) => {
                let has_profile = contents.contains("COMPOSE_PROFILES=grid")
                    || contents.contains("COMPOSE_PROFILES=\"grid\"");
                let has_auth_key = contents.contains("TS_AUTHKEY=tskey-auth-");
                let has_hostname = contents.contains("TS_HOSTNAME=");

                if has_profile && has_auth_key && has_hostname {
                    checks.push(json!({
                        "check": "docker_grid_profile",
                        "status": "pass",
                        "detail": "Grid profile configured in .env",
                    }));
                    true
                } else {
                    let mut missing = Vec::new();
                    if !has_profile {
                        missing.push("COMPOSE_PROFILES=grid");
                    }
                    if !has_auth_key {
                        missing.push("TS_AUTHKEY");
                    }
                    if !has_hostname {
                        missing.push("TS_HOSTNAME");
                    }
                    checks.push(json!({
                        "check": "docker_grid_profile",
                        "status": "warn",
                        "detail": format!("Missing in .env: {}", missing.join(", ")),
                    }));
                    if !has_auth_key {
                        actions.push(
                            "Generate auth key: https://login.tailscale.com/admin/settings/keys"
                                .into(),
                        );
                    }
                    false
                }
            }
            Err(_) => {
                checks.push(json!({
                    "check": "docker_grid_profile",
                    "status": "warn",
                    "detail": "Could not read .env",
                }));
                false
            }
        }
    } else {
        checks.push(json!({
            "check": "docker_grid_profile",
            "status": "info",
            "detail": "No .env file — running in local-only mode. Run setup.sh to enable grid.",
        }));
        false
    };

    // ── Build grid URLs ────────────────────────────────────────
    let grid_urls = if let Some(ref dns) = ts_dns_name {
        if grid_profile_active {
            Some(json!({
                "web": format!("https://{dns}"),
                "websocket": format!("wss://{dns}:9001"),
                "livekit": format!("wss://{dns}:7880"),
            }))
        } else if let Some(ref ip) = ts_self_ip {
            // No grid profile → plain HTTP via Tailscale IP
            Some(json!({
                "web": format!("http://{ip}:9003"),
                "note": "HTTP only — run 'continuum grid enable' for HTTPS",
            }))
        } else {
            None
        }
    } else {
        None
    };

    // ── Build summary ────────────────────────────────────────
    let summary = if ready && ts_connected {
        if ts_peers.is_empty() {
            "Tailscale connected. No peers yet — install Continuum on another machine.".to_string()
        } else {
            format!(
                "Grid ready! {} peers online. {}",
                ts_peers.len(),
                if grid_profile_active {
                    "Docker grid profile active."
                } else {
                    "Docker grid profile not configured (optional for peer-to-peer)."
                }
            )
        }
    } else if ts_installed && !ts_connected {
        "Tailscale installed but not connected. Run 'tailscale up' to join your tailnet."
            .to_string()
    } else {
        "Grid not ready. Follow the actions below to configure.".to_string()
    };

    Ok(CommandResult::Json(json!({
        "ready": ready && ts_connected,
        "tailscaleIp": ts_self_ip,
        "dnsName": ts_dns_name,
        "peerCount": ts_peers.len(),
        "gridUrls": grid_urls,
        "checks": checks,
        "actions": actions,
        "summary": summary,
    })))
}

/// grid/route — dry-run routing check.
pub async fn handle_route(state: &Arc<GridState>, params: Value) -> Result<CommandResult, String> {
    let command = params
        .get("targetCommand")
        .and_then(|v| v.as_str())
        .or_else(|| params.get("command").and_then(|v| v.as_str()))
        .ok_or("command or targetCommand parameter required")?;

    let decision = state.router.route(command, &params, &state.registry);

    match decision {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// what this catches: a capability report that OVERSTATES what this node has.
    ///
    /// The producer took `system_profiler`'s leading number and multiplied by 1024
    /// unconditionally — right for Apple Silicon (which reports GB), wrong by exactly
    /// 1024x on an Intel Mac (which reports MB). Measured 2026-09-05: "1536 MB" on an
    /// Intel UHD 630 was published as `memoryTotalMb: 1572864`, so the weakest node on
    /// the grid advertised 1.5 TB of VRAM — more than every other GPU combined, from a
    /// box that could not run a decode lane at all.
    ///
    /// Both units are asserted because fixing only the MB case by flipping the
    /// multiplier would silently break every Apple Silicon node instead.
    #[test]
    fn vram_units_are_read_not_assumed() {
        assert_eq!(
            vram_string_to_mb("1536 MB"),
            Some(1536),
            "the Intel Mac case: MB must stay MB, not become 1572864"
        );
        assert_eq!(
            vram_string_to_mb("8 GB"),
            Some(8192),
            "the Apple Silicon case: GB must still convert"
        );
    }

    /// what this catches: guessing a multiplier for a unit we do not recognise.
    ///
    /// `None` surfaces as 0 / unknown, which UNDERSTATES. That asymmetry is the whole
    /// point: understating costs throughput, overstating makes a node claim work it
    /// cannot do and the failure lands far from the cause, on a box that "had plenty
    /// of memory". Per `[[no-fallbacks-ever]]` — silence is honest, a guess is a
    /// confident lie.
    #[test]
    fn an_unknown_unit_reports_nothing_rather_than_guessing() {
        assert_eq!(vram_string_to_mb("1536"), None, "no unit is not an implied unit");
        assert_eq!(vram_string_to_mb("4 TB"), None, "an unhandled unit must not be assumed");
        assert_eq!(vram_string_to_mb(""), None);
        assert_eq!(vram_string_to_mb("lots MB"), None, "a non-numeric value is not a size");
    }
}
