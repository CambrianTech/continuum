//! THE DEPLOY TRACKER MODULE — the fleet's cadence-deploy DECISION, in Rust, wiring the
//! pure core ([`crate::runtime::deploy_tracker`]) to git/gh and the supervisor seam.
//!
//! Joel, 2026-09-17: "It's supposed to be rust for infrastructure not shell jacks." The
//! decision that `tools/scripts/track-canary.sh` (bash + launchd, Unix-only) makes now
//! runs here as a `ServiceModule` — one binary, every platform. It NEVER reboots: on a
//! deploy verdict it records a [`DeployRequest`] (the seam), and the supervisor (card
//! 82af11f5, BigMama's lane) performs the cross-platform build + swap + re-exec.
//!
//! Shape (canonical `ServiceModule`): its own tick, git/gh gathered OFF the tick via
//! `bounded_command::probe` (a launch-path probe gets a bound + a named outcome, the 9/5
//! law), the pure [`decide`] applied, a probe at every seam. A source outage DEGRADES
//! (`SourceUnavailable`) — the running build stands, never a wedge, never a deploy on a
//! guess. `gh` manages its own rate-limiting, so the reqwest rate-limit primitive
//! (`provisioning::rate_limit`, for direct HF calls) is not threaded here.

use std::any::Any;
use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;

use crate::runtime::deploy_tracker::{
    decide, Checks, DeployRequest, DeploySource, DeployVerdict, Hold, RequestOutcome,
    TickInputs,
};
use crate::runtime::{CommandResult, ModuleConfig, ModulePriority, ServiceModule};

/// The deploy tracker checks the tip on this cadence.
pub const DEPLOY_TRACK_INTERVAL: Duration = Duration::from_secs(300);
const GIT_TIMEOUT: Duration = Duration::from_secs(30);
const GH_TIMEOUT: Duration = Duration::from_secs(20);

/// The running core's build SHA — the compile-time constant the binary was stamped with,
/// which IS the SHA a fresh deploy would replace.
fn running_sha() -> &'static str {
    env!("CONTINUUM_BUILD_GIT_SHA")
}

/// Map GitHub's check-runs JSON to a [`Checks`] verdict — the exact rule track-canary
/// applied: any hard-failed run is red, any not-completed run is pending, all-completed
/// is green, and NO runs is unknown (never green — a tip with no CI yet must wait, not
/// deploy). Pure over the JSON body so it is assertable without gh.
pub fn parse_check_runs(body: &str) -> Checks {
    let Ok(v) = serde_json::from_str::<Value>(body) else {
        return Checks::Unknown;
    };
    let runs = match v.get("check_runs").and_then(|r| r.as_array()) {
        Some(r) if !r.is_empty() => r,
        _ => return Checks::Unknown,
    };
    let hard_fail = ["failure", "timed_out", "cancelled", "action_required"];
    if runs.iter().any(|r| {
        r.get("conclusion")
            .and_then(|c| c.as_str())
            .map(|c| hard_fail.contains(&c))
            .unwrap_or(false) // unwrap_or: a run with no conclusion is not a hard-fail
    }) {
        return Checks::Red;
    }
    if runs
        .iter()
        .any(|r| r.get("status").and_then(|s| s.as_str()) != Some("completed"))
    {
        return Checks::Pending;
    }
    Checks::Green
}

/// The git+gh source of the deployable tip. Reads the tracked branch's tip SHA (after a
/// bounded fetch) and its check-state, all through `bounded_command::probe`.
struct GitGhDeploySource {
    repo_dir: PathBuf,
    repo: String,
    branch: String,
}

impl GitGhDeploySource {
    /// Resolve from config/env: `CONTINUUM_TRACK_REPO_DIR` (the checkout) + the origin
    /// remote's `owner/name` + `CONTINUUM_TRACK_BRANCH` (default `canary`). `None` when no
    /// checkout is configured — the module then simply never has a source (degraded, safe).
    fn from_env() -> Option<Self> {
        let repo_dir = crate::config_env::read("CONTINUUM_TRACK_REPO_DIR")
            .map(PathBuf::from)
            .or_else(|| {
                // Build-time source root (this node built here): core/continuum-core → repo.
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .ancestors()
                    .nth(2)
                    .map(|p| p.to_path_buf())
            })
            .filter(|p| p.join(".git").exists())?;
        let branch = crate::config_env::read("CONTINUUM_TRACK_BRANCH")
            .unwrap_or_else(|| "canary".to_string()); // unwrap_or_else: no branch configured = the canary default
        let repo = origin_repo(&repo_dir)?;
        Some(Self { repo_dir, repo, branch })
    }
}

