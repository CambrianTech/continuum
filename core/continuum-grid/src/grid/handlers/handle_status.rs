use super::*;

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

    let transport = address.carrier(&state.transports)
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
        format!("ping-{}", uuid::Uuid::new_v4().simple()),
        our_address,
        node_id.to_string(),
        super::super::commands::PING.to_string(),
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

    let transport = address.carrier(&state.transports)
        .ok_or_else(|| format!("No transport for {}", address.display_address()))?;

    let our_address = transport
        .local_address()
        .map(|a| a.display_address())
        .unwrap_or_else(|| "unknown".into());

    let corr_id = format!("cmd-{}", uuid::Uuid::new_v4().simple());
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
            timestamp: EpochMs::now().millis(),
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

    let trust = trust_str.parse::<TrustLevel>()?;

    // Parse optional capabilities: gpu name and vram
    let gpu = params.get("gpu").and_then(|v| v.as_str()).map(String::from);
    let vram_mb = params.get("vramMb").and_then(|v| v.as_u64());

    let mut capabilities = Vec::new();
    if gpu.is_some() || vram_mb.is_some() {
        capabilities.push(super::super::node::NodeCapability::Compute { gpu, vram_mb });
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
        last_seen: EpochMs::now().millis(),
        latency_ms: None,
        peer_id,
        build_sha: None,
        build_number: 0,
        served_model: None,
        lanes: 0,
        residents: 0,
        resident_alarms: Vec::new(),
        silent_secs: 0,
        stale: false,
        behind: false,
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

    let trust = trust_str.parse::<TrustLevel>()?;
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
                    "remoteCommand": super::super::commands::NODE_STATUS,
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
        "timestamp": continuum_base::time::EpochMs::now().rfc3339(),
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
                    "remoteCommand": super::super::commands::JOB_SUBMIT,
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
        continuum_base::time::EpochMs::now().millis(),
        rand::random::<u32>() & 0xFFFFFF
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
        "startedAt": continuum_base::time::EpochMs::now().rfc3339(),
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
        "estimatedStart": continuum_base::time::EpochMs::now().rfc3339(),
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
                    "remoteCommand": super::super::commands::JOB_CONTROL,
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

    // A job with no recorded pid has no process to signal. Never signal pid 0: kill(0, sig)
    // reaches every process in the CALLER's group — pausing such a job would stop the core.
    if pid <= 0 && matches!(action, "pause" | "resume") {
        return Err(format!("Job '{job_id}' has no recorded process to {action}"));
    }
    let new_state = match action {
        "pause" => {
            job_signal(pid, JobSignal::Stop)?;
            "paused"
        }
        "resume" => {
            job_signal(pid, JobSignal::Continue)?;
            "running"
        }
        "cancel" => {
            if pid > 0 {
                job_signal(pid, JobSignal::Terminate)?;
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
                    "remoteCommand": super::super::commands::JOB_QUEUE,
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
pub(super) fn is_local_node(state: &Arc<GridState>, node_id: &str) -> bool {
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
