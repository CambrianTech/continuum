//! THE BOOT REAPS THE EYES IT LEFT BEHIND.
//!
//! `boot_plan::step_eye_node_beside` spawns a fresh eye-node on every boot and never
//! looked for the last one. Measured on the M5 on 2026-09-08, after a week of deploys:
//! 17 eye-node processes up to two days old, and 31 headless Chromium browsers they had
//! launched through Playwright — 29 of those orphaned to launchd (ppid 1) because their
//! eye-node had exited while the browser kept running — 214 processes and 337 leaked
//! temp profile directories in total. Each browser writes its profile continuously, so
//! the fleet was the highest-rate filesystem writer on a box whose fseventsd was holding
//! tens of gigabytes.
//!
//! Two of the three holes in a browser's lifetime are closable in the code that opens it
//! (a failed initialization closes its own browser, #3886; a clean exit closes its
//! session). The third cannot be: `kill -9`, a panic, an OOM kill, or a machine that
//! loses power leaves a browser with no one to close it. That is what this is for — the
//! boot is the one moment the node knows nothing of its own is legitimately running.
//!
//! It is deliberately narrow. A browser is reapable only when BOTH hold: its command
//! line carries one of OUR automation profile markers, and its parent is dead (`ppid`
//! 1). A browser a live eye-node is using is never touched, and neither is anything a
//! person opened.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

/// The temp-profile markers Playwright uses for the browsers the eye launches. A match
/// on one of these is what makes a process OURS rather than a person's browser.
pub const AUTOMATION_PROFILE_MARKERS: [&str; 2] = [
    "playwright_chromiumdev_profile-",
    "org.chromium.Chromium.scoped_dir.",
];

/// How the eye-node itself appears in the process table (`npx tsx …/apps/eye-node/src/index.ts`).
pub const EYE_NODE_MARKER: &str = "apps/eye-node/src/index.ts";

/// A row of `ps -axo pid=,ppid=,command=`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PsRow {
    pub pid: u32,
    pub ppid: u32,
    pub command: String,
}

/// What the boot found to clean up.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reapable {
    /// Orphaned automation browsers: our profile marker, parent already dead.
    pub browsers: Vec<u32>,
    /// Eye-nodes left by earlier boots (every one of them: the boot is about to spawn a
    /// fresh eye, so any existing one is a previous generation).
    pub eye_nodes: Vec<u32>,
    /// The temp profile directories those browsers were writing.
    pub profiles: Vec<PathBuf>,
}

impl Reapable {
    pub fn is_empty(&self) -> bool {
        self.browsers.is_empty() && self.eye_nodes.is_empty() && self.profiles.is_empty()
    }
}

fn is_automation_browser(command: &str) -> bool {
    AUTOMATION_PROFILE_MARKERS.iter().any(|m| command.contains(m))
}

/// The `--user-data-dir=` path of an automation browser, when it is one of ours.
fn profile_dir_of(command: &str) -> Option<PathBuf> {
    let rest = command.split("--user-data-dir=").nth(1)?;
    let path = rest.split_whitespace().next()?;
    is_automation_browser(path).then(|| PathBuf::from(path))
}

/// THE DECISION, pure. `self_pid` is this process, never reaped.
///
/// A browser is reaped only when its parent is dead: a live eye-node's browser is in
/// use, and killing it would break a look in flight. An eye-node is reaped whatever its
/// parent, because the boot is about to spawn its replacement — but never this process
/// and never a process that merely mentions the marker in an argument to something else.
pub fn reapable_from(rows: &[PsRow], self_pid: u32) -> Reapable {
    let mut out = Reapable::default();
    for row in rows {
        if row.pid == self_pid || row.pid <= 1 {
            continue;
        }
        if is_automation_browser(&row.command) {
            // ORPHANS ONLY. ppid 1 means launchd adopted it: nothing owns it now.
            if row.ppid == 1 {
                out.browsers.push(row.pid);
                if let Some(dir) = profile_dir_of(&row.command) {
                    if !out.profiles.contains(&dir) {
                        out.profiles.push(dir);
                    }
                }
            }
            continue;
        }
        if row.command.contains(EYE_NODE_MARKER) {
            out.eye_nodes.push(row.pid);
        }
    }
    out
}

/// Why a process-table read produced nothing — never an empty list (card c7ae34b2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PsFailed {
    Spawn,
    Timeout,
}

impl std::fmt::Display for PsFailed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Spawn => f.write_str("ps could not be run"),
            Self::Timeout => f.write_str("ps did not finish inside the bound"),
        }
    }
}

/// Parse `ps -axo pid=,ppid=,command=`. Rows that do not parse are skipped, never guessed.
pub fn parse_ps_rows(out: &str) -> Vec<PsRow> {
    out.lines()
        .filter_map(|line| {
            let mut it = line.split_whitespace();
            let pid: u32 = it.next()?.parse().ok()?;
            let ppid: u32 = it.next()?.parse().ok()?;
            let command = it.collect::<Vec<_>>().join(" ");
            (!command.is_empty()).then_some(PsRow { pid, ppid, command })
        })
        .collect()
}

/// Read the process table, bounded. The child's stdout is drained while we wait: `ps`
/// prints more than a pipe buffer holds on a developer machine, and a parent that only
/// reads after exit deadlocks (2026-09-08, the first cut of the anomaly reader).
fn ps_rows(timeout: Duration) -> Result<Vec<PsRow>, PsFailed> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let out = Command::new("ps")
            .args(["-axo", "pid=,ppid=,command="])
            .stderr(std::process::Stdio::null())
            .output();
        let _ = tx.send(out);
    });
    match rx.recv_timeout(timeout) {
        Ok(Ok(out)) if out.status.success() => {
            Ok(parse_ps_rows(&String::from_utf8_lossy(&out.stdout)))
        }
        Ok(_) => Err(PsFailed::Spawn),
        Err(_) => Err(PsFailed::Timeout),
    }
}

