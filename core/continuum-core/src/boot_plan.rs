//! The typed boot plan — BOOT-IS-A-TYPED-PLAN.md made real (slice 1).
//!
//! Joel, 2026-09-02: *"Your entire system startup is randomly stuck together
//! with duct tape. You told me you'd make it deterministic."* This module is
//! that commitment: boot is an ORDERED, RECEIPTED sequence of typed steps —
//! not ~920 lines of bash whose rows can't see each other's dependencies.
//!
//! Scope of slice 1 (the strangler rule — a row lives in exactly one world):
//! `continuum boot` owns RUNTIME bring-up of an already-built binary — the
//! startup a user or a fresh install actually experiences: lane
//! adopt-or-reap, airc transport, core launch + #194 SHA verify, and the
//! optional Beside rails (desktop, eye-node) that must NEVER gate the core.
//! The dev-time source build stays in the script until slice 2 migrates it.
//!
//! Every step emits a `boot.step` probe and one row in the printed receipt:
//! `{name, outcome, ms}`. A slow boot names its row; a regression is a diff
//! between two receipts, not a feeling. Required steps abort the plan loudly;
//! optional steps record their skip reason and the plan continues.

use std::time::Instant;

/// Where a step runs relative to the core process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Must complete before the core is launched (transport, lane fate).
    Before,
    /// Spawned alongside the core launch and NOT awaited — the core answers
    /// while these land behind it (desktop, eye-node). A Beside step's
    /// receipt records that it was SPAWNED; its own completion is its own
    /// process's business.
    Beside,
}

/// One step's outcome, as the receipt records it.
#[derive(Debug, Clone)]
pub enum Outcome {
    Ok(String),
    /// Optional step didn't run / didn't apply — the reason IS the receipt.
    Skipped(String),
    Failed(String),
}

pub struct StepReport {
    pub name: &'static str,
    pub outcome: Outcome,
    pub ms: u128,
}

/// The boot receipt: every step, in execution order, with timing. Printed as
/// a table and probed row-by-row — the ONE place "what did boot do" lives.
pub struct BootReceipt {
    pub steps: Vec<StepReport>,
    pub ok: bool,
}

impl BootReceipt {
    pub fn push(&mut self, name: &'static str, started: Instant, outcome: Outcome) {
        let ms = started.elapsed().as_millis();
        let (kind, detail) = match &outcome {
            Outcome::Ok(d) => ("ok", d.clone()),
            Outcome::Skipped(d) => ("skipped", d.clone()),
            Outcome::Failed(d) => ("failed", d.clone()),
        };
        crate::probe!(
            class = "boot.step",
            step = name,
            outcome = kind,
            ms = ms as u64,
            detail = %detail,
            "boot plan step"
        );
        println!("  [{ms:>6}ms] {name:<22} {kind:<8} {detail}");
        self.steps.push(StepReport { name, outcome, ms });
    }
}

/// Adopt-or-reap every llama-server this install owns, by the ONE verdict
/// ([`crate::inference::lane_process::lane_verdict`]): healthy → adopted (warm weights,
/// live KV kept for the serving daemon's reclaim); BUSY (every probe missed but the
/// process is working — a slow answer under load) → adopted too, the core's readiness
/// check proves decode before a citizen is seated; only DEAD (missed and frozen) is
/// reaped. Deterministic over the same evidence, and every fate is a receipt: the
/// boot line and a `boot.lane_verdict` probe carry the pid and the evidence, so a reap
/// can be audited after the fact. The bash `adopt_or_reap_llama_lanes` row this
/// superseded is deleted (it reaped the M5's lane mid-generation twice on 2026-09-20
/// on a single 3 s probe; card b57b19fd) — there is no second criterion.
fn step_adopt_lanes() -> Outcome {
    let mut adopted = 0u32;
    let mut reaped = 0u32;
    let mut lines = Vec::new();
    for pid in crate::inference::lane_process::owned_llama_pids() {
        let verdict = crate::inference::lane_process::lane_verdict(pid);
        let evidence = verdict.evidence();
        crate::probe!(
            class = "boot.lane_verdict",
            pid = pid as u64,
            adopt = verdict.adopt(),
            evidence = evidence.as_str(),
            "the boot path's verdict on a lane it did not spawn — two signals, one receipt"
        );
        if verdict.adopt() {
            adopted += 1;
        } else {
            crate::inference::lane_process::kill_lane(pid);
            reaped += 1;
        }
        lines.push(format!("pid {pid}: {evidence}"));
    }
    Outcome::Ok(format!("{adopted} adopted, {reaped} reaped [{}]", lines.join("; ")))
}

