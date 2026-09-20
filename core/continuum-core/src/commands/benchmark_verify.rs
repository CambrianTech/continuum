//! `benchmark/verify` — the substrate-integrity audit as ONE command.
//!
//! Born 2026-08-22, the day the audit it packages was rigged by hand: the #2366
//! oracle fix shipped while all 1,000 cached DS-1000 tasks still staged the
//! outlawed splicing runner, and 76/115 staged checkouts lacked the `.airc`
//! exclude shield. Finding either required an operator hand-writing hash
//! comparisons and sweep scripts — which a weaker driver cannot rig up, so the
//! defects would simply have stood (Joel: *"Must be down to one command, easy to
//! use, with examples in that commands readme; help etc."*).
//!
//! What it checks (and, where safe, heals):
//! - **Fetched-gym caches** carry a fingerprint sidecar matching the CURRENT
//!   adapter ([`crate::cognition::gym::fetched_gym_statuses`]) — a stale cache
//!   would restage an outdated oracle on the next dispatch. Not healed
//!   automatically (re-materializing needs the network); the row names the one
//!   re-fetch command.
//! - **Staged SWE checkouts** have the `.airc`/`.DS_Store` exclude shield, so
//!   substrate artifacts can never enter a graded diff. Healing is local,
//!   idempotent, and on by default (`--heal=false` to only report).
//!
//! Workspace-staged artifacts (a task dir's `run.py` etc.) are deliberately NOT
//! checked: since #2364 every dispatch re-runs the task's `setup_shell`, so those
//! self-heal on the path that uses them.

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx};

#[derive(Debug, Default, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/BenchmarkVerifyParams.ts"
)]
pub struct BenchmarkVerifyParams {
    /// Apply the safe fixes while sweeping (default true). Only local, idempotent
    /// repairs are ever applied — currently the checkout exclude shield. Stale gym
    /// caches are REPORTED with their re-fetch command, never re-fetched implicitly.
    #[serde(default)]
    #[ts(optional)]
    pub heal: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/BenchmarkVerifyGym.ts"
)]
pub struct BenchmarkVerifyGym {
    /// Cache file basename (`ds-1000.jsonl`).
    pub basename: String,
    /// `fresh` | `stale` | `not-fetched`.
    pub state: String,
    /// The one command that fixes a non-fresh state; absent when fresh.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub action: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/BenchmarkVerifyResult.ts"
)]
pub struct BenchmarkVerifyResult {
    /// True iff no gym is stale AND no checkout is left unshielded. (`not-fetched`
    /// does not fail the verdict — an unfetched suite stages nothing.)
    pub ok: bool,
    /// Cache health per contracted fetched-gym suite.
    pub gyms: Vec<BenchmarkVerifyGym>,
    /// Staged SWE checkouts examined (`<home>/citizens/peers/*/workspace/swe/*`).
    #[ts(type = "number")]
    pub checkouts_swept: u64,
    /// Checkouts that were missing the exclude shield and got it written this run.
    #[ts(type = "number")]
    pub checkouts_healed: u64,
    /// Checkouts still unshielded after the sweep (only non-zero with `heal=false`).
    #[ts(type = "number")]
    pub checkouts_unshielded: u64,
}

#[derive(Default)]
pub struct BenchmarkVerify;

#[async_trait]
impl ActionCommand for BenchmarkVerify {
    const NAME: &'static str = "benchmark/verify";
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Audit the benchmark substrate's integrity in one shot: every fetched-gym cache is \
         checked against its CURRENT adapter fingerprint (a stale cache restages an outdated \
         oracle — the 2026-08-22 defect where 1,000 cached DS-1000 tasks kept a broken runner \
         after the fix shipped), and every staged SWE checkout is checked for the .airc/.DS_Store \
         exclude shield (unshielded trees let substrate artifacts into graded diffs). Safe local \
         repairs (the shield) are applied by default; stale caches are reported with the exact \
         re-fetch command. Run it after any deploy that touched benchmark adapters, before \
         dispatching a round, or whenever a grade looks infra-shaped. Examples: \
         `continuum benchmark/verify` (audit + heal), `continuum benchmark/verify --heal=false` \
         (report only).";
    type Params = BenchmarkVerifyParams;
    type Output = BenchmarkVerifyResult;