/// `owner/name` from the checkout's origin remote (github only) — nothing hardcoded, the
/// tenant-neutrality law: this repo is for other people and their orgs.
fn origin_repo(repo_dir: &std::path::Path) -> Option<String> {
    let dir = repo_dir.to_string_lossy();
    let out = crate::system_resources::bounded_command::probe(
        "git",
        &["-C", &dir, "remote", "get-url", "origin"],
        GIT_TIMEOUT,
    );
    let url = out.stdout_if_ok()?.trim();
    let after_host = url
        .rsplit_once("github.com")
        .map(|(_, r)| r.trim_start_matches([':', '/']))
        .unwrap_or(url); // unwrap_or: a non-github URL passes through unchanged
    Some(after_host.trim_end_matches(".git").trim_matches('/').to_string())
}

#[async_trait]
impl DeploySource for GitGhDeploySource {
    async fn tip(&self) -> Result<Option<(String, Checks)>, String> {
        let dir = self.repo_dir.to_string_lossy().into_owned();
        let branch = self.branch.clone();
        let repo = self.repo.clone();
        tokio::task::spawn_blocking(move || {
            use crate::system_resources::bounded_command::probe;
            // Fetch (bounded); a fetch failure is a source outage, not a deploy trigger.
            let fetched = probe("git", &["-C", &dir, "fetch", "-q", "origin", &branch], GIT_TIMEOUT);
            if fetched.stdout_if_ok().is_none() && fetched.outcome() != "ok" {
                return Err(format!("git fetch {branch} {}", fetched.outcome()));
            }
            let tip_out = probe("git", &["-C", &dir, "rev-parse", &format!("origin/{branch}")], GIT_TIMEOUT);
            let Some(tip) = tip_out.stdout_if_ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()) else {
                return Err(format!("git rev-parse origin/{branch} {}", tip_out.outcome()));
            };
            // Check-state via gh (gh manages its own rate-limiting). gh unreachable → Unknown → wait.
            let path = format!("repos/{repo}/commits/{tip}/check-runs?per_page=100");
            let gh_out = probe("gh", &["api", &path], GH_TIMEOUT);
            let checks = gh_out.stdout_if_ok().map(parse_check_runs).unwrap_or(Checks::Unknown); // unwrap_or: gh unreachable = Unknown = wait, never a deploy on a guess
            Ok(Some((tip, checks)))
        })
        .await
        .map_err(|e| format!("deploy-source task join error: {e}"))?
    }
}

/// The deploy hold, read as a substrate fact from `state/deploy-hold` — a JSON [`Hold`]
/// (new form, with a TTL), or, for a legacy bash-written text file, a TTL-LESS hold whose
/// reason is the file's text (so an old hold is still honored AND flagged stale by the
/// pure core, the ee76c0df fix). `None` = no hold.
fn read_hold(state_dir: &std::path::Path) -> Option<Hold> {
    let text = std::fs::read_to_string(state_dir.join("deploy-hold")).ok()?;
    if let Ok(hold) = serde_json::from_str::<Hold>(&text) {
        return Some(hold);
    }
    let created_ms = std::fs::metadata(state_dir.join("deploy-hold"))
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0); // unwrap_or: an unreadable mtime = epoch 0 = immediately stale, which is loud not silent
    Some(Hold { reason: text.trim().chars().take(200).collect(), created_ms, ttl_ms: None })
}

/// Atomically record the deploy request the supervisor consumes.
/// The recorded request, if one stands. The other half of [`write_deploy_request`] —
/// until this existed the seam was write-only and nothing ever learned whether what was
/// asked for arrived.
fn read_deploy_request(state_dir: &std::path::Path) -> Option<DeployRequest> {
    let text = std::fs::read_to_string(state_dir.join("deploy-request.json")).ok()?;
    serde_json::from_str::<DeployRequest>(&text).ok()
}