/// The airc transport daemon — the one service whose absence makes the whole
/// system inert (no rooms → no residency → nothing to resume into).
fn step_airc_daemon() -> Outcome {
    use crate::airc::daemon_supervisor::{spawn, Spawned};
    match spawn() {
        Spawned::Answering => Outcome::Ok("daemon answering".into()),
        Spawned::Started { pid } => Outcome::Ok(format!("daemon started (pid {pid}, owned)")),
        Spawned::BinaryAbsent => Outcome::Skipped("airc binary absent — transportless box (CI/fresh)".into()),
        Spawned::NoHome => Outcome::Failed(
            "cannot spawn airc daemon: neither USERPROFILE nor HOME is set, so the \
             machine-account scope is unresolvable — a daemon spawned from the repo CWD \
             would serve the socket under the WRONG identity and airc's ownership guard \
             refuses it"
                .into(),
        ),
        Spawned::Failed(e) => Outcome::Failed(e),
    }
}

/// Beside: the optional desktop build — NEVER gates the core (Joel:
/// "Desktop is optional… depends on core being up"). Spawned detached; its
/// completion is visible via desktop.dm probes and the dist appearing.
fn step_desktop_beside(repo_root: &std::path::Path) -> Outcome {
    if !repo_root.join("apps/web/package.json").exists() {
        return Outcome::Skipped("no web app in this tree (installed user)".into());
    }
    match std::process::Command::new("npm")
        .args(["run", "build", "-w", "@continuum/web"])
        .current_dir(repo_root)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(_) => Outcome::Ok("build spawned beside the core".into()),
        Err(e) => Outcome::Skipped(format!("npm unavailable: {e}")),
    }
}

/// Beside: the eye-node perception provider (retry-dials the core itself).
/// Reap the eyes an earlier boot left behind, BEFORE spawning this boot's eye. Every
/// boot span­ned a fresh eye-node and looked for none of its predecessors: 17 of them and
/// 31 orphaned browsers had accumulated on the M5 by 2026-09-08. Same shape as
/// `step_adopt_lanes` — a boot owns the process tree it is about to add to.
fn step_reap_eyes() -> Outcome {
    match crate::system_resources::eye_reaper::reap(std::time::Duration::from_secs(5)) {
        Ok(found) if found.is_empty() => Outcome::Ok("no eyes left behind".into()),
        Ok(found) => Outcome::Ok(format!(
            "reaped {} orphaned browser(s), {} previous eye-node(s), {} profile dir(s)",
            found.browsers.len(),
            found.eye_nodes.len(),
            found.profiles.len()
        )),
        // A process table we could not read is said, never a silent "nothing to do".
        Err(why) => Outcome::Skipped(format!("could not read the process table: {why}")),
    }
}

fn step_eye_node_beside(repo_root: &std::path::Path) -> Outcome {
    let eye = repo_root.join("apps/eye-node/src/index.ts");
    if !eye.exists() {
        return Outcome::Skipped("no eye-node in this tree".into());
    }
    let tsx = repo_root.join("node_modules/tsx/dist/cli.mjs");
    if !tsx.is_file() {
        return Outcome::Skipped("eye-node needs the installed workspace dependency tsx".into());
    }
    let endpoint = crate::ipc::endpoint_paths::core_socket_path();
    let log_path = std::path::PathBuf::from(crate::ipc::endpoint_paths::core_start_logfile())
        .with_file_name("continuum-eye-node.log");
    let log = match std::fs::File::create(&log_path) {
        Ok(log) => log,
        Err(e) => return Outcome::Skipped(format!("eye-node log {}: {e}", log_path.display())),
    };
    let node = match crate::shell_portable::locate_executable("node")
        .and_then(|path| std::fs::canonicalize(path).ok())
    {
        Some(node) => node,
        None => return Outcome::Skipped("eye-node needs a resolvable node executable on PATH".into()),
    };
    let command = eye_node_command(repo_root, &endpoint, &node);
    match spawn_eye_logged(command, &log) {
        Ok(pid) => Outcome::Ok(format!(
            "eye-node spawned pid {} for {endpoint}; registration pending; log {}",
            pid, log_path.display()
        )),
        Err(e) => Outcome::Skipped(format!("eye-node launch for {endpoint} failed: {e}")),
    }
}

