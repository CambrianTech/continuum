//! `code/github/pr-create` — open a GitHub pull request from the caller's pushed branch.

use std::path::Path;
use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{run_gh, workspace_root_for};
use crate::modules::code::CodeState;
use crate::sdk_codegen::CommandError;

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/code/GithubPrCreateParams.ts")]
pub struct GithubPrCreateParams {
    /// The PR title.
    pub title: String,
    /// The PR body / description (markdown). Say WHAT changed and WHY, like a teammate would.
    pub body: String,
    /// Base branch to merge INTO. Omit for the repo's default branch (usually `main`).
    #[serde(default)]
    #[ts(optional)]
    pub base: Option<String>,
    /// Head branch carrying your changes. Omit for the current branch — push it first with
    /// `code/git/push`.
    #[serde(default)]
    #[ts(optional)]
    pub head: Option<String>,
    /// Open as a DRAFT PR (work-in-progress, not yet ready for review).
    #[serde(default)]
    #[ts(optional)]
    pub draft: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/code/GithubPrCreateResult.ts")]
pub struct GithubPrCreateResult {
    /// The URL of the opened pull request.
    pub url: String,
}

crate::action_command! {
    /// Open a GitHub pull request (`gh pr create`) from your pushed branch. Commit and
    /// `code/git/push` your branch FIRST, then open the PR. Write a real body — what
    /// changed and why — like a teammate. Returns the PR URL.
    pub struct CodeGithubPrCreate { state: Arc<CodeState> }
    name: "code/github/pr-create",
    access: AiSafe,
    native: false, // reachable BY NAME; never pushed into every turn (placeholder-issue spam, 2026-09-03)
    params: GithubPrCreateParams,
    output: GithubPrCreateResult,
    run(this, ctx, p) => {
        super::require_operator(ctx, "code/github/pr-create")?;
        if p.title.trim().is_empty() {
            return Err(CommandError::Invalid("code/github/pr-create: 'title' is required".into()));
        }
        let root = workspace_root_for(&this.state, ctx).await?;
        branch_preflight(
            &root,
            p.base.as_deref().map(str::trim).filter(|s| !s.is_empty()),
            p.head.as_deref().map(str::trim).filter(|s| !s.is_empty()),
        )
        .await?;
        let mut args = vec![
            "pr".to_string(), "create".to_string(),
            "--title".to_string(), p.title,
            "--body".to_string(), p.body,
        ];
        if let Some(base) = p.base.filter(|s| !s.trim().is_empty()) {
            args.push("--base".to_string());
            args.push(base);
        }
        if let Some(head) = p.head.filter(|s| !s.trim().is_empty()) {
            args.push("--head".to_string());
            args.push(head);
        }
        if p.draft.unwrap_or(false) { // absent draft flag means a normal PR; boolean option, not a measurement
            args.push("--draft".to_string());
        }
        let url = run_gh(root, args).await?;
        Ok(GithubPrCreateResult { url })
    }
}

/// Refuse a PR this tool KNOWS it cannot open, before any `gh` shell-out — naming the verb
/// that fixes it. Two confident refusals: nothing on `<head>` ahead of the base (commit first),
/// and `<head>` not fully pushed to origin (`code/git/push` first). Anything ambiguous — fork
/// heads, no local view of the default branch — defers to `gh`, which still fails loud with its
/// own message: preflight may add refusals, never new failure modes. A confused retry is a tool
/// defect, not the citizen's slip.
async fn branch_preflight(root: &Path, base: Option<&str>, head_param: Option<&str>) -> Result<(), CommandError> {
    // Fork heads (`owner:branch`) live on someone else's origin and have no local remote-tracking
    // ref to check — gh's territory. A SLASH in the head is NOT fork notation: `feature/foo` is an
    // ordinary local branch and gets the full preflight (the early slash bypass let unpushed ones through).
    if head_param.is_some_and(|h| h.contains(':')) {
        return Ok(());
    }

    let head = match head_param {
        Some(h) => h.to_string(),
        None => super::git_quiet(root.to_path_buf(), vec!["rev-parse".into(), "--abbrev-ref".into(), "HEAD".into()])
            .await
            .ok_or_else(|| CommandError::Invalid(
                "code/github/pr-create: cannot determine the current branch — is this a git repo with commits?".into()))?,
    };
    if head == "HEAD" {
        return Err(CommandError::Invalid(
            "code/github/pr-create: HEAD is detached — check out a branch (git switch -c <name>), then code/git/push it first".into()));
    }

    // Ahead of base? Resolve the base ref locally; unknown → defer to gh's own error.
    let base_ref = match base {
        Some(b) => Some(format!("origin/{b}")),
        None => super::git_quiet(root.to_path_buf(), vec!["symbolic-ref".into(), "refs/remotes/origin/HEAD".into()])
            .await
            .filter(|s| s.starts_with("refs/remotes/origin/")), // e.g. refs/remotes/origin/main
    };
    if let Some(base_ref) = base_ref {
        match super::git_quiet(root.to_path_buf(), vec!["rev-list".into(), "--count".into(), format!("{base_ref}..{head}")])
            .await
            .as_deref()
            .and_then(|s| s.parse::<u32>().ok())
        {
            Some(0) => return Err(CommandError::Invalid(format!(
                "code/github/pr-create: no commits on {head} ahead of the base branch yet — commit your change, then code/git/push, then pr-create"))),
            _ => {} // unknown or >0 → proceed; gh remains the last word
        }
    }

    // Pushed? Resolve the SELECTED head's local tip — not checkout HEAD: a caller on main
    // can open a PR from an already-pushed `feature`, and comparing origin/feature to
    // main's tip would invent a "run push" refusal for a branch that needs none.
    let head_sha = super::git_quiet(root.to_path_buf(), vec!["rev-parse".into(), "--verify".into(), format!("refs/heads/{head}")])
        .await;
    // Fast path: the remote-tracking ref matches the local tip exactly (no network).
    let tracking = super::git_quiet(root.to_path_buf(), vec!["rev-parse".into(), "--verify".into(), format!("refs/remotes/origin/{head}")])
        .await;
    if let (Some(t), Some(h)) = (&tracking, &head_sha) {
        if t == h {
            return Ok(()); // up to date on origin — done
        }
    }
    // Stale or missing local view: ask origin directly (one lightweight round-trip).
    match remote_head_tip(root, &head).await {
        RemoteHead::Found(sha) => match &head_sha {
            Some(h) if *h == sha => Ok(()), // on origin, fully pushed (the local view was just stale)
            Some(_) => Err(CommandError::Invalid(format!(
                "code/github/pr-create: local {head} has commits beyond what's on origin — run code/git/push first, then pr-create"))),
            None => Ok(()), // no local tip to compare against → defer to gh
        },
        RemoteHead::Absent => Err(CommandError::Invalid(format!(
            "code/github/pr-create: branch {head} is not pushed to origin yet — run code/git/push first, then pr-create"))),
        // Transport/auth failure, or no way to ask origin at all: cannot decide. Defer to
        // gh, which fails loud with the true cause — preflight adds refusals, never misdirects.
        RemoteHead::Unknown => Ok(()),
    }
}

/// The three ways origin can answer "what is your tip for `head`?" and why the split is
/// load-bearing: only `Absent` licenses a preflight refusal. Anything else that isn't
/// `Found` is undecidable — network blip, auth prompt, remote gone — and inventing
/// "not pushed" for it would point at the wrong fix. The decision rule is rc 0 + empty
/// output = absent, which held in every measurement on this host (git 2.54).
enum RemoteHead {
    Found(String), // tip sha on origin, for exactly refs/heads/{head}
    Absent,        // rc 0 + empty output — no such ref on origin
    Unknown,       // could not ask or cannot tell — defer to `gh`
}

/// Ask origin for `head`'s tip in one lightweight round-trip, layered on the shared
/// [`super::git_output`] executor so execution/error policy has a single owner. The query is
/// the EXACT ref (`refs/heads/{head}`) and only that exact ref name licenses `Found`: ls-remote
/// patterns match tails, so a bare head would let `topic/feature` answer for an absent
/// `feature`, misdirecting (or masking) the refusal. rc 0 + empty = absent; non-zero or
/// unparseable = undecidable → defer to `gh`. Missing git → Unknown.
async fn remote_head_tip(root: &Path, head: &str) -> RemoteHead {
    let expected = format!("refs/heads/{head}");
    let out = match super::git_output(
        root.to_path_buf(),
        vec!["ls-remote".into(), "--heads".into(), "origin".into(), expected.clone()],
    )
    .await
    {
        Some(o) => o,
        None => return RemoteHead::Unknown, // no git on this host → defer to `gh`
    };
    match out.status.code() {
        Some(0) => {
            let text = String::from_utf8_lossy(&out.stdout);
            if text.trim().is_empty() {
                return RemoteHead::Absent; // rc 0 + empty output — origin says "no such ref" (measured on this host, git 2.54)
            }
            let mut fields = text.lines().next().unwrap_or_default().split_whitespace();
            let sha = fields.next().unwrap_or_default();
            let refname = fields.next().unwrap_or_default();
            if !sha.is_empty() && refname == expected {
                RemoteHead::Found(sha.to_string())
            } else {
                RemoteHead::Unknown // origin answered, but not for the exact ref asked — undecidable here
            }
        }
        _ => RemoteHead::Unknown, // transport/auth/other → cannot decide; defer to gh
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{ActionCommand, Ctx};

    // what this catches: a blank title is rejected with a typed Invalid BEFORE any `gh`
    // shell-out — never open a titleless PR, never silently default it (the same fail-loud
    // contract as code/git/commit).
    #[tokio::test]
    async fn blank_title_is_rejected() {
        let state = Arc::new(CodeState::new(
            Arc::new(dashmap::DashMap::new()),
            Arc::new(dashmap::DashMap::new()),
            tokio::runtime::Handle::current(),
        ));
        let cmd = CodeGithubPrCreate { state };
        let err = cmd
            .run(&Ctx::default(), GithubPrCreateParams { title: "  ".into(), body: "x".into(), ..Default::default() })
            .await
            .unwrap_err();
        assert!(matches!(err, CommandError::Invalid(_)));
    }

    // ── preflight: a real local repo with a bare `origin`, driven through the same checks the
    // command runs — branch_preflight takes a path, so no CodeState is needed here.
    fn git(dir: &std::path::Path, args: &[&str]) -> String {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .expect("git runs in the test environment");
        assert!(out.status.success(), "git {args:?} failed: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    struct PrFixture {
        work: tempfile::TempDir,
        bare: tempfile::TempDir,
    }

    impl PrFixture {
        fn new() -> Self {
            let bare = tempfile::tempdir().expect("bare tempdir");
            let work = tempfile::tempdir().expect("work tempdir");
            git(work.path(), &["init", "-q"]);
            git(work.path(), &["config", "user.email", "kimi@test.local"]);
            git(work.path(), &["config", "user.name", "Kimi"]);
            git(work.path(), &["symbolic-ref", "HEAD", "refs/heads/main"]); // deterministic branch name
            std::fs::write(work.path().join("a.txt"), "one\n").expect("seed file");
            git(work.path(), &["add", "a.txt"]);
            git(work.path(), &["commit", "-q", "-m", "initial"]);
            git(bare.path(), &["init", "-q", "--bare"]);
            git(work.path(), &["remote", "add", "origin", bare.path().to_str().unwrap()]);
            git(work.path(), &["push", "-q", "-u", "origin", "main"]);
            git(work.path(), &["remote", "set-head", "origin", "main"]); // what a clone would have set
            PrFixture { work, bare }
        }
    }

    #[tokio::test]
    async fn no_ahead_of_base_is_refused_with_the_fixing_verb() {
        let fx = PrFixture::new(); // on main, pushed, nothing ahead
        let err = branch_preflight(fx.work.path(), None, None).await.unwrap_err();
        assert!(matches!(err, CommandError::Invalid(_)), "{err}");
        let msg = err.to_string();
        assert!(msg.contains("no commits"), "{msg}");
        assert!(msg.contains("code/git/push") || msg.contains("commit your change"), "names the next step: {msg}");
    }

    #[tokio::test]
    async fn unpushed_branch_is_refused_naming_code_git_push() {
        let fx = PrFixture::new();
        git(fx.work.path(), &["checkout", "-q", "-b", "feature"]);
        std::fs::write(fx.work.path().join("b.txt"), "two\n").expect("wip file");
        git(fx.work.path(), &["add", "b.txt"]);
        git(fx.work.path(), &["commit", "-q", "-m", "wip"]);
        let err = branch_preflight(fx.work.path(), Some("main"), Some("feature")).await.unwrap_err();
        assert!(matches!(err, CommandError::Invalid(_)), "{err}");
        let msg = err.to_string();
        assert!(msg.contains("code/git/push"), "names the fixing verb: {msg}");
    }

    #[tokio::test]
    async fn pushed_branch_with_ahead_commits_passes() {
        let fx = PrFixture::new();
        git(fx.work.path(), &["checkout", "-q", "-b", "feature"]);
        std::fs::write(fx.work.path().join("b.txt"), "two\n").expect("wip file");
        git(fx.work.path(), &["add", "b.txt"]);
        git(fx.work.path(), &["commit", "-q", "-m", "wip"]);
        git(fx.work.path(), &["push", "-q", "-u", "origin", "feature"]);
        branch_preflight(fx.work.path(), Some("main"), Some("feature")).await.expect("pushed + ahead is fine");
    }

    // what this catches: ONLY the colon is fork notation — `owner:branch` lives on someone
    // else's origin and has no local ref to check, so gh stays the last word there. A SLASH in
    // a head name is ordinary branch naming, not fork syntax (the old early slash bypass let an
    // unpushed `feature/foo` through with no refusal at all).
    #[tokio::test]
    async fn colon_fork_heads_defer_to_gh() {
        let fx = PrFixture::new();
        branch_preflight(fx.work.path(), Some("main"), Some("someone:feature"))
            .await
            .expect("colon fork heads are gh's territory");
    }

    #[tokio::test]
    async fn slash_branch_names_get_the_full_push_check() {
        let fx = PrFixture::new();
        git(fx.work.path(), &["checkout", "-q", "-b", "feature/foo"]);
        std::fs::write(fx.work.path().join("b.txt"), "two\n").expect("wip file");
        git(fx.work.path(), &["add", "b.txt"]);
        git(fx.work.path(), &["commit", "-q", "-m", "wip"]); // ahead of main, NOT pushed
        let err = branch_preflight(fx.work.path(), Some("main"), Some("feature/foo"))
            .await
            .unwrap_err();
        assert!(matches!(err, CommandError::Invalid(_)), "{err}");
        let msg = err.to_string();
        assert!(msg.contains("code/git/push"), "names the fixing verb: {msg}");
    }

    // what this catches: ls-remote patterns match tails — with only `topic/feature` on origin,
    // a bare `feature` query answers with topic/feature's tip and preflight compares against
    // the wrong ref (refusing for the wrong reason, or passing when feature is absent). The
    // exact-ref query + ref-name validation must land in Absent: "not pushed", naming push.
    #[tokio::test]
    async fn unrelated_suffix_branch_does_not_answer_for_the_head() {
        let fx = PrFixture::new();
        git(fx.work.path(), &["checkout", "-q", "-b", "feature"]);
        std::fs::write(fx.work.path().join("b.txt"), "two\n").expect("wip file");
        git(fx.work.path(), &["add", "b.txt"]);
        git(fx.work.path(), &["commit", "-q", "-m", "wip"]); // local feature, ahead of main, unpushed
        git(fx.work.path(), &["push", "-q", "origin", "main:refs/heads/topic/feature"]); // tail-matching ref on origin
        let err = branch_preflight(fx.work.path(), Some("main"), Some("feature"))
            .await
            .unwrap_err();
        assert!(matches!(err, CommandError::Invalid(_)), "{err}");
        let msg = err.to_string();
        assert!(msg.contains("not pushed to origin"), "absent stays absent even with a tail-matching ref: {msg}");
    }

    #[tokio::test]
    async fn detached_head_is_refused_named() {
        let fx = PrFixture::new();
        git(fx.work.path(), &["checkout", "-q", "--detach"]);
        let err = branch_preflight(fx.work.path(), None, None).await.unwrap_err();
        assert!(err.to_string().contains("detached"), "{err}");
    }

    // what this catches: the push check resolves the SELECTED head's tip, not checkout HEAD.
    // On main with `feature` already pushed, the old code compared origin/feature to
    // main's tip and refused "run code/git/push first" for a branch that needed none.
    #[tokio::test]
    async fn selected_head_resolves_the_named_branch_not_HEAD() {
        let fx = PrFixture::new(); // on main, pushed
        git(fx.work.path(), &["checkout", "-q", "-b", "feature"]);
        std::fs::write(fx.work.path().join("b.txt"), "two\n").expect("wip file");
        git(fx.work.path(), &["add", "b.txt"]);
        git(fx.work.path(), &["commit", "-q", "-m", "wip"]);
        git(fx.work.path(), &["push", "-q", "-u", "origin", "feature"]);
        git(fx.work.path(), &["checkout", "-q", "main"]); // HEAD is main; the PR head is feature
        branch_preflight(fx.work.path(), Some("main"), Some("feature"))
            .await
            .expect("a fully-pushed selected head passes even when HEAD sits elsewhere");
    }

    // what this catches: a transport failure asking origin (unreachable remote) is UNKNOWN,
    // not "not pushed" — preflight defers to gh so the true cause surfaces instead of a
    // misdirected fix. Only origin's URL breaks; the fixture's ref state stays real.
    #[tokio::test]
    async fn unreachable_origin_defers_to_gh() {
        let fx = PrFixture::new();
        git(fx.work.path(), &["checkout", "-q", "-b", "feature"]);
        std::fs::write(fx.work.path().join("b.txt"), "two\n").expect("wip file");
        git(fx.work.path(), &["add", "b.txt"]);
        git(fx.work.path(), &["commit", "-q", "-m", "wip"]);
        // Point origin at a path that does not exist: ls-remote then exits non-zero on a
        // transport failure — preflight cannot call that "absent" and defers to gh.
        git(fx.work.path(), &["remote", "set-url", "origin", "/nonexistent/definitely-not-a-repo"]);
        branch_preflight(fx.work.path(), Some("main"), Some("feature"))
            .await
            .expect("an unreachable origin defers to gh instead of inventing 'not pushed'");
    }
}
