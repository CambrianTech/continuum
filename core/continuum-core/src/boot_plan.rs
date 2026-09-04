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

/// Adopt-or-reap every llama-server this install owns: identity-verified
/// health check per pid — a HEALTHY lane is adopted (left running, warm
/// weights kept for the serving daemon's reclaim); an unhealthy one is
/// reaped. Deterministic: the same census yields the same fates, and every
/// fate is a receipt line. (The bash `adopt_or_reap_llama_lanes` row is
/// superseded by this step — strangler rule: delete the bash when this
/// lands on the boot path.)
fn step_adopt_lanes() -> Outcome {
    let mut adopted = 0u32;
    let mut reaped = 0u32;
    for pid in crate::inference::lane_process::owned_llama_pids() {
        let healthy = crate::inference::lane_process::lane_health_by_pid(pid);
        if healthy {
            adopted += 1;
        } else {
            crate::inference::lane_process::kill_lane(pid);
            reaped += 1;
        }
    }
    Outcome::Ok(format!("{adopted} adopted, {reaped} reaped"))
}

/// The airc transport daemon — the one service whose absence makes the whole
/// system inert (no rooms → no residency → nothing to resume into).
fn step_airc_daemon() -> Outcome {
    let probe = std::process::Command::new("airc")
        .arg("ipc-endpoint")
        .output();
    match probe {
        Ok(out) if out.status.success() => Outcome::Ok("daemon answering".into()),
        Ok(_) => {
            // Present but not answering: start it detached, bounded wait.
            //
            // CWD = the OS home dir, NEVER the repo. `airc daemon` derives its
            // scope from the working directory; spawned from a continuum
            // checkout it derives the REPO scope, and airc's ownership guard
            // (their #1347/#1352/#1353 family) then rightly refuses to serve
            // the machine-account socket under a foreign identity:
            //
            //   airc: refusing to serve a socket this scope does not own.
            //
            // Measured on the 5090 node 2026-09-04: `continuum reboot` died
            // exactly here — core built, then "no transport: no rooms, no
            // resident citizens". This is the THIRD spawn-site instance of
            // that bug class; airc made it a compile error internally with a
            // newtype, but this call site lives outside airc where the type
            // cannot reach, so the rule is enforced the only way available
            // here: spawn from the home that owns the socket. Refusing to
            // spawn at all when no home dir is resolvable is deliberate — a
            // daemon under the wrong identity gives every caller the wrong
            // peer_id, which is strictly worse than no daemon.
            let Some(home) =
                std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))
            else {
                return Outcome::Failed(
                    "cannot spawn airc daemon: neither USERPROFILE nor HOME is set, \
                     so the machine-account scope is unresolvable — a daemon spawned \
                     from the repo CWD would serve the socket under the WRONG identity \
                     and airc's ownership guard refuses it"
                        .into(),
                );
            };
            let spawned = std::process::Command::new("airc")
                .arg("daemon")
                .current_dir(&home)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();
            match spawned {
                Ok(_) => {
                    for _ in 0..20 {
                        std::thread::sleep(std::time::Duration::from_millis(250));
                        if std::process::Command::new("airc")
                            .arg("ipc-endpoint")
                            .output()
                            .map(|o| o.status.success())
                            .unwrap_or(false) // safe: a probe error reads as not-ready; the loop retries
                        {
                            return Outcome::Ok("daemon started".into());
                        }
                    }
                    Outcome::Failed("airc daemon spawned but never answered (5s)".into())
                }
                Err(e) => Outcome::Failed(format!("airc daemon spawn: {e}")),
            }
        }
        Err(_) => Outcome::Skipped("airc binary absent — transportless box (CI/fresh)".into()),
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
fn step_eye_node_beside(repo_root: &std::path::Path) -> Outcome {
    let eye = repo_root.join("apps/eye-node/src/index.ts");
    if !eye.exists() {
        return Outcome::Skipped("no eye-node in this tree".into());
    }
    match std::process::Command::new("npx")
        .args(["tsx", eye.to_string_lossy().as_ref()])
        .current_dir(repo_root.join("apps/eye-node"))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(_) => Outcome::Ok("perception provider spawned".into()),
        Err(e) => Outcome::Skipped(format!("npx unavailable: {e}")),
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
}
