#![cfg(test)]
//! RESIDENT PARITY: what an agent session does, a resident does, through HER OWN hands: her tool
//! surface as offered, her shell (unconfined, her identity, her brokered credential), her board.
//! Joel, 2026-11-08: "They're supposed to be as capable as you, not disabled"; "It's always you
//! guys designing them as second class." Each case names the capability; a case that cannot be
//! done is a [`Gap`] naming what blocks it, and the suite fails listing every gap at once.
//!
//! The offline cases run in every `cargo test` (CI's affected-crate tests). The cases that need
//! GitHub, the network or installs run with `--features resident-parity-live` on a node that
//! holds a `gh` login: CI has no credential to broker.
//!
//! One limit, named: two test identities share no transcript without a daemon, so the other
//! persona's review in the finish loop is projected through airc's codec (as
//! `training_credit::automatic` does); the merge gate over that board is airc's
//! (`airc-cli` merger `review_gate`), tested there.

use std::path::{Path, PathBuf};

use crate::code::git_actor::GitActor;
use crate::code::shell_session::ShellSession;

/// What an agent session does that a resident must be able to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Capability {
    /// `git commit` from her shell is hers, author and committer, never the node's config.
    ShellCommitsAsHer,
    /// `git fetch && git rebase origin/canary && git push` from her shell.
    ShellFetchRebasePush,
    /// Work in a second repository's checkout (airc beside continuum): stand in it, commit, push.
    SecondRepository,
    /// Plain filesystem use outside her workspace from her shell.
    ShellFilesystem,
    /// The typed verbs an agent uses are in her hands (offered, or one help away).
    TypedVerbsInHands,
    /// Stale branch -> typed rebase -> submit -> PR linked -> another persona passes -> what the
    /// merger requires holds (Review, PR head = reviewed head, an independent pass).
    FinishLoop,
}

/// A capability she lacks, and what blocks it.
#[derive(Debug)]
struct Gap {
    capability: Capability,
    blocked_by: String,
}

type Case = Result<(), String>;

/// Her identity in these cases: a resident's peer, named as her runtime names her.
fn her() -> GitActor {
    GitActor::citizen(uuid::Uuid::from_u128(0xa115_0000_0000_0000_0000_0000_0000_0001), Some("Aris"))
}

/// Run `command` in HER shell exactly as `code/shell` does (her session, her identity, the
/// credential the core brokers), in `cwd`. Ok with stdout on exit 0; Err naming the failure.
async fn in_her_shell(workspace: &Path, cwd: &Path, command: &str) -> Result<String, String> {
    let workspace = workspace.to_path_buf();
    let cwd = cwd.to_path_buf();
    let command = command.to_string();
    let rt = tokio::runtime::Handle::current();
    tokio::task::spawn_blocking(move || {
        let mut shell = ShellSession::new("parity", "aris", &workspace)?;
        shell.cd(&cwd.to_string_lossy())?;
        let as_her = Some((her(), continuum_hands::commands::code::github::credential_source()));
        let done = shell.execute_and_wait_as(&command, None, &rt, as_her)?;
        match done.exit_code {
            Some(0) => Ok(done.stdout.unwrap_or_default()), // unwrap_or_default: a command that printed nothing
            code => Err(format!("`{command}` exited {code:?}: {}", done.stderr.unwrap_or_default())), // unwrap_or_default: no stderr to quote
        }
    })
    .await
    .map_err(|e| format!("her shell task did not finish: {e}"))?
}

/// The operator's identity, set as the repository's own config: a commit that leaks it shows.
async fn as_operator(dir: &Path) -> Case {
    for (key, value) in [("user.name", "Operator"), ("user.email", "operator@example.com")] {
        crate::code::git_bridge::run_git(dir, &["config", key, value]).await?;
    }
    Ok(())
}

/// A bare origin whose `canary` has moved past where her branch was cut, and her checkout of it.
struct StaleBranch {
    _tmp: tempfile::TempDir,
    workspace: PathBuf,
    origin: PathBuf,
    checkout: PathBuf,
}

