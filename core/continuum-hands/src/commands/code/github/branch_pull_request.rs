//! A branch's pull request on GitHub: the ONE owner of finding it by head and opening it, through
//! this module's brokered `gh` (the operator's credentials, usable by a citizen, never readable).
//! `code/git/push` asks it whether the pushed branch has a PR, `code/github/pr-create` opens one
//! through it, and `work/submit` ensures the card's PR with both, then links the card.

use std::path::Path;

use serde::Deserialize;

use super::{run_gh_owned, GhRunError};

/// An open pull request, as GitHub reports it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct OpenPullRequest {
    pub number: u64,
    pub url: String,
    /// The branch it merges from.
    #[serde(rename = "headRefName")]
    pub head: String,
    /// The branch it merges into.
    #[serde(rename = "baseRefName")]
    pub base: String,
}

/// The open pull request whose head is `branch`, asked of GitHub once. `repo` (`owner/name`) names
/// the repository when the caller knows it; otherwise `gh` reads it from `root`'s remote.
pub async fn open_pull_request_for(root: &Path, repo: Option<&str>, branch: &str) -> Result<Option<OpenPullRequest>, GhRunError> {
    let mut args: Vec<String> = ["pr", "list", "--state", "open", "--head", branch, "--json", "number,url,headRefName,baseRefName"]
        .map(str::to_string)
        .to_vec();
    if let Some(repo) = repo {
        args.extend(["--repo".to_string(), repo.to_string()]);
    }
    let listed = run_gh_owned(root, &args, None).await?;
    first_listed(&listed).map_err(|why| GhRunError::Incomplete { args: args.join(" "), code: Some(0), note: why })
}

/// `gh pr list --json`'s answer: the first open PR, or none. A list that is not that JSON is said,
/// never read as "no PR".
fn first_listed(listed: &str) -> Result<Option<OpenPullRequest>, String> {
    let open: Vec<OpenPullRequest> = serde_json::from_str(listed.trim()).map_err(|e| format!("gh pr list did not answer a PR list: {e}"))?;
    Ok(open.into_iter().next())
}

/// A pull request to open.
#[derive(Debug, Clone, Copy)]
pub struct NewPullRequest<'a> {
    pub title: &'a str,
    /// Already attributed by the caller (`super::attributed`) when a citizen wrote it.
    pub body: &'a str,
    /// The branch to merge into; `None` lets GitHub use the repository's default branch.
    pub base: Option<&'a str>,
    /// The branch carrying the change; `None` is `root`'s current branch.
    pub head: Option<&'a str>,
    pub draft: bool,
    /// `owner/name`, when the caller knows it.
    pub repo: Option<&'a str>,
}

/// Open `pr` (`gh pr create`) from `root`. Returns the PR's URL.
pub async fn create_pull_request(root: &Path, pr: NewPullRequest<'_>) -> Result<String, GhRunError> {
    let mut args = vec!["pr".to_string(), "create".to_string(), "--title".to_string(), pr.title.to_string(), "--body".to_string(), pr.body.to_string()];
    for (flag, value) in [("--base", pr.base), ("--head", pr.head), ("--repo", pr.repo)] {
        if let Some(value) = value {
            args.extend([flag.to_string(), value.to_string()]);
        }
    }
    if pr.draft {
        args.push("--draft".to_string());
    }
    run_gh_owned(root, &args, None).await
}

/// Where a branch's pull request lives: the seam `work/submit` ensures a card's PR through. GitHub
/// through the brokered `gh` ([`GhHost`]) in production; a test hands in its own host, so the
/// whole finish is exercised without the network.
#[async_trait::async_trait]
pub trait PullRequestHost: Send + Sync {
    /// The open pull request whose head is `branch`.
    async fn open_for(&self, branch: &str) -> Result<Option<OpenPullRequest>, GhRunError>;
    /// Open `pr`; its URL.
    async fn create(&self, pr: NewPullRequest<'_>) -> Result<String, GhRunError>;
}

/// GitHub, through `gh` run in `root` against `repo` (`owner/name`).
pub struct GhHost<'a> {
    pub root: &'a Path,
    pub repo: &'a str,
}

#[async_trait::async_trait]
impl PullRequestHost for GhHost<'_> {
    async fn open_for(&self, branch: &str) -> Result<Option<OpenPullRequest>, GhRunError> {
        open_pull_request_for(self.root, Some(self.repo), branch).await
    }

    async fn create(&self, pr: NewPullRequest<'_>) -> Result<String, GhRunError> {
        create_pull_request(self.root, NewPullRequest { repo: Some(self.repo), ..pr }).await
    }
}

/// GitHub as a list in memory: the host seam's one test adapter (every crate's tests that ensure
/// a PR use it), opening PRs numbered from 101 and counting what it opened.
#[cfg(any(test, feature = "test-fixtures"))]
#[derive(Default)]
pub struct InMemoryPullRequestHost {
    open: std::sync::Mutex<Vec<OpenPullRequest>>,
    opened: std::sync::atomic::AtomicU64,
}

#[cfg(any(test, feature = "test-fixtures"))]
impl InMemoryPullRequestHost {
    /// How many PRs this host has opened.
    pub fn opened(&self) -> u64 {
        self.opened.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[cfg(any(test, feature = "test-fixtures"))]
#[async_trait::async_trait]
impl PullRequestHost for InMemoryPullRequestHost {
    async fn open_for(&self, branch: &str) -> Result<Option<OpenPullRequest>, GhRunError> {
        let open = self.open.lock().map_err(|e| GhRunError::Spawn(e.to_string()))?;
        Ok(open.iter().find(|pr| pr.head == branch).cloned())
    }

    async fn create(&self, pr: NewPullRequest<'_>) -> Result<String, GhRunError> {
        let number = 101 + self.opened.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let url = format!("https://github.com/in-memory/pull/{number}");
        let head = pr.head.ok_or_else(|| GhRunError::Spawn("the in-memory host opens a PR only for a named head".into()))?;
        self.open.lock().map_err(|e| GhRunError::Spawn(e.to_string()))?.push(OpenPullRequest {
            number,
            url: url.clone(),
            head: head.to_string(),
            base: pr.base.unwrap_or("main").to_string(),
        });
        Ok(url)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: an open PR read as none (a second PR opened for the same branch, the
    // card linked to neither), or GitHub's answer misread: a malformed list is an error, never
    // "no PR yet".
    #[test]
    fn a_listed_pull_request_is_read_whole_or_refused() {
        let one = r#"[{"number":5718,"url":"https://github.com/CambrianTech/continuum/pull/5718","headRefName":"e5c80c7f/her-fix","baseRefName":"canary"}]"#;
        assert_eq!(
            first_listed(one),
            Ok(Some(OpenPullRequest {
                number: 5718,
                url: "https://github.com/CambrianTech/continuum/pull/5718".into(),
                head: "e5c80c7f/her-fix".into(),
                base: "canary".into(),
            }))
        );
        assert_eq!(first_listed("[]\n"), Ok(None));
        assert!(first_listed("").is_err(), "an empty answer is not an empty list");
        assert!(first_listed("5718 https://x").is_err());
    }
}