/// Retire a request the running build has satisfied. Removing the file IS the record
/// that it settled: a later tick finds nothing owed rather than re-deciding a deploy that
/// already happened, and an operator reading the state dir sees only what is outstanding.
fn clear_deploy_request(state_dir: &std::path::Path) {
    let _ = std::fs::remove_file(state_dir.join("deploy-request.json"));
}

fn write_deploy_request(state_dir: &std::path::Path, req: &DeployRequest) {
    let _ = std::fs::create_dir_all(state_dir);
    if let Ok(bytes) = serde_json::to_vec_pretty(req) {
        let path = state_dir.join("deploy-request.json");
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, bytes).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    }
}

pub struct DeployTrackerModule {
    source: Option<Box<dyn DeploySource>>,
    root: PathBuf,
    repo_dir: Option<PathBuf>,
    /// The tip last reported as stranded, so a durable condition is said ONCE rather than
    /// every tick — the chatty-floor failure that buries the line it exists to surface.
    stranded_reported: parking_lot::Mutex<Option<String>>,
}

impl DeployTrackerModule {
    pub fn new() -> Self {
        let source = GitGhDeploySource::from_env();
        let repo_dir = source.as_ref().map(|s| s.repo_dir.clone());
        let root = crate::commands::benchmark::continuum_home().unwrap_or_else(|_| PathBuf::from(".")); // unwrap_or_else: no home = cwd; the deploy source degrades, never deploys on a guess
        Self {
            source: source.map(|s| Box::new(s) as Box<dyn DeploySource>),
            root,
            repo_dir,
            stranded_reported: parking_lot::Mutex::new(None),
        }
    }

    fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0) // unwrap_or: a clock before 1970 = 0; the decision degrades safely
    }

    fn tree_dirty(&self) -> bool {
        let Some(dir) = self.repo_dir.as_ref().map(|p| p.to_string_lossy().into_owned()) else {
            return false;
        };
        let out = crate::system_resources::bounded_command::probe(
            "git",
            // TRACKED changes only. An untracked file cannot change what a detached checkout
            // of the tip builds, and the 5090's first unattended deploy sat at RefuseDirty
            // behind a stale sock lock and five locally regenerated ts-rs bindings
            // (2026-09-19 03:3xZ). Same rule the consumer applies before it checks the tip out.
            &["-C", &dir, "status", "--porcelain", "--untracked-files=no"],
            GIT_TIMEOUT,
        );
        out.stdout_if_ok().map(|s| !s.trim().is_empty()).unwrap_or(false) // unwrap_or: an unreadable git status = assume clean; a real dirty tree still refuses via the guard
    }
}