/// Pass the core's endpoint authority to the worker. Invoke the installed JS entry
/// through node directly: npx is a .cmd shim on Windows and may fetch packages.
fn eye_node_command(repo_root: &std::path::Path, endpoint: &str, node: &std::path::Path) -> std::process::Command {
    let mut command = std::process::Command::new(node);
    command
        .arg(repo_root.join("node_modules/tsx/dist/cli.mjs"))
        .arg(repo_root.join("apps/eye-node/src/index.ts"))
        .current_dir(repo_root.join("apps/eye-node"))
        .env("CONTINUUM_CORE_SOCKET", endpoint)
        .stdin(std::process::Stdio::null());
    command
}

fn spawn_eye_logged(command: std::process::Command, log: &std::fs::File) -> std::io::Result<u32> {
    #[cfg(windows)]
    {
        // Reuse the restricted-handle launcher: std Command would let this long-lived
        // child inherit unrelated CLI-host pipes and keep their EOF open (#9bc0fc5e).
        continuum_cli_lifecycle::windows_launch::spawn_logged(
            &command, log, log, 0x0800_0000, // CREATE_NO_WINDOW
        ).map(|child| child.id())
    }
    #[cfg(not(windows))]
    {
        let mut command = command;
        command.stdout(log.try_clone()?).stderr(log.try_clone()?).spawn().map(|child| child.id())
    }
}

/// Run slice 1 of the typed boot plan. `repo_root` is `Some` in a source tree
/// (Beside dev rails apply) and `None` for an installed user (they skip with
/// a stated reason — the receipt never has silent holes).
///
/// Returns the receipt; the CALLER launches the core binary between the
/// Before and Beside phases (it owns binary location + socket + #194 verify,
/// which already live beside it in the CLI) and records that as its own row.
/// Slice 2 pulls the launch itself in here.
pub fn run_before_phase() -> BootReceipt {
    println!("boot plan (slice 1) — every step: [ms] name outcome detail");
    let mut receipt = BootReceipt {
        steps: Vec::new(),
        ok: true,
    };
    let t = Instant::now();
    receipt.push("adopt-or-reap-lanes", t, step_adopt_lanes());

    let t = Instant::now();
    let airc = step_airc_daemon();
    if matches!(airc, Outcome::Failed(_)) {
        receipt.ok = false; // transport is REQUIRED — a system without rooms is not running
    }
    receipt.push("airc-daemon", t, airc);
    receipt
}

/// The Beside phase — call AFTER the core process is launched (never awaited).
pub fn run_beside_phase(receipt: &mut BootReceipt, repo_root: Option<&std::path::Path>) {
    match repo_root {
        Some(root) => {
            let t = Instant::now();
            receipt.push("desktop-beside", t, step_desktop_beside(root));
            let t = Instant::now();
            receipt.push("reap-eyes", t, step_reap_eyes());
            let t = Instant::now();
            receipt.push("eye-node-beside", t, step_eye_node_beside(root));
        }
        None => {
            let t = Instant::now();
            receipt.push(
                "desktop-beside",
                t,
                Outcome::Skipped("installed user — dist ships with the install".into()),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the receipt recording EVERY step with an explicit
    // outcome — a silent hole in the boot receipt is the duct-tape shape this
    // module exists to end (a row that ran but left no evidence).
    #[test]
    fn every_step_leaves_a_receipt_row() {
        let mut r = BootReceipt {
            steps: Vec::new(),
            ok: true,
        };
        let t = Instant::now();
        r.push("x", t, Outcome::Skipped("test".into()));
        assert_eq!(r.steps.len(), 1);
        assert!(matches!(r.steps[0].outcome, Outcome::Skipped(_)));
    }

    // what this catches: a worker guessing /tmp instead of the selected endpoint,
    // or going through npx and shell parsing instead of installed dependencies.
    #[test]
    fn eye_launch_preserves_endpoint_and_paths_as_arguments() {
        let root = std::env::temp_dir().join("source tree with spaces");
        let endpoint = "tcp://127.0.0.1:45678";
        let node = root.join("node executable");
        let command = eye_node_command(&root, endpoint, &node);
        assert_eq!(command.get_program(), node.as_os_str());
        assert_eq!(command.get_current_dir(), Some(root.join("apps/eye-node").as_path()));
        assert_eq!(command.get_args().collect::<Vec<_>>(), vec![
            root.join("node_modules/tsx/dist/cli.mjs").as_os_str(),
            root.join("apps/eye-node/src/index.ts").as_os_str(),
        ]);
        assert!(command.get_envs().any(|(key, value)|
            key == "CONTINUUM_CORE_SOCKET" && value == Some(std::ffi::OsStr::new(endpoint))));
    }
}
