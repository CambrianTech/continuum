//! `code/github/pr-checks` — a pull request's CI at its head, as a teammate reads it before merging
//! and after a red run: each check's state, failures first, and for a failed workflow job its
//! annotations and the log of its failing steps. Before this a resident could open a PR and never
//! learn why it was red (2026-11-14: Claude read `gh pr checks` and `gh run view --log-failed`,
//! her hands had neither).

use std::path::Path;
use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::pr_head::{read_at_head, GithubAnnotationsRead, GithubCheck, GithubCheckAnnotation, GithubCheckFailure, GithubCheckState, GithubFailedLogRead, PrAddress, RolledCheck};
use super::{run_gh_owned, checkout_root_for, GhRunError};
use crate::modules::code::CodeState;

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/code/GithubPrChecksParams.ts")]
pub struct GithubPrChecksParams {
    /// `owner/name`; omit for your workspace's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub repo: Option<String>,
    /// PR number.
    #[ts(type = "number")]
    pub number: u64,
}

impl GithubPrChecksParams {
    pub fn address(&self) -> PrAddress<'_> {
        PrAddress { repo: self.repo.as_deref(), number: self.number }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/code/GithubPrChecksResult.ts")]
pub struct GithubPrChecksResult {
    #[ts(type = "number")]
    pub number: u64,
    /// The head commit every check here ran on.
    pub head: String,
    /// Failed first, then still running, then passed and skipped.
    pub checks: Vec<GithubCheck>,
}

/// One annotation as GitHub's check-run API answers it.
#[derive(Deserialize)]
struct ApiAnnotation {
    path: String,
    start_line: u64,
    annotation_level: String,
    #[serde(default)]
    title: Option<String>,
    message: String,
}

/// The check-run API's annotation list.
fn annotations_of(answer: &str) -> Result<Vec<GithubCheckAnnotation>, serde_json::Error> {
    let listed: Vec<ApiAnnotation> = serde_json::from_str(answer.trim())?;
    Ok(listed
        .into_iter()
        .map(|a| GithubCheckAnnotation { path: a.path, line: a.start_line, level: a.annotation_level, title: a.title.filter(|t| !t.is_empty()), message: a.message })
        .collect())
}

/// What failed workflow job `job` left to read: its annotations and its failing steps' log, each
/// read on its own, so one GitHub has not served yet never hides the other.
async fn failure_of(root: &Path, pr: &PrAddress<'_>, job: u64) -> GithubCheckFailure {
    let api = vec!["api".to_string(), format!("repos/{}/check-runs/{job}/annotations?per_page=100", pr.api_repo())];
    let annotations = match run_gh_owned(root, &api, None).await {
        Ok(answer) => match annotations_of(&answer) {
            Ok(annotations) => GithubAnnotationsRead::Read { annotations },
            Err(unread) => GithubAnnotationsRead::Unavailable { why: unread.to_string() },
        },
        Err(unserved) => GithubAnnotationsRead::Unavailable { why: unserved.to_string() },
    };
    let mut log_args: Vec<String> = ["run", "view", "--job"].map(str::to_string).to_vec();
    log_args.extend([job.to_string(), "--log-failed".to_string()]);
    if let Some(repo) = pr.repo {
        log_args.extend(["--repo".to_string(), repo.to_string()]);
    }
    let log = match run_gh_owned(root, &log_args, None).await {
        Ok(log) => GithubFailedLogRead::Read { log },
        Err(unserved) => GithubFailedLogRead::Unavailable { why: unserved.to_string() },
    };
    GithubCheckFailure { annotations, log }
}

/// A check as reported: a failed workflow job carries what it left to read.
async fn reported(root: &Path, pr: &PrAddress<'_>, rolled: RolledCheck) -> GithubCheck {
    let RolledCheck { mut check, job } = rolled;
    if let (GithubCheckState::Failed, Some(job)) = (check.state, job) {
        check.failure = Some(failure_of(root, pr, job).await);
    }
    check
}

/// The checks in the order she acts on them: failed, running, passed, skipped; GitHub's order kept
/// within each.
fn ordered(mut checks: Vec<GithubCheck>) -> Vec<GithubCheck> {
    checks.sort_by_key(|check| check.state);
    checks
}

/// Pull request `pr`'s checks at its head, read from `root`. The failed jobs' details are fetched
/// together.
pub async fn checks_of(root: &Path, pr: &PrAddress<'_>) -> Result<GithubPrChecksResult, GhRunError> {
    let at = read_at_head(root, pr).await?;
    let checks = futures::future::join_all(at.checks.into_iter().map(|rolled| reported(root, pr, rolled))).await;
    Ok(GithubPrChecksResult { number: at.number, head: at.head.to_string(), checks: ordered(checks) })
}

crate::action_command! {
    /// A PR's CI at its head: checks failed first, each failed job's annotations and log.
    pub struct CodeGithubPrChecks { state: Arc<CodeState> }
    name: "code/github/pr-checks",
    access: AiSafe,
    native: true, // named by the merge gate's refusals as her next call; one help away (OnDemand)
    effect: Observes,
    disclosure: OnDemand,
    params: GithubPrChecksParams,
    output: GithubPrChecksResult,
    run(this, ctx, p) => {
        let root = checkout_root_for(&this.state, ctx).await?;
        Ok(checks_of(&root, &p.address()).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(name: &str, state: GithubCheckState) -> GithubCheck {
        GithubCheck { name: name.into(), workflow: None, state, conclusion: None, link: None, failure: None }
    }

    // what this catches: a red check buried under green ones, so the reader acts on the wrong one;
    // the order is failed, running, passed, skipped, and GitHub's order holds within each.
    #[test]
    fn failures_lead_and_each_state_keeps_githubs_order() {
        let checks = ordered(vec![
            check("build", GithubCheckState::Passed),
            check("docs", GithubCheckState::Skipped),
            check("lint", GithubCheckState::Pending),
            check("test-a", GithubCheckState::Failed),
            check("test-b", GithubCheckState::Failed),
        ]);
        let names: Vec<&str> = checks.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["test-a", "test-b", "lint", "build", "docs"]);
    }

    // what this catches: a failed job's annotations (the compiler's own error line) lost to a
    // field-name mismatch, a blank title kept as noise, or a malformed answer read as "none".
    #[test]
    fn a_failed_jobs_annotations_read_whole() {
        let answer = r#"[{"path":"src/lib.rs","start_line":42,"end_line":42,"annotation_level":"failure","title":"","message":"error[E0308]: mismatched types","raw_details":null},
            {"path":".github","start_line":1,"end_line":1,"annotation_level":"warning","title":"Node 16","message":"deprecated"}]"#;
        let read = annotations_of(answer).expect("annotations");
        assert_eq!(
            read[0],
            GithubCheckAnnotation { path: "src/lib.rs".into(), line: 42, level: "failure".into(), title: None, message: "error[E0308]: mismatched types".into() }
        );
        assert_eq!(read[1].title.as_deref(), Some("Node 16"));
        assert!(annotations_of("not json").is_err(), "a malformed answer is said, never an empty list");
    }
}
