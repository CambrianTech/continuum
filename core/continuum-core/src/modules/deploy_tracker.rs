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

/// The events whose check-suites ARE the tip's own verdict. A suite some other event
/// attached to the same sha (a `schedule`, a `workflow_run` chained off another branch)
/// judged something else and merely landed here.
const OWN_SUITE_EVENTS: [&str; 3] = ["push", "pull_request", "workflow_dispatch"];

/// Workflows whose check-runs are NEVER the tip's verdict, by path, whatever event started
/// them. `promote-main` (card 7c0990b0) runs on the default branch on a schedule AND by
/// hand (`workflow_dispatch`, an own-suite event); it moves main to canary's tip and its
/// failure says nothing about the tip's code — a red run there must not read as a red
/// tip and refuse every deploy on the fleet (the #4243 shape, by a different door).
const NON_DEPLOY_WORKFLOW_PATHS: [&str; 1] = [".github/workflows/promote-main.yml"];

/// The tip's verdict plus what the read excluded — the numbers a probe wants when a
/// verdict surprises someone reading the tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TipChecks {
    pub checks: Checks,
    /// Check-runs on the sha in total.
    pub total: usize,
    /// Check-runs excluded as not the tip's own (some other event's suite). Zero when the
    /// workflow-runs read did not answer and every check counted.
    pub excluded: usize,
    /// The workflow-runs read answered, so the own-suite filter applied.
    pub filtered: bool,
}

/// Map GitHub's check-runs JSON (+ the sha's workflow-runs JSON) to a [`Checks`] verdict —
/// the ONE rule for the tip, the same one `track-canary.sh::tip_checks` applies: any
/// hard-failed run is red, any not-completed run is pending, all-completed is green, and
/// NO runs is unknown (never green — a tip with no CI yet must wait, not deploy).
///
/// Judged over the checks the tip's OWN push ran. 2026-09-20 00:06Z a weekly SCHEDULED
/// audit (Node-era paths, fails every Sunday) landed a failure on the canary tip and the
/// all-checks read said "red", refusing every deploy on the fleet until a new tip appeared
/// (#4243 fixed the shell copy of this rule; this is the Rust copy, the only one the
/// Windows `deploy-consume` reads). So: the workflow runs for the sha carry the triggering
/// event, and only check-runs from suites started by [`OWN_SUITE_EVENTS`] count — minus
/// the suites of [`NON_DEPLOY_WORKFLOW_PATHS`], which judge nothing about the tip. The runs
/// API answered with no such suite = unknown, never "green by absence". The runs API did
/// not answer (`None` or not JSON) = every check counts, as before — a degraded read, not
/// a lie. Pure over the JSON bodies so it is assertable without gh.
pub fn parse_tip_checks(check_runs_body: &str, workflow_runs_body: Option<&str>) -> TipChecks {
    let unknown = |total, excluded, filtered| TipChecks { checks: Checks::Unknown, total, excluded, filtered };
    let Ok(v) = serde_json::from_str::<Value>(check_runs_body) else {
        return unknown(0, 0, false);
    };
    let all: Vec<&Value> = v
        .get("check_runs")
        .and_then(|r| r.as_array())
        .map(|r| r.iter().collect())
        .unwrap_or_default(); // unwrap_or_default: no check_runs array = zero checks = Unknown below
    let own_suites: Option<std::collections::HashSet<u64>> = workflow_runs_body
        .and_then(|b| serde_json::from_str::<Value>(b).ok())
        .and_then(|w| w.get("workflow_runs")?.as_array().cloned())
        .map(|runs| {
            runs.iter()
                .filter(|r| r.get("event").and_then(|e| e.as_str()).is_some_and(|e| OWN_SUITE_EVENTS.contains(&e)))
                .filter(|r| !r.get("path").and_then(|p| p.as_str()).is_some_and(|p| NON_DEPLOY_WORKFLOW_PATHS.contains(&p)))
                .filter_map(|r| r.get("check_suite_id").and_then(|id| id.as_u64()))
                .collect()
        });
    let filtered = own_suites.is_some();
    let runs: Vec<&Value> = match &own_suites {
        Some(own) => all
            .iter()
            .copied()
            .filter(|c| c.get("check_suite").and_then(|s| s.get("id")).and_then(|id| id.as_u64()).is_some_and(|id| own.contains(&id)))
            .collect(),
        None => all.clone(),
    };
    let total = all.len();
    let excluded = total - runs.len();
    if runs.is_empty() {
        return unknown(total, excluded, filtered);
    }
    let hard_fail = ["failure", "timed_out", "cancelled", "action_required"];
    let checks = if runs.iter().any(|r| {
        r.get("conclusion")
            .and_then(|c| c.as_str())
            .map(|c| hard_fail.contains(&c))
            .unwrap_or(false) // unwrap_or: a run with no conclusion is not a hard-fail
    }) {
        Checks::Red
    } else if runs
        .iter()
        .any(|r| r.get("status").and_then(|s| s.as_str()) != Some("completed"))
    {
        Checks::Pending
    } else {
        Checks::Green
    };
    TipChecks { checks, total, excluded, filtered }
}

