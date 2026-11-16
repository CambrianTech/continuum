//! A pull request read at its head: the one read the checks report and the merge gate judge, so a
//! merge lands exactly the commit its checks and its review covered. GitHub refuses the merge when
//! the head moved after this read (`--match-head-commit`), so nothing lands unjudged.

use std::path::Path;

use airc_work::GitObjectId;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::pr_view::GithubPrState;
use super::{run_gh_owned, GhRunError};

/// A pull request by number, in the repository the workspace pushes to unless `repo` names another.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrAddress<'a> {
    /// `owner/name`; `None` lets `gh` read it from the workspace's remote.
    pub repo: Option<&'a str>,
    pub number: u64,
}

impl PrAddress<'_> {
    /// `gh <verb..> <number> [--repo <repo>]`: the arguments every pull-request call shares.
    pub fn gh_args(&self, verb: &[&str]) -> Vec<String> {
        let mut args: Vec<String> = verb.iter().map(|word| word.to_string()).collect();
        args.push(self.number.to_string());
        if let Some(repo) = self.repo {
            args.extend(["--repo".to_string(), repo.to_string()]);
        }
        args
    }

    /// The repository segment of a `gh api` path: the one named, or `gh`'s own placeholders, which
    /// it fills from the workspace's remote.
    pub fn api_repo(&self) -> String {
        self.repo.map_or_else(|| "{owner}/{repo}".to_string(), str::to_string)
    }
}

/// Where one check stands at the head. Declared in the order a reader acts on them: failures
/// first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../protocol/typescript/code/GithubCheckState.ts")]
pub enum GithubCheckState {
    Failed,
    Pending,
    Passed,
    Skipped,
}

/// One annotation a failed job left: a compiler's or test runner's own line, at its file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/code/GithubCheckAnnotation.ts")]
pub struct GithubCheckAnnotation {
    pub path: String,
    #[ts(type = "number")]
    pub line: u64,
    pub level: String,
    #[ts(optional)]
    pub title: Option<String>,
    pub message: String,
}

/// A failed job's annotations, or why GitHub did not serve them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "state", rename_all = "snake_case")]
#[ts(export, export_to = "../../../protocol/typescript/code/GithubAnnotationsRead.ts")]
pub enum GithubAnnotationsRead {
    Read { annotations: Vec<GithubCheckAnnotation> },
    Unavailable { why: String },
}

/// A failed job's failing-step log, or why GitHub did not serve it (a log is served some time after
/// the job ends; the annotations are there sooner).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "state", rename_all = "snake_case")]
#[ts(export, export_to = "../../../protocol/typescript/code/GithubFailedLogRead.ts")]
pub enum GithubFailedLogRead {
    Read { log: String },
    Unavailable { why: String },
}

/// What a failed workflow job left to read, whole: your view shows what fits, and the rest stays
/// reachable through the result's handle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/code/GithubCheckFailure.ts")]
pub struct GithubCheckFailure {
    pub annotations: GithubAnnotationsRead,
    pub log: GithubFailedLogRead,
}

/// One check GitHub ran on the head: a workflow job or a commit status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/code/GithubCheck.ts")]
pub struct GithubCheck {
    pub name: String,
    #[ts(optional)]
    pub workflow: Option<String>,
    pub state: GithubCheckState,
    /// GitHub's own word for how it ended (`FAILURE`, `TIMED_OUT`, ...), once it has.
    #[ts(optional)]
    pub conclusion: Option<String>,
    #[ts(optional)]
    pub link: Option<String>,
    /// For a failed workflow job: its annotations and its failing steps' log.
    #[ts(optional)]
    pub failure: Option<GithubCheckFailure>,
}

/// Whether GitHub can merge the head into the base as it stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Mergeable {
    Mergeable,
    Conflicting,
    /// GitHub has not computed it yet; it does when first asked.
    Unknown,
}

/// GitHub's merge state for the pull request (`mergeStateStatus`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MergeState {
    Behind,
    Blocked,
    Clean,
    Dirty,
    Draft,
    HasHooks,
    Unknown,
    Unstable,
}

/// A check as the head carries it, with the workflow job it ran as when it is one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RolledCheck {
    pub check: GithubCheck,
    /// The workflow job id, read from the check's link (`.../actions/runs/<run>/job/<job>`).
    pub job: Option<u64>,
}