impl Default for DeployTrackerModule {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ServiceModule for DeployTrackerModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "deploy_tracker",
            priority: ModulePriority::Background,
            command_prefixes: &[],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 1,
            tick_interval: Some(DEPLOY_TRACK_INTERVAL),
        }
    }

    async fn initialize(&self, _ctx: &crate::runtime::ModuleContext) -> Result<(), String> {
        Ok(())
    }

    async fn tick(&self) -> Result<(), String> {
        let now = Self::now_ms();
        let (tip_sha, source_error, checks) = match &self.source {
            Some(src) => match src.tip().await {
                Ok(Some((sha, checks))) => (Some(sha), None, checks),
                Ok(None) => (None, Some("branch has no tip".to_string()), Checks::Unknown),
                Err(e) => (None, Some(e), Checks::Unknown),
            },
            None => (None, Some("no checkout configured (CONTINUUM_TRACK_REPO_DIR)".to_string()), Checks::Unknown),
        };
        let build_in_flight =
            crate::runtime::deploy_claim::in_flight(&self.root, now).blocks();
        let inputs = TickInputs {
            running_sha: Some(running_sha().to_string()),
            tip_sha,
            source_error,
            checks,
            hold: read_hold(&self.root.join("state")),
            build_in_flight,
            tree_dirty: self.tree_dirty(),
            now_ms: now,
        };

        // CLOSE THE LOOP BEFORE DECIDING THE NEXT ONE. `decide` writes a DeployRequest and
        // nothing ever read one back, so "the deploy I asked for has arrived" was not a
        // fact this node held — the only way to learn it was to poll `continuum ping`
        // until the sha changed (measured 2026-09-17: two hours of an operator
        // re-deriving by hand a comparison available from two values already in hand).
        // Both facts are right here in `inputs`; this just compares them.
        let state_dir = self.root.join("state");
        match crate::runtime::deploy_tracker::reconcile_request(
            read_deploy_request(&state_dir).as_ref(),
            running_sha(),
            build_in_flight,
            now,
        ) {
            RequestOutcome::Settled { tip_sha, waited_ms } => {
                clear_deploy_request(&state_dir);
                crate::probe!(
                    class = "deploy.settled",
                    tip = tip_sha.as_str(),
                    running = running_sha(),
                    waited_ms,
                    "the requested deploy is now the running build — request retired"
                );
            }
            RequestOutcome::Stranded { tip_sha, elapsed_ms } => {
                // Once per stranded tip, not every tick: the condition is durable and the
                // request file on disk is the standing evidence. A per-tick repeat is the
                // chatty-floor failure that buries the line it exists to surface.
                let mut last = self.stranded_reported.lock();
                if last.as_deref() != Some(tip_sha.as_str()) {
                    *last = Some(tip_sha.clone());
                    crate::probe!(
                        class = "deploy.stranded",
                        tip = tip_sha.as_str(),
                        running = running_sha(),
                        elapsed_ms,
                        "a deploy was requested, nothing is building, and the tip is NOT \
                         running — the request did not take (a detached build that died, or \
                         a supervisor that never ran it)"
                    );
                }
            }
            RequestOutcome::InFlight { .. } | RequestOutcome::Nothing => {
                // In flight is already named by the BuildInFlight verdict below; nothing
                // owed is the quiet case and must stay quiet.
            }
        }

        let verdict = decide(&inputs);
        match &verdict {
            DeployVerdict::Deploy { tip_sha } => {
                write_deploy_request(&self.root.join("state"), &DeployRequest::new(tip_sha.clone(), now));
                crate::probe!(
                    class = "deploy.track.request_written",
                    tip = tip_sha.as_str(),
                    running = running_sha(),
                    "deploy wanted — recorded a DeployRequest for the supervisor"
                );
            }
            DeployVerdict::UpToDate => {}
            DeployVerdict::Held { reason, stale } => {
                crate::probe!(
                    class = "deploy.track.decision",
                    verdict = "held",
                    stale = *stale,
                    reason = reason.as_str(),
                    "deploy held — a hold stands (stale holds are flagged, never silent)"
                );
            }
            other => {
                crate::probe!(
                    class = "deploy.track.decision",
                    verdict = ?other,
                    running = running_sha(),
                    "deploy not taken this tick — the guard that applied"
                );
            }
        }
        Ok(())
    }

    async fn handle_command(&self, command: &str, _params: Value) -> Result<CommandResult, String> {
        Err(format!("deploy-tracker has no command surface; '{command}' is unknown"))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: GitHub's check-runs body maps to the same verdict track-canary
    // used — a hard-failed run is red, a not-completed run is pending, all-completed is
    // green, and NO runs is unknown (a tip with no CI must WAIT, never deploy).
    #[test]
    fn check_runs_map_to_the_same_verdict_as_the_shell_owner() {
        assert_eq!(parse_check_runs(r#"{"check_runs":[{"status":"completed","conclusion":"success"}]}"#), Checks::Green);
        assert_eq!(parse_check_runs(r#"{"check_runs":[{"status":"completed","conclusion":"success"},{"status":"in_progress"}]}"#), Checks::Pending);
        assert_eq!(parse_check_runs(r#"{"check_runs":[{"status":"completed","conclusion":"failure"}]}"#), Checks::Red);
        assert_eq!(parse_check_runs(r#"{"check_runs":[{"status":"completed","conclusion":"cancelled"}]}"#), Checks::Red);
        assert_eq!(parse_check_runs(r#"{"check_runs":[]}"#), Checks::Unknown, "no CI yet = wait, never green");
        assert_eq!(parse_check_runs("not json"), Checks::Unknown);
    }
}
