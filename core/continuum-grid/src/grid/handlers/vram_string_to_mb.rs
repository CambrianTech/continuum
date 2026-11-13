use super::*;

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
pub(super) fn vram_string_to_mb(s: &str) -> Option<u32> {
    let mut parts = s.split_whitespace();
    let value: u32 = parts.next()?.parse().ok()?;
    match parts.next()?.to_ascii_uppercase().as_str() {
        u if u.starts_with("GB") => value.checked_mul(1024),
        u if u.starts_with("MB") => Some(value),
        _ => None,
    }
}

pub(super) fn query_gpu_info() -> Value {
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

pub(super) fn query_forge_processes() -> Vec<Value> {
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

                    Some(json!({ "pid": pid, "type": job_type, "detail": crate::logging::preview::truncate_at_char_boundary(&cmd, 120), "cpu": cpu, "mem": mem }))
                })
                .collect()
        }
        _ => vec![],
    }
}

pub(super) fn query_job_queue(grid_dir: &std::path::Path) -> Vec<Value> {
    let queue_dir = grid_dir.join("jobs/queued");
    list_meta_files(&queue_dir)
        .iter()
        .filter_map(|path| {
            let name = path.file_stem()?.to_string_lossy().replace(".meta", "");
            Some(json!({ "name": name, "path": path.to_string_lossy() }))
        })
        .collect()
}

pub(super) fn gethostname() -> String {
    std::process::Command::new("hostname")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string())
}

pub(super) fn find_alloy_executor() -> Option<std::path::PathBuf> {
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

pub(super) fn find_job_meta(jobs_dir: &std::path::Path, job_id: &str) -> Option<Value> {
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

pub(super) fn move_job_files(
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

pub(super) fn list_meta_files(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
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

pub(super) fn is_process_alive(pid: i32) -> bool {
    u32::try_from(pid).is_ok_and(continuum_cli_lifecycle::process::is_alive)
}

pub(super) enum JobSignal {
    Stop,
    Continue,
    Terminate,
}

/// Deliver a job-control signal. Stop/continue have no Windows equivalent here, so they are
/// REFUSED there — the handler used to report "paused"/"running" while nothing happened. Terminate
/// ends the job's whole process tree on Windows (taskkill /T), SIGTERM on Unix.
pub(super) fn job_signal(pid: i32, signal: JobSignal) -> Result<(), String> {
    #[cfg(unix)]
    {
        let sig = match signal {
            JobSignal::Stop => libc::SIGSTOP,
            JobSignal::Continue => libc::SIGCONT,
            JobSignal::Terminate => libc::SIGTERM,
        };
        // SAFETY: plain signal delivery to a positive pid (the callers exclude pid <= 0).
        if unsafe { libc::kill(pid, sig) } != 0 {
            return Err(format!("signal to pid {pid} failed: {}", std::io::Error::last_os_error()));
        }
        Ok(())
    }
    #[cfg(not(unix))]
    {
        match signal {
            JobSignal::Terminate => {
                continuum_process::kill9(pid as u32);
                Ok(())
            }
            JobSignal::Stop | JobSignal::Continue => Err(
                "pause/resume are not supported on Windows (no SIGSTOP/SIGCONT); the job is still running".into(),
            ),
        }
    }
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
        if let Some(addr) = t.local_address() {
            checks.push(json!({
                "check": format!("transport_{}", t.name()),
                "status": "pass",
                "detail": format!("{} listening at {}", t.name(), addr.display_address()),
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