async fn stale_branch(name: &str) -> Result<StaleBranch, String> {
    let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
    let workspace = tmp.path().join("workspace");
    std::fs::create_dir_all(&workspace).map_err(|e| e.to_string())?;
    let origin = tmp.path().join(format!("{name}.git"));
    let checkout = tmp.path().join(name);
    let fixture = GitActor::citizen(uuid::Uuid::nil(), Some("Upstream"));
    let git = |dir: PathBuf, args: Vec<String>| {
        let fixture = fixture.clone();
        async move {
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            crate::code::git_bridge::run_git_as(&dir, &fixture, &args).await
        }
    };
    let s = |v: &[&str]| v.iter().map(|a| a.to_string()).collect::<Vec<_>>();
    git(tmp.path().to_path_buf(), s(&["init", "-q", "--bare", "-b", "canary", &origin.to_string_lossy()])).await?;
    git(tmp.path().to_path_buf(), s(&["init", "-q", "-b", "canary", &checkout.to_string_lossy()])).await?;
    std::fs::write(checkout.join("lib.rs"), "fn base() {}\n").map_err(|e| e.to_string())?;
    git(checkout.clone(), s(&["add", "lib.rs"])).await?;
    git(checkout.clone(), s(&["commit", "-q", "-m", "base"])).await?;
    git(checkout.clone(), s(&["remote", "add", "origin", &origin.to_string_lossy()])).await?;
    git(checkout.clone(), s(&["push", "-q", "-u", "origin", "canary"])).await?;
    git(checkout.clone(), s(&["checkout", "-q", "-b", "a1150000/her-fix"])).await?;
    std::fs::write(checkout.join("fix.rs"), "fn fix() {}\n").map_err(|e| e.to_string())?;
    git(checkout.clone(), s(&["add", "fix.rs"])).await?;
    git(checkout.clone(), s(&["commit", "-q", "-m", "her fix"])).await?;
    git(checkout.clone(), s(&["push", "-q", "-u", "origin", "a1150000/her-fix"])).await?;
    // canary moves on without her: another clone pushes to it.
    let other = tmp.path().join("other");
    git(tmp.path().to_path_buf(), s(&["clone", "-q", "-b", "canary", &origin.to_string_lossy(), &other.to_string_lossy()])).await?;
    std::fs::write(other.join("lib.rs"), "fn base() {}\nfn moved() {}\n").map_err(|e| e.to_string())?;
    git(other.clone(), s(&["commit", "-q", "-am", "canary moved"])).await?;
    git(other, s(&["push", "-q", "origin", "canary"])).await?;
    as_operator(&checkout).await?;
    Ok(StaleBranch { _tmp: tmp, workspace, origin, checkout })
}

async fn shell_commits_as_her() -> Case {
    let repo = stale_branch("continuum").await?;
    std::fs::write(repo.checkout.join("note.md"), "hers\n").map_err(|e| e.to_string())?;
    in_her_shell(&repo.workspace, &repo.checkout, "git add note.md && git commit -q -m 'her note'").await?;
    let who = in_her_shell(&repo.workspace, &repo.checkout, "git log -1 --format='%an|%ae|%cn|%ce'").await?;
    let her = her();
    let want = format!("{0}|{1}|{0}|{1}", her.name(), her.email());
    (who.trim() == want).then_some(()).ok_or_else(|| format!("her shell's commit reads `{}`, not `{want}`", who.trim()))
}

async fn shell_fetch_rebase_push() -> Case {
    let repo = stale_branch("continuum").await?;
    in_her_shell(&repo.workspace, &repo.checkout, "git fetch -q origin && git rebase -q origin/canary && git push -q --force-with-lease origin HEAD").await?;
    let local = in_her_shell(&repo.workspace, &repo.checkout, "git rev-parse HEAD").await?;
    let remote = crate::code::git_bridge::run_git(&repo.origin, &["rev-parse", "refs/heads/a1150000/her-fix"]).await?;
    let on_canary = crate::code::git_bridge::run_git(&repo.checkout, &["merge-base", "--is-ancestor", "origin/canary", "HEAD"]).await;
    match (local.trim() == remote.trim(), on_canary) {
        (true, Ok(_)) => Ok(()),
        (false, _) => Err(format!("origin holds {} after her push, not her rebased {}", remote.trim(), local.trim())),
        (true, Err(e)) => Err(format!("her branch is not on top of canary after the rebase: {e}")),
    }
}