/// The pull request at its head.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrAtHead {
    pub number: u64,
    pub url: String,
    pub state: GithubPrState,
    pub mergeable: Mergeable,
    pub merge_state: MergeState,
    /// The branch it merges into.
    pub base: String,
    pub head: GitObjectId,
    pub checks: Vec<RolledCheck>,
}

impl PrAtHead {
    /// `owner/name`, read from the pull request's own URL (`https://<host>/<owner>/<name>/pull/<n>`).
    pub fn repo(&self) -> Option<String> {
        let (before, _) = self.url.split_once("/pull/")?;
        let mut segments = before.rsplitn(3, '/');
        let name = segments.next()?;
        let owner = segments.next()?;
        Some(format!("{owner}/{name}"))
    }
}

/// Why `gh pr view`'s answer could not be read as a pull request at its head.
#[derive(Debug, thiserror::Error)]
pub enum PrHeadUnread {
    #[error("gh pr view did not answer a pull request: {0}")]
    Json(#[from] serde_json::Error),
    #[error("gh pr view reported a head that is not a commit id: {0}")]
    Head(#[from] airc_work::model::GitObjectIdError),
}

/// The fields of `gh pr view --json` this read takes.
const VIEW_FIELDS: &str = "number,url,state,mergeable,mergeStateStatus,baseRefName,headRefOid,statusCheckRollup";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Viewed {
    number: u64,
    url: String,
    state: GithubPrState,
    mergeable: Mergeable,
    merge_state_status: MergeState,
    base_ref_name: String,
    head_ref_oid: String,
    #[serde(default)]
    status_check_rollup: Option<Vec<Rolled>>,
}

#[derive(Deserialize)]
#[serde(tag = "__typename")]
enum Rolled {
    #[serde(rename_all = "camelCase")]
    CheckRun {
        name: String,
        status: String,
        #[serde(default)]
        conclusion: Option<String>,
        #[serde(default)]
        details_url: Option<String>,
        #[serde(default)]
        workflow_name: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    StatusContext {
        context: String,
        state: String,
        #[serde(default)]
        target_url: Option<String>,
    },
}

/// A workflow run's state from GitHub's words: running until `COMPLETED`, then by its conclusion.
/// A conclusion this does not know reads as failed, so a check nobody understood never merges.
fn run_state(status: &str, conclusion: Option<&str>) -> GithubCheckState {
    match (status, conclusion) {
        ("COMPLETED", Some("SUCCESS")) => GithubCheckState::Passed,
        ("COMPLETED", Some("SKIPPED" | "NEUTRAL")) => GithubCheckState::Skipped,
        ("COMPLETED", _) => GithubCheckState::Failed,
        _ => GithubCheckState::Pending,
    }
}

/// A commit status's state from GitHub's words.
fn status_state(state: &str) -> GithubCheckState {
    match state {
        "SUCCESS" => GithubCheckState::Passed,
        "PENDING" | "EXPECTED" => GithubCheckState::Pending,
        _ => GithubCheckState::Failed,
    }
}

/// The job id in a workflow job's link, `.../actions/runs/<run>/job/<job>`.
fn job_of(link: &str) -> Option<u64> {
    let (_, job) = link.rsplit_once("/job/")?;
    job.split(['/', '?', '#']).next()?.parse().ok()
}

impl From<Rolled> for RolledCheck {
    fn from(rolled: Rolled) -> Self {
        match rolled {
            Rolled::CheckRun { name, status, conclusion, details_url, workflow_name } => {
                let conclusion = conclusion.filter(|c| !c.is_empty());
                let state = run_state(&status, conclusion.as_deref());
                let job = details_url.as_deref().and_then(job_of);
                RolledCheck { check: GithubCheck { name, workflow: workflow_name, state, conclusion, link: details_url, failure: None }, job }
            }
            Rolled::StatusContext { context, state, target_url } => RolledCheck {
                check: GithubCheck { name: context, workflow: None, state: status_state(&state), conclusion: Some(state), link: target_url, failure: None },
                job: None,
            },
        }
    }
}

/// `gh pr view --json`'s answer, read at its head.
fn at_head(viewed: &str) -> Result<PrAtHead, PrHeadUnread> {
    let v: Viewed = serde_json::from_str(viewed.trim())?;
    Ok(PrAtHead {
        number: v.number,
        url: v.url,
        state: v.state,
        mergeable: v.mergeable,
        merge_state: v.merge_state_status,
        base: v.base_ref_name,
        head: GitObjectId::new(v.head_ref_oid)?,
        checks: v.status_check_rollup.into_iter().flatten().map(RolledCheck::from).collect(),
    })
}

/// Read pull request `pr` at its head, from `root`.
pub async fn read_at_head(root: &Path, pr: &PrAddress<'_>) -> Result<PrAtHead, GhRunError> {
    let mut args = pr.gh_args(&["pr", "view"]);
    args.extend(["--json".to_string(), VIEW_FIELDS.to_string()]);
    let viewed = run_gh_owned(root, &args, None).await?;
    at_head(&viewed).map_err(|unread| GhRunError::Incomplete { args: args.join(" "), code: Some(0), note: unread.to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEAD: &str = "cccccccccccccccccccccccccccccccccccccccc";

    fn viewed(rollup: &str) -> String {
        format!(
            r#"{{"number":5990,"url":"https://github.com/acme/widgets/pull/5990","state":"OPEN","mergeable":"MERGEABLE",
            "mergeStateStatus":"CLEAN","baseRefName":"main","headRefOid":"{HEAD}","statusCheckRollup":{rollup}}}"#
        )
    }

    // what this catches: a check misread at the head: a running job read as passed, a failed or
    // unknown conclusion read as anything but failed, a commit status dropped, or the job id lost
    // so its log cannot be fetched.
    #[test]
    fn every_check_on_the_head_reads_with_its_state_and_job() {
        let rollup = r#"[
            {"__typename":"CheckRun","name":"build","status":"COMPLETED","conclusion":"SUCCESS","detailsUrl":"https://github.com/acme/widgets/actions/runs/7/job/70","workflowName":"CI"},
            {"__typename":"CheckRun","name":"test","status":"COMPLETED","conclusion":"FAILURE","detailsUrl":"https://github.com/acme/widgets/actions/runs/7/job/71","workflowName":"CI"},
            {"__typename":"CheckRun","name":"lint","status":"IN_PROGRESS","conclusion":"","detailsUrl":"https://github.com/acme/widgets/actions/runs/7/job/72","workflowName":"CI"},
            {"__typename":"CheckRun","name":"odd","status":"COMPLETED","conclusion":"SOMETHING_NEW","detailsUrl":null,"workflowName":null},
            {"__typename":"StatusContext","context":"deploy/preview","state":"PENDING","targetUrl":"https://preview.example"}
        ]"#;
        let pr = at_head(&viewed(rollup)).expect("a pull request");
        let states: Vec<_> = pr.checks.iter().map(|c| (c.check.name.as_str(), c.check.state, c.job)).collect();
        assert_eq!(
            states,
            [
                ("build", GithubCheckState::Passed, Some(70)),
                ("test", GithubCheckState::Failed, Some(71)),
                ("lint", GithubCheckState::Pending, Some(72)),
                ("odd", GithubCheckState::Failed, None),
                ("deploy/preview", GithubCheckState::Pending, None),
            ]
        );
        assert_eq!(pr.checks[2].check.conclusion, None, "a running job has no conclusion yet");
        assert_eq!(pr.head, GitObjectId::new(HEAD).expect("a commit id"));
        assert_eq!(pr.repo().as_deref(), Some("acme/widgets"));
    }

    // what this catches: a pull request with no CI read as an error, or an answer that is not a PR
    // (or carries no commit at its head) read as one with no checks.
    #[test]
    fn no_checks_is_an_empty_list_and_a_bad_answer_is_said() {
        assert!(at_head(&viewed("null")).expect("a PR without CI").checks.is_empty());
        assert!(at_head(&viewed("[]")).expect("a PR without CI").checks.is_empty());
        assert!(matches!(at_head("{}"), Err(PrHeadUnread::Json(_))));
        let headless = viewed("[]").replace(HEAD, "");
        assert!(matches!(at_head(&headless), Err(PrHeadUnread::Head(_))));
    }

    // what this catches: a named repository not reaching `gh`, or the workspace's repository
    // spelled as a literal name instead of `gh`'s placeholders.
    #[test]
    fn an_address_names_its_repository_only_when_given() {
        let here = PrAddress { repo: None, number: 7 };
        assert_eq!(here.gh_args(&["pr", "view"]), ["pr", "view", "7"]);
        assert_eq!(here.api_repo(), "{owner}/{repo}");
        let there = PrAddress { repo: Some("acme/widgets"), number: 7 };
        assert_eq!(there.gh_args(&["pr", "diff"]), ["pr", "diff", "7", "--repo", "acme/widgets"]);
        assert_eq!(there.api_repo(), "acme/widgets");
    }
}