/// A profile directory is removable only if it is one of ours AND lives under a temp
/// root. Belt and braces: the marker already implies it, and a `rm -rf` deserves both.
fn is_removable_profile(dir: &Path) -> bool {
    let s = dir.to_string_lossy();
    is_automation_browser(&s) && (s.contains("/T/") || s.starts_with("/tmp") || s.contains("/var/folders/"))
}

/// Reap. Returns the counts for the boot receipt; never fails the boot.
pub fn reap(timeout: Duration) -> Result<Reapable, PsFailed> {
    let rows = ps_rows(timeout)?;
    let found = reapable_from(&rows, std::process::id());
    for pid in found.browsers.iter().chain(found.eye_nodes.iter()) {
        kill_hard(*pid);
    }
    for dir in &found.profiles {
        if is_removable_profile(dir) {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
    if !found.is_empty() {
        crate::probe!(
            class = "perception.eye.reaped",
            browsers = found.browsers.len(),
            eye_nodes = found.eye_nodes.len(),
            profiles = found.profiles.len(),
            "the boot reaped the eyes an earlier boot left behind — orphaned automation browsers, their profiles, and previous-generation eye-nodes"
        );
    }
    Ok(found)
}

/// Kill without ceremony. A headless browser whose parent is gone has nothing to flush,
/// and a polite signal to a wedged one is how these survived a week of boots.
#[cfg(unix)]
fn kill_hard(pid: u32) {
    // SAFETY: kill(2) with a pid we read from the process table; an exited pid is ESRCH,
    // which we ignore deliberately — the process being already gone is the goal.
    unsafe {
        libc::kill(pid as libc::pid_t, libc::SIGKILL);
    }
}

/// Windows has no signals; taskkill is the platform's equivalent and /T takes the
/// browser's helper processes with it.
#[cfg(windows)]
fn kill_hard(pid: u32) {
    let _ = Command::new("taskkill")
        .args(["/F", "/T", "/PID", &pid.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(pid: u32, ppid: u32, command: &str) -> PsRow {
        PsRow { pid, ppid, command: command.to_string() }
    }

    // what this catches: the reaper killing a browser a LIVE eye is using, or a person's
    // browser, or missing the orphans that actually accumulated (29 of 31 on the M5,
    // 2026-09-08). The two conditions are load-bearing and easy to loosen by accident.
    #[test]
    fn only_orphaned_automation_browsers_and_previous_eyes_are_reaped() {
        let rows = vec![
            row(100, 1, "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome --headless --user-data-dir=/var/folders/63/x/T/playwright_chromiumdev_profile-mSvwLY"),
            row(101, 90326, "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome --headless --user-data-dir=/var/folders/63/x/T/org.chromium.Chromium.scoped_dir.YbFD3j"),
            row(102, 1, "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"),
            row(103, 1, "/Users/joel/.hermes/node/bin/node --require x /Users/joel/Development/continuum/apps/eye-node/src/index.ts"),
            row(104, 500, "/Applications/Opera.app/Contents/MacOS/Opera"),
            row(1, 0, "/sbin/launchd"),
        ];
        let found = reapable_from(&rows, 999);
        assert_eq!(found.browsers, vec![100], "only the ORPHANED automation browser");
        assert_eq!(found.eye_nodes, vec![103], "the previous generation's eye");
        assert_eq!(
            found.profiles,
            vec![PathBuf::from("/var/folders/63/x/T/playwright_chromiumdev_profile-mSvwLY")],
            "only the reaped browser's profile"
        );
        // A live eye's browser, a person's Chrome, a person's Opera and launchd all survive.
        assert!(!found.browsers.contains(&101) && !found.browsers.contains(&102));
        assert!(!found.eye_nodes.contains(&104));
    }

    // what this catches: reaping OURSELVES, or pid 1, if the core is ever launched in a
    // way that puts a marker on its own command line.
    #[test]
    fn this_process_and_launchd_are_never_reaped() {
        let rows = vec![
            row(42, 1, "continuum-core-server --profile playwright_chromiumdev_profile-x"),
            row(1, 0, "/sbin/launchd playwright_chromiumdev_profile-x"),
        ];
        let found = reapable_from(&rows, 42);
        assert!(found.is_empty(), "{found:?}");
    }

    // what this catches: the ps parser losing rows to path-bearing commands, and a
    // removable-profile check that would let a rm -rf escape the temp root.
    #[test]
    fn ps_rows_parse_and_only_temp_profiles_are_removable() {
        let rows = parse_ps_rows("  100     1 /Applications/Google Chrome.app/Contents/MacOS/Google Chrome --user-data-dir=/var/folders/a/T/playwright_chromiumdev_profile-Z\n garbage\n 7 1 /sbin/launchd\n");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].pid, 100);
        assert_eq!(rows[0].ppid, 1);
        assert!(is_removable_profile(Path::new("/var/folders/a/T/playwright_chromiumdev_profile-Z")));
        assert!(!is_removable_profile(Path::new("/Users/joel/Development/continuum")));
        assert!(!is_removable_profile(Path::new("/Users/joel/playwright_chromiumdev_profile-Z")), "our marker outside a temp root is still not a rm -rf target");
    }
}