async fn second_repository() -> Case {
    // Her workspace is continuum's; the card is airc's: she stands in that checkout and works.
    let continuum = stale_branch("continuum").await?;
    let airc = stale_branch("airc").await?;
    std::fs::write(airc.checkout.join("merger.rs"), "fn closed_unmerged() {}\n").map_err(|e| e.to_string())?;
    in_her_shell(&continuum.workspace, &airc.checkout, "git add merger.rs && git commit -q -m 'airc: closed unmerged' && git push -q origin HEAD").await?;
    let remote = crate::code::git_bridge::run_git(&airc.origin, &["log", "-1", "--format=%s|%ce", "refs/heads/a1150000/her-fix"]).await?;
    let want = format!("airc: closed unmerged|{}", her().email());
    (remote.trim() == want).then_some(()).ok_or_else(|| format!("airc's origin reads `{}`, not `{want}`", remote.trim()))
}

async fn shell_filesystem() -> Case {
    let repo = stale_branch("continuum").await?;
    let outside = tempfile::tempdir().map_err(|e| e.to_string())?;
    let file = outside.path().join("scratch.txt");
    let wrote = in_her_shell(&repo.workspace, outside.path(), &format!("echo outside > '{}' && cat '{}'", file.display(), file.display())).await?;
    (wrote.trim() == "outside").then_some(()).ok_or_else(|| format!("her shell read back `{}` from outside her workspace", wrote.trim()))
}

fn typed_verbs_in_hands() -> Case {
    use crate::sdk_codegen::ActionCommand;
    use continuum_hands::commands::code::{git, github};
    let verbs: [(&str, continuum_base::tool_disclosure::Disclosure); 10] = [
        (git::add::CodeGitAdd::NAME, git::add::CodeGitAdd::DISCLOSURE),
        (git::commit::CodeGitCommit::NAME, git::commit::CodeGitCommit::DISCLOSURE),
        (git::fetch::CodeGitFetch::NAME, git::fetch::CodeGitFetch::DISCLOSURE),
        (git::rebase::CodeGitRebase::NAME, git::rebase::CodeGitRebase::DISCLOSURE),
        (git::push::CodeGitPush::NAME, git::push::CodeGitPush::DISCLOSURE),
        (github::pr_create::CodeGithubPrCreate::NAME, github::pr_create::CodeGithubPrCreate::DISCLOSURE),
        (github::pr_view::CodeGithubPrView::NAME, github::pr_view::CodeGithubPrView::DISCLOSURE),
        (github::pr_comment::CodeGithubPrComment::NAME, github::pr_comment::CodeGithubPrComment::DISCLOSURE),
        (crate::modules::work::submission::WorkSubmit::NAME, crate::modules::work::submission::WorkSubmit::DISCLOSURE),
        (crate::modules::work::submission::WorkReview::NAME, crate::modules::work::submission::WorkReview::DISCLOSURE),
    ];
    let missing: Vec<&str> = verbs.iter().filter(|(_, d)| !d.in_hands()).map(|(name, _)| *name).collect();
    missing.is_empty().then_some(()).ok_or_else(|| format!("not in her hands: {}", missing.join(", ")))
}

