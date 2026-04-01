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

/// grid/pair — register a new node with trust level and optional capabilities.
pub async fn handle_pair(state: &Arc<GridState>, params: Value) -> Result<CommandResult, String> {
    let address_str = params.get("address").and_then(|v| v.as_str())
        .ok_or("address parameter required")?;
    let name = params.get("name").and_then(|v| v.as_str());
    let trust_str = params.get("trust").and_then(|v| v.as_str()).unwrap_or("owner");

    let trust = parse_trust_level(trust_str)?;

    // Parse optional capabilities: gpu name and vram
    let gpu = params.get("gpu").and_then(|v| v.as_str()).map(String::from);
    let vram_mb = params.get("vramMb").and_then(|v| v.as_u64());

    let mut capabilities = Vec::new();
    if gpu.is_some() || vram_mb.is_some() {
        capabilities.push(super::node::NodeCapability::Compute {
            gpu,
            vram_mb,
        });
    }

    let address = TransportAddress::tailscale(address_str, name.map(String::from));
    let node_id = address_str.to_string();

    let node = GridNode {
        node_id: node_id.clone(),
        node_name: name.map(String::from),
        addresses: vec![address],
        capabilities: capabilities.clone(),
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
        "capabilities": capabilities,
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

/// grid/node-status — query local GPU, running jobs, queue depth.
/// When called remotely via grid/send, this executes on the TARGET node.
pub async fn handle_node_status(state: &Arc<GridState>, params: Value) -> Result<CommandResult, String> {
    let node_id = params.get("nodeId").and_then(|v| v.as_str());

    // If nodeId targets a remote node, delegate via handle_send
    if let Some(nid) = node_id {
        if !nid.is_empty() {
            // Check if this is us
            let is_local = state.transports.iter().any(|t| {
                t.local_address().map(|a| a.display_address().contains(nid)).unwrap_or(false)
            });
            if !is_local {
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
pub async fn handle_job_submit(state: &Arc<GridState>, params: Value) -> Result<CommandResult, String> {
    let alloy = params.get("alloy")
        .ok_or("alloy parameter required")?;
    let priority = params.get("priority").and_then(|v| v.as_u64()).unwrap_or(5);

    let jobs_dir = state.grid_dir.join("jobs");
    let running_dir = jobs_dir.join("running");
    std::fs::create_dir_all(&running_dir)
        .map_err(|e| format!("Failed to create jobs dir: {e}"))?;

    let job_id = format!("job-{}-{:06x}", std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis(),
        rand_u32() & 0xFFFFFF);

    let alloy_path = running_dir.join(format!("{job_id}.alloy.json"));
    let log_path = running_dir.join(format!("{job_id}.log"));
    let meta_path = running_dir.join(format!("{job_id}.meta.json"));

    // Write alloy
    std::fs::write(&alloy_path, serde_json::to_string_pretty(alloy).unwrap_or_default())
        .map_err(|e| format!("Failed to write alloy: {e}"))?;

    // Find alloy_executor.py
    let executor = find_alloy_executor();

    let pid = if let Some(exec_path) = executor {
        // Start forge pipeline
        let log_file = std::fs::File::create(&log_path)
            .map_err(|e| format!("Failed to create log: {e}"))?;
        let log_err = log_file.try_clone()
            .map_err(|e| format!("Failed to clone log fd: {e}"))?;

        let child = std::process::Command::new("python3")
            .arg(&exec_path)
            .arg(&alloy_path)
            .arg("--output-dir")
            .arg(running_dir.join(&job_id))
            .current_dir(exec_path.parent().and_then(|p| p.parent()).unwrap_or(std::path::Path::new(".")))
            .stdout(log_file)
            .stderr(log_err)
            .spawn()
            .map_err(|e| format!("Failed to start forge: {e}"))?;
        child.id()
    } else {
        0 // No executor found — job is queued but not started
    };

    let alloy_name = alloy.get("name").and_then(|v| v.as_str()).unwrap_or(&job_id);

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
    std::fs::write(&meta_path, serde_json::to_string_pretty(&meta).unwrap_or_default())
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
pub async fn handle_job_control(state: &Arc<GridState>, params: Value) -> Result<CommandResult, String> {
    let job_id = params.get("jobId").and_then(|v| v.as_str())
        .ok_or("jobId parameter required")?;
    let action = params.get("action").and_then(|v| v.as_str())
        .ok_or("action parameter required")?;

    let jobs_dir = state.grid_dir.join("jobs");
    let meta = find_job_meta(&jobs_dir, job_id)
        .ok_or_else(|| format!("Job '{job_id}' not found"))?;

    let pid = meta.get("pid").and_then(|v| v.as_u64()).unwrap_or(0) as i32;
    let previous_state = meta.get("state").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();

    let new_state = match action {
        "pause" => {
            #[cfg(unix)]
            unsafe { libc::kill(pid, libc::SIGSTOP); }
            "paused"
        }
        "resume" => {
            #[cfg(unix)]
            unsafe { libc::kill(pid, libc::SIGCONT); }
            "running"
        }
        "cancel" => {
            #[cfg(unix)]
            unsafe { libc::kill(pid, libc::SIGTERM); }
            // Move to failed
            let _ = move_job_files(&jobs_dir, job_id, "running", "failed");
            "cancelled"
        }
        _ => return Err(format!("Invalid action: {action}. Must be pause, resume, or cancel.")),
    };

    // Update meta
    let mut updated = meta.clone();
    updated["state"] = json!(new_state);
    let state_dir = if new_state == "cancelled" { "failed" } else { new_state };
    let meta_path = jobs_dir.join(state_dir).join(format!("{job_id}.meta.json"));
    let _ = std::fs::create_dir_all(meta_path.parent().unwrap_or(std::path::Path::new(".")));
    let _ = std::fs::write(&meta_path, serde_json::to_string_pretty(&updated).unwrap_or_default());

    Ok(CommandResult::Json(json!({
        "success": true,
        "jobId": job_id,
        "previousState": previous_state,
        "newState": new_state,
        "checkpoint": meta.get("checkpoint").cloned().unwrap_or(json!({})),
    })))
}

/// grid/job-queue — list jobs from filesystem.
pub async fn handle_job_queue(state: &Arc<GridState>, params: Value) -> Result<CommandResult, String> {
    let state_filter = params.get("state").and_then(|v| v.as_str()).unwrap_or("all");
    let limit = params.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as usize;

    let jobs_dir = state.grid_dir.join("jobs");
    let hostname = gethostname();
    let dirs = ["queued", "running", "paused", "completed", "failed"];

    let mut summary = json!({ "queued": 0, "running": 0, "paused": 0, "completed": 0, "failed": 0 });
    let mut jobs = Vec::new();

    for dir_name in &dirs {
        let dir_path = jobs_dir.join(dir_name);
        let metas = list_meta_files(&dir_path);
        summary[*dir_name] = json!(metas.len());

        if state_filter != "all" && state_filter != *dir_name { continue; }
        if jobs.len() >= limit { continue; }

        for meta_path in metas {
            if jobs.len() >= limit { break; }
            if let Ok(content) = std::fs::read_to_string(&meta_path) {
                if let Ok(meta) = serde_json::from_str::<Value>(&content) {
                    let pid = meta.get("pid").and_then(|v| v.as_u64()).unwrap_or(0) as i32;
                    let is_alive = pid > 0 && is_process_alive(pid);
                    let effective_state = if *dir_name == "running" && !is_alive { "completed" } else { *dir_name };

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

// ── Helper functions for job management ─────────────────────────────────

fn query_gpu_info() -> Value {
    // Try standard path first, then WSL2 path
    let nvidia_smi = if std::path::Path::new("/usr/lib/wsl/lib/nvidia-smi").exists() {
        "/usr/lib/wsl/lib/nvidia-smi"
    } else {
        "nvidia-smi"
    };

    let output = std::process::Command::new(nvidia_smi)
        .args(["--query-gpu=name,utilization.gpu,memory.used,memory.total,temperature.gpu",
               "--format=csv,noheader,nounits"])
        .output();

    match output {
        Ok(o) if o.status.success() => {
            let s = String::from_utf8_lossy(&o.stdout);
            let parts: Vec<&str> = s.trim().split(',').map(|p| p.trim()).collect();
            json!({
                "name": parts.first().unwrap_or(&""),
                "utilization": parts.get(1).and_then(|v| v.parse::<u32>().ok()).unwrap_or(0),
                "memoryUsedMb": parts.get(2).and_then(|v| v.parse::<u32>().ok()).unwrap_or(0),
                "memoryTotalMb": parts.get(3).and_then(|v| v.parse::<u32>().ok()).unwrap_or(0),
                "temperatureC": parts.get(4).and_then(|v| v.parse::<u32>().ok()).unwrap_or(0),
            })
        }
        _ => json!({ "name": "", "utilization": 0, "memoryUsedMb": 0, "memoryTotalMb": 0, "temperatureC": 0 }),
    }
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

                    Some(json!({ "pid": pid, "type": job_type, "detail": &cmd[..cmd.len().min(120)], "cpu": cpu, "mem": mem }))
                })
                .collect()
        }
        _ => vec![],
    }
}

fn query_job_queue(grid_dir: &std::path::Path) -> Vec<Value> {
    let queue_dir = grid_dir.join("jobs/queued");
    list_meta_files(&queue_dir).iter().filter_map(|path| {
        let name = path.file_stem()?.to_string_lossy().replace(".meta", "");
        Some(json!({ "name": name, "path": path.to_string_lossy() }))
    }).collect()
}

fn gethostname() -> String {
    std::process::Command::new("hostname")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string())
}

fn chrono_now_iso() -> String {
    // Simple ISO 8601 without chrono dependency
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    // Return epoch millis as string — the TS side can format it
    format!("{}Z", now.as_millis())
}

fn find_alloy_executor() -> Option<std::path::PathBuf> {
    let candidates = [
        std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("sentinel-ai/scripts/alloy_executor.py"),
        std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("Development/cambrian/sentinel-ai/scripts/alloy_executor.py"),
    ];
    candidates.into_iter().find(|p| p.exists())
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

fn move_job_files(jobs_dir: &std::path::Path, job_id: &str, from: &str, to: &str) -> std::io::Result<()> {
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
    std::fs::read_dir(dir).ok()
        .map(|entries| {
            let mut files: Vec<_> = entries.flatten()
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
    { unsafe { libc::kill(pid, 0) == 0 } }
    #[cfg(not(unix))]
    { false }
}

fn rand_u32() -> u32 {
    // Simple random without pulling in rand crate
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos()
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