    async fn run(
        &self,
        _ctx: &Ctx,
        p: BenchmarkVerifyParams,
    ) -> Result<BenchmarkVerifyResult, CommandError> {
        let heal = p.heal.unwrap_or(true); // the command's documented default: verify AND apply the safe fixes
        let gyms: Vec<BenchmarkVerifyGym> = crate::cognition::gym::fetched_gym_statuses()
            .into_iter()
            .map(|s| BenchmarkVerifyGym {
                basename: s.basename,
                state: s.state.to_string(),
                action: s.action,
            })
            .collect();

        let mut swept = 0u64;
        let mut healed = 0u64;
        let mut unshielded = 0u64;
        let peers = crate::commands::benchmark::continuum_home()?
            .join("citizens")
            .join("peers");
        if let Ok(citizens) = std::fs::read_dir(&peers) {
            for citizen in citizens.flatten() {
                let swe = citizen.path().join("workspace").join("swe");
                let Ok(trees) = std::fs::read_dir(&swe) else {
                    continue; // a citizen with no SWE stagings has nothing to shield
                };
                for tree in trees.flatten() {
                    let repo = tree.path();
                    if !repo.join(".git").is_dir() {
                        continue;
                    }
                    swept += 1;
                    let exclude = repo.join(".git/info/exclude");
                    let shielded = std::fs::read_to_string(&exclude)
                        .map(|t| t.lines().any(|l| l.trim() == ".airc/"))
                        .unwrap_or(false); // unreadable/absent = unshielded; that IS the finding
                    if shielded {
                        continue;
                    }
                    if heal {
                        crate::cognition::swe_bench::shield_workspace_excludes(&repo);
                        healed += 1;
                    } else {
                        unshielded += 1;
                    }
                }
            }
        }

        let ok = gyms.iter().all(|g| g.state != "stale") && unshielded == 0;
        // The verdict also rides the event stream, not only this caller's result:
        // the academy widgets (and any citizen's perception) watch events, and a
        // verify that only answered the invoker would be invisible to the room.
        crate::probe!(
            class = "benchmark.verify",
            ok = %ok,
            stale_gyms = %gyms.iter().filter(|g| g.state == "stale").count(),
            checkouts_swept = %swept,
            checkouts_healed = %healed,
            checkouts_unshielded = %unshielded,
            "benchmark substrate integrity verdict"
        );
        Ok(BenchmarkVerifyResult {
            ok,
            gyms,
            checkouts_swept: swept,
            checkouts_healed: healed,
            checkouts_unshielded: unshielded,
        })
    }
}

crate::register_stateless_command!(BenchmarkVerify);

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the verdict logic inverting — `ok` claiming health while a
    // stale gym or unshielded checkout stands would turn the one-command audit into
    // the exact false-green it was built to prevent. (State strings come from
    // gym::fetched_gym_statuses; this pins how the verdict folds them.)
    #[test]
    fn the_verdict_fails_on_stale_gyms_or_unshielded_checkouts() {
        let stale = BenchmarkVerifyGym {
            basename: "ds-1000.jsonl".into(),
            state: "stale".into(),
            action: Some("continuum benchmark/fetch --benchmark ds-1000".into()),
        };
        let fresh = BenchmarkVerifyGym {
            basename: "algotune.jsonl".into(),
            state: "fresh".into(),
            action: None,
        };
        let unfetched = BenchmarkVerifyGym {
            basename: "super-masked.jsonl".into(),
            state: "not-fetched".into(),
            action: Some("continuum benchmark/fetch --benchmark super-masked".into()),
        };
        let verdict = |gyms: &[BenchmarkVerifyGym], unshielded: u64| {
            gyms.iter().all(|g| g.state != "stale") && unshielded == 0
        };
        assert!(verdict(&[fresh.clone(), unfetched.clone()], 0), "not-fetched alone stays ok");
        assert!(!verdict(&[fresh.clone(), stale], 0), "one stale gym fails the verdict");
        assert!(!verdict(&[fresh, unfetched], 3), "unshielded checkouts fail the verdict");
        assert_eq!(BenchmarkVerify::NAME, "benchmark/verify");
    }
}