async fn finish_loop() -> Case {
    use continuum_hands::commands::code::git::rebase::{rebase, GitRebaseParams, GitRebaseResult};
    use continuum_hands::commands::code::github::branch_pull_request::InMemoryPullRequestHost;
    let repo = stale_branch("continuum").await?;
    let her = her();
    crate::code::git_bridge::run_git(&repo.checkout, &["fetch", "-q", "origin"]).await?;
    let head = match rebase(&repo.checkout, &her, &GitRebaseParams::Start { onto: "origin/canary".into() }).await.map_err(|e| e.to_string())? {
        GitRebaseResult::Rebased { head } => head,
        other => return Err(format!("her typed rebase onto origin/canary answered {other:?}")),
    };
    let head = airc_work::GitObjectId::new(head).map_err(|e| e.to_string())?;
    let home = tempfile::tempdir().map_err(|e| e.to_string())?;
    let _home = crate::paths::test_home::HomeGuard::set(home.path()).await;
    let airc = airc_lib::Airc::open_with_wire_root_for_test(home.path(), home.path()).await.map_err(|e| e.to_string())?;
    let room = airc.join("parity").await.map_err(|e| e.to_string())?;
    let repo_id = airc_work::RepoId::new("CambrianTech/continuum").map_err(|e| e.to_string())?;
    let card_id = airc
        .create_work_card(airc_lib::CreateWorkCard::new(repo_id.clone(), "her fix", airc_lib::Priority::P1))
        .await
        .map_err(|e| e.to_string())?;
    let claim = airc.claim_work_card(airc_lib::ClaimWorkCard { card_id, ttl_ms: 600_000 }).await.map_err(|e| e.to_string())?;
    let patch = crate::code::git_bridge::run_git(&repo.checkout, &["diff", "origin/canary", "HEAD"]).await?;
    let artifact = {
        use airc_blobs::ContentAddressedStore;
        airc_blobs::MediaRef {
            hash: crate::media::artifact::store()?.put(patch.as_bytes()).map_err(|e| e.to_string())?,
            size_bytes: patch.len() as u64,
            mime: Some("text/x-diff".into()),
        }
    };
    let base = crate::code::git_bridge::run_git(&repo.checkout, &["rev-parse", "origin/canary"]).await?;
    let submitted = airc
        .submit_work_in(&room, airc_lib::SubmitWork {
            submission_id: airc_work::SubmissionId::new(),
            card_id,
            claim_id: claim,
            instance: "her-fix".into(),
            base_sha: airc_work::GitObjectId::new(base.trim()).map_err(|e| e.to_string())?,
            head: head.clone(),
            artifact,
        })
        .await
        .map_err(|e| e.to_string())?;
    let card = airc.work_board_in(&room).await.map_err(|e| e.to_string())?.card(card_id).cloned().ok_or("card left the board")?;
    let host = InMemoryPullRequestHost::default();
    let text = crate::modules::work::submission::card_pull_request::PullRequestText { title: "her fix", body: "card, submission, patch".into() };
    let ensured = crate::modules::work::submission::card_pull_request::ensure_card_pull_request(&airc, &room, &card, &repo.checkout, &head, &host, text)
        .await
        .map_err(|e| e.to_string())?;
    // Another persona reviews and passes, projected over the same events her board holds. Her
    // evidence text is what its hash names, as airc's review validation requires.
    const FINDING: &str = "ran the tests on the rebased head";
    let reviewer = airc_core::PeerId::new();
    let board = airc.work_board_in(&room).await.map_err(|e| e.to_string())?;
    let linked = board.card(card_id).cloned().ok_or("card left the board")?;
    let submission = linked.latest_submission().cloned().ok_or("her submission is not on the card")?;
    let review_card = airc_work::submission_review_card_id(card_id, submission.submission_id);
    let review_claim = airc_work::ClaimId::new();
    let at = submission.submitted_at_ms;
    let owner = airc.peer_id();
    let created = |id, reviews, by| airc_work::WorkEvent::CardCreated(airc_work::CardCreated {
        card_id: id,
        repo: repo_id.clone(),
        title: "her fix".into(),
        body: None,
        priority: airc_work::Priority::P1,
        lane_id: None,
        created_by: by,
        created_at_ms: at.saturating_sub(2),
        reviews,
        origin: None,
    });
    let claimed = |id, claim_id, holder| airc_work::WorkEvent::CardClaimed(airc_work::WorkCardClaimed {
        selected_at_ms: None,
        origin: airc_work::ClaimOrigin::Unknown,
        card_id: id,
        claim_id,
        owner: holder,
        ttl_ms: 600_000,
        claimed_at_ms: at.saturating_sub(1),
    });
    let pass = airc_work::WorkSubmissionReview {
        review_id: airc_work::WorkReviewId::new(),
        card_id,
        submission_id: submission.submission_id,
        artifact: submission.artifact.clone(),
        review_card_id: review_card,
        review_claim_id: review_claim,
        reviewer,
        outcome: airc_work::WorkReviewOutcome::Passed,
        evidence: airc_work::SubmissionArtifact { hash: airc_blobs::ContentHash::from_bytes(FINDING.as_bytes()), size_bytes: FINDING.len() as u64, mime: Some("text/plain".into()) },
        evidence_text: Some(FINDING.into()),
        reviewed_at_ms: at + 2,
    };
    let linked_event = airc_work::WorkEvent::PullRequestLinked(airc_work::PullRequestLinked {
        card_id,
        pull_request: ensured.pull_request.clone(),
        linked_by: owner,
        linked_at_ms: at + 1,
    });
    let projected = super::training_credit::project_events(
        room.channel.as_uuid(),
        vec![
            (created(card_id, None, owner), owner),
            (claimed(card_id, claim, owner), owner),
            (airc_work::WorkEvent::WorkSubmitted(submission.clone()), owner),
            (linked_event, owner),
            (created(review_card, Some(card_id), reviewer), reviewer),
            (claimed(review_card, review_claim, reviewer), reviewer),
            (airc_work::WorkEvent::WorkSubmissionReviewed(pass), reviewer),
        ],
    );
    let card = projected.card(card_id).ok_or("card left the projected board")?;
    let pr_head = crate::code::git_bridge::run_git(&repo.origin, &["rev-parse", &format!("refs/heads/{}", ensured.pull_request.head)]).await?;
    let independent_pass = projected
        .submission_reviews_for(submission.submission_id)
        .any(|r| r.reviewer != submission.publisher && r.outcome == airc_work::WorkReviewOutcome::Passed);
    let mut unmet = Vec::new();
    if card.state != airc_work::CardState::Review {
        unmet.push(format!("the card is {:?}, not Review", card.state));
    }
    if card.pull_request.as_ref() != Some(&ensured.pull_request) {
        unmet.push("the card does not link her PR".to_string());
    }
    if submission.head.commit() != Some(&head) || pr_head.trim() != head.as_str() {
        unmet.push(format!("reviewed head {:?}, PR head {}, her rebased head {head}", submission.head.commit(), pr_head.trim()));
    }
    if !independent_pass {
        unmet.push("no independent passing review on her latest submission".to_string());
    }
    unmet.is_empty().then_some(()).ok_or_else(|| format!("the merger could not land it: {}", unmet.join("; ")))
}