/// The git+gh source of the deployable tip. Reads the tracked branch's tip SHA (after a
/// bounded fetch) and its check-state, all through `bounded_command::probe`.
///
/// The CHECKOUT is not held here: it is resolved on every read through
/// [`crate::runtime::tracked_checkout`] (card 790c6bcb — the IntelMac tracked a pruned
/// lease worktree for 4.5 hours because the dir was resolved once at construction and its
/// disappearance read as "git fetch absent"). A checkout that is gone is named by path.
struct GitGhDeploySource {
    branch: String,
}

impl GitGhDeploySource {
    /// `CONTINUUM_TRACK_BRANCH` (default `canary`). The checkout and the origin repo are
    /// read per tick, not here.
    fn from_env() -> Self {
        let branch = crate::config_env::read("CONTINUUM_TRACK_BRANCH")
            .unwrap_or_else(|| "canary".to_string()); // unwrap_or_else: no branch configured = the canary default
        Self { branch }
    }
}

/// The checkout to track, resolved NOW, as the tick's source error when there is none.
fn checkout_now() -> Result<PathBuf, String> {
    match crate::runtime::tracked_checkout::tracked_checkout() {
        Ok(Some(dir)) => Ok(dir),
        Ok(None) => Err(format!(
            "no checkout configured ({})",
            crate::runtime::tracked_checkout::TRACK_REPO_DIR_KEY
        )),
        Err(e) => Err(e.to_string()),
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
        let branch = self.branch.clone();
        tokio::task::spawn_blocking(move || {
            use crate::system_resources::bounded_command::probe;
            // The checkout, resolved for THIS tick: gone or not a repo is said by path.
            let repo_dir = checkout_now()?;
            let repo = origin_repo(&repo_dir)
                .ok_or_else(|| format!("checkout {} has no github origin", repo_dir.display()))?;
            let dir = repo_dir.to_string_lossy().into_owned();
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
            let Some(check_runs) = gh_out.stdout_if_ok() else {
                return Ok(Some((tip, Checks::Unknown))); // gh unreachable = Unknown = wait, never a deploy on a guess
            };
            // The sha's workflow runs carry the triggering EVENT the check-runs body lacks;
            // without them a scheduled audit's failure reads as the tip's own (#4243). A
            // failed read here degrades to the all-checks rule inside `parse_tip_checks`.
            let runs_path = format!("repos/{repo}/actions/runs?head_sha={tip}&per_page=100");
            let runs_out = probe("gh", &["api", &runs_path], GH_TIMEOUT);
            let read = parse_tip_checks(check_runs, runs_out.stdout_if_ok());
            if read.excluded > 0 || !read.filtered {
                crate::probe!(
                    class = "deploy.tip.checks",
                    tip = tip.as_str(),
                    checks = ?read.checks,
                    total = read.total,
                    excluded = read.excluded,
                    filtered = read.filtered,
                    runs_read = runs_out.outcome(),
                    "the tip's verdict is judged over its own push's suites; excluded = another event's check-runs on the same sha, filtered=false = the runs read failed and every check counted"
                );
            }
            Ok(Some((tip, read.checks)))
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
    source: Box<dyn DeploySource>,
    root: PathBuf,
    /// `<root>/state` — joined once here, not twice per tick (card 948c30c2, row 14).
    state_dir: PathBuf,
    /// The tip last reported as stranded, so a durable condition is said ONCE rather than
    /// every tick — the chatty-floor failure that buries the line it exists to surface.
    stranded_reported: parking_lot::Mutex<Option<String>>,
}

impl DeployTrackerModule {
    pub fn new() -> Self {
        let root = crate::commands::benchmark::continuum_home().unwrap_or_else(|_| PathBuf::from(".")); // unwrap_or_else: no home = cwd; the deploy source degrades, never deploys on a guess
        let state_dir = root.join("state");
        Self {
            source: Box::new(GitGhDeploySource::from_env()),
            root,
            state_dir,
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
        // The same per-tick resolution the source uses; no checkout = nothing to be dirty.
        let Some(dir) = checkout_now().ok().map(|p| p.to_string_lossy().into_owned()) else {
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
        let (tip_sha, source_error, checks) = match self.source.tip().await {
            Ok(Some((sha, checks))) => (Some(sha), None, checks),
            Ok(None) => (None, Some("branch has no tip".to_string()), Checks::Unknown),
            Err(e) => (None, Some(e), Checks::Unknown),
        };
        let build_in_flight =
            crate::runtime::deploy_claim::in_flight(&self.root, now).blocks();
        let inputs = TickInputs {
            running_sha: Some(running_sha().to_string()),
            tip_sha,
            source_error,
            checks,
            hold: read_hold(&self.state_dir),
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
        let state_dir = &self.state_dir;
        match crate::runtime::deploy_tracker::reconcile_request(
            read_deploy_request(state_dir).as_ref(),
            running_sha(),
            build_in_flight,
            now,
        ) {
            RequestOutcome::Settled { tip_sha, waited_ms } => {
                clear_deploy_request(state_dir);
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
                // Write ONLY when the tip changed. An unchanged tip keeps its original
                // `requested_ms`, which is the only record of how long this deploy has been
                // owed — re-stamping it every tick pinned `elapsed_ms` at the tick period and
                // made every age-based decision downstream meaningless (card c48fc453). The
                // probe follows the write, so a standing request stops repeating itself too.
                if let Some(req) = crate::runtime::deploy_tracker::request_to_persist(
                    read_deploy_request(state_dir).as_ref(),
                    tip_sha,
                    now,
                ) {
                    write_deploy_request(state_dir, &req);
                    crate::probe!(
                        class = "deploy.track.request_written",
                        tip = tip_sha.as_str(),
                        running = running_sha(),
                        "deploy wanted — recorded a DeployRequest for the supervisor"
                    );
                }
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

    fn verdict(check_runs: &str, workflow_runs: Option<&str>) -> Checks {
        parse_tip_checks(check_runs, workflow_runs).checks
    }

    // what this catches: GitHub's check-runs body maps to the same verdict track-canary
    // used — a hard-failed run is red, a not-completed run is pending, all-completed is
    // green, and NO runs is unknown (a tip with no CI must WAIT, never deploy). With no
    // workflow-runs read the rule is the all-checks rule it always was.
    #[test]
    fn check_runs_map_to_the_same_verdict_as_the_shell_owner() {
        assert_eq!(verdict(r#"{"check_runs":[{"status":"completed","conclusion":"success"}]}"#, None), Checks::Green);
        assert_eq!(verdict(r#"{"check_runs":[{"status":"completed","conclusion":"success"},{"status":"in_progress"}]}"#, None), Checks::Pending);
        assert_eq!(verdict(r#"{"check_runs":[{"status":"completed","conclusion":"failure"}]}"#, None), Checks::Red);
        assert_eq!(verdict(r#"{"check_runs":[{"status":"completed","conclusion":"cancelled"}]}"#, None), Checks::Red);
        assert_eq!(verdict(r#"{"check_runs":[]}"#, None), Checks::Unknown, "no CI yet = wait, never green");
        assert_eq!(verdict("not json", None), Checks::Unknown);
    }

    // what this catches (card 2d333f25; the canary tip 6d342745b, 2026-09-20 00:06Z): a
    // weekly SCHEDULED audit failed and landed its check-run on the tip alongside the
    // push's three green suites. The all-checks rule read RED and every deploy on the
    // fleet refused. The tip's verdict is its OWN push's suites: the schedule's suite is
    // excluded, the tip is green. The same body with the runs read absent still reads red
    // (degraded, not a lie); a runs read that answers with NO own suite is unknown, never
    // green by absence; and a failure in an OWN suite is still red. The shape is the live
    // API's on that sha (7 check-runs, 4 workflow runs), abbreviated.
    #[test]
    fn a_scheduled_audits_failure_is_not_the_tips_verdict() {
        let checks = r#"{"check_runs":[
            {"name":"audit","status":"completed","conclusion":"failure","check_suite":{"id":96070172760}},
            {"name":"cargo test","status":"completed","conclusion":"success","check_suite":{"id":96068500514}},
            {"name":"drift guard","status":"completed","conclusion":"success","check_suite":{"id":96068500520}},
            {"name":"install smoke","status":"completed","conclusion":"success","check_suite":{"id":96068500507}}
        ]}"#;
        let runs = r#"{"workflow_runs":[
            {"id":35477870572,"event":"schedule","check_suite_id":96070172760},
            {"id":35477208032,"event":"push","check_suite_id":96068500520},
            {"id":35477208024,"event":"push","check_suite_id":96068500507},
            {"id":35477208029,"event":"push","check_suite_id":96068500514}
        ]}"#;
        let read = parse_tip_checks(checks, Some(runs));
        assert_eq!(read, TipChecks { checks: Checks::Green, total: 4, excluded: 1, filtered: true }, "the schedule's failure is excluded: green");
        assert_eq!(parse_tip_checks(checks, None), TipChecks { checks: Checks::Red, total: 4, excluded: 0, filtered: false }, "no runs read = every check counts, as before");
        assert_eq!(parse_tip_checks(checks, Some("not json")), TipChecks { checks: Checks::Red, total: 4, excluded: 0, filtered: false }, "an unparseable runs read = degraded, not green");
        assert_eq!(verdict(checks, Some(r#"{"workflow_runs":[{"id":1,"event":"schedule","check_suite_id":96070172760}]}"#)), Checks::Unknown, "only a schedule suite = no own suite yet = wait, never green by absence");
        let own_failure = r#"{"check_runs":[{"status":"completed","conclusion":"failure","check_suite":{"id":96068500514}}]}"#;
        assert_eq!(verdict(own_failure, Some(runs)), Checks::Red, "a failure in the tip's own suite is the tip's verdict");
        let pending = r#"{"check_runs":[{"status":"in_progress","check_suite":{"id":96068500514}},{"status":"completed","conclusion":"failure","check_suite":{"id":96070172760}}]}"#;
        assert_eq!(verdict(pending, Some(runs)), Checks::Pending, "the schedule's failure does not pre-empt the push's pending suite");
    }

    // what this catches (card 7c0990b0): the promote-main workflow runs on the default
    // branch and can be started by hand — `workflow_dispatch`, an OWN-suite event — so the
    // event filter alone would let its failure read as the tip's. A run of that workflow
    // is excluded by PATH whatever its event; the push's own suites still decide, and a
    // sha whose only own suite is promote-main's is unknown, never green by absence.
    #[test]
    fn a_promote_main_run_is_never_the_tips_verdict_whatever_its_event() {
        let checks = r#"{"check_runs":[
            {"name":"promote-main","status":"completed","conclusion":"failure","check_suite":{"id":11}},
            {"name":"cargo test","status":"completed","conclusion":"success","check_suite":{"id":22}}
        ]}"#;
        let runs = r#"{"workflow_runs":[
            {"id":1,"event":"workflow_dispatch","path":".github/workflows/promote-main.yml","check_suite_id":11},
            {"id":2,"event":"push","path":".github/workflows/continuum-rust-tests.yml","check_suite_id":22}
        ]}"#;
        assert_eq!(parse_tip_checks(checks, Some(runs)), TipChecks { checks: Checks::Green, total: 2, excluded: 1, filtered: true }, "a dispatched promote-main failure is excluded by path: green");
        let in_flight = r#"{"check_runs":[
            {"name":"promote-main","status":"in_progress","check_suite":{"id":11}},
            {"name":"cargo test","status":"completed","conclusion":"success","check_suite":{"id":22}}
        ]}"#;
        assert_eq!(verdict(in_flight, Some(runs)), Checks::Green, "a promote-main run in flight is not a pending tip");
        let only_promote = r#"{"workflow_runs":[{"id":1,"event":"push","path":".github/workflows/promote-main.yml","check_suite_id":11}]}"#;
        assert_eq!(verdict(checks, Some(only_promote)), Checks::Unknown, "promote-main as the only suite = no own suite = wait, never green by absence");
        let own_failure = r#"{"check_runs":[{"name":"cargo test","status":"completed","conclusion":"failure","check_suite":{"id":22}}]}"#;
        assert_eq!(verdict(own_failure, Some(runs)), Checks::Red, "the push's own failure is still the tip's verdict");
    }
}