// what this catches: a resident who cannot do what an agent session does through her own hands
// (Joel: "as capable as you, not disabled"). Every capability is tried; the failure lists each
// one she lacks and what blocks it, so no gap hides behind the first.
#[tokio::test]
async fn a_resident_can_do_what_an_agent_session_does() {
    // the running core installs the disk ownership registry at boot; her shell's cargo-cache
    // guard reads it, so the test installs the same writer fixture first
    crate::system_resources::disk_reporters::prepare_writer_fixture().await;
    let mut gaps = Vec::new();
    let mut record = |capability, case: Case| {
        if let Err(blocked_by) = case {
            gaps.push(Gap { capability, blocked_by });
        }
    };
    record(Capability::ShellCommitsAsHer, shell_commits_as_her().await);
    record(Capability::ShellFetchRebasePush, shell_fetch_rebase_push().await);
    record(Capability::SecondRepository, second_repository().await);
    record(Capability::ShellFilesystem, shell_filesystem().await);
    record(Capability::TypedVerbsInHands, typed_verbs_in_hands());
    record(Capability::FinishLoop, finish_loop().await);
    assert!(gaps.is_empty(), "a resident lacks what an agent has:\n{gaps:#?}");
}

/// The cases that need GitHub, the network or installs: a node with a `gh` login runs them.
#[cfg(feature = "resident-parity-live")]
mod live {
    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum LiveCapability {
        ShellGithubCli,
        ShellGitOverHttps,
        ShellNetwork,
        ShellToolchains,
        ShellInstall,
    }

    // what this catches: her shell's `gh`, git over HTTPS, network and installs failing where an
    // agent's work: a missing brokered credential, a denied network, a PATH without the agent's
    // toolchains. Each failure names the blocker.
    #[tokio::test]
    async fn her_shell_reaches_github_the_network_and_her_toolchains() {
        let workspace = tempfile::tempdir().expect("workspace");
        let install_root = tempfile::tempdir().expect("install root");
        let cases = [
            (LiveCapability::ShellGithubCli, "gh pr view 1 --repo CambrianTech/continuum --json state".to_string()),
            (LiveCapability::ShellGitOverHttps, "git ls-remote https://github.com/CambrianTech/airc.git HEAD".to_string()),
            (LiveCapability::ShellNetwork, "curl -fsSI https://github.com".to_string()),
            (LiveCapability::ShellToolchains, "cargo install --list && brew list --formula && npm view left-pad version".to_string()),
            (LiveCapability::ShellInstall, format!("cargo install --locked ripgrep --root '{}'", install_root.path().display())),
        ];
        let mut gaps = Vec::new();
        for (capability, command) in cases {
            if let Err(blocked_by) = in_her_shell(workspace.path(), workspace.path(), &command).await {
                gaps.push((capability, blocked_by));
            }
        }
        assert!(gaps.is_empty(), "her shell lacks what an agent's has:\n{gaps:#?}");
    }
}
