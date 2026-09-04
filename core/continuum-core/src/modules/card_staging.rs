//! Card staging — prepare the CLAIMER's workspace for the card she just claimed, per
//! the recipe the card belongs to. The on-claim recipe step.
//!
//! Before this, `benchmark/dispatch` staged every card into a round-robin ASSIGNEE's
//! workspace and PRE-CLAIMED it for her — a push. That is why a shared deck was never
//! shared: a card pulled by anyone else pointed at a checkout in someone else's
//! workspace, so eligibility had to be gated on the assignee, and 7 of 12 residents
//! dreamed while 5 worked (Joel 2026-09-03: "this dispatch issue is a MONTHS OLD bug").
//!
//! Now the CLAIM stages. Whoever pulls a card gets its work prepared in HER workspace,
//! so any resident can work any Open card, and a rebooted citizen who re-reads the
//! board and pulls is staged exactly like a first claimer — resume and dispatch are one
//! motion. Two callers, one seam: `work/claim` (a citizen pulls) and
//! `benchmark/dispatch` (a detached-solve round stages its directed assignee before
//! firing her solve).
//!
//! Generic by construction (Joel 2026-09-03: "is this benchmark recipe, or can/should it
//! be more generic? Should this function be a command — always if it uses heavy
//! resources?"): the entry takes a card TITLE and a claimer and asks the recipe what
//! staging means. A non-recipe card stages as [`Staging::Ordinary`] (hands stay put).
//! Today the recipes are the benchmark specs; a pipeline or design recipe adds its
//! own arm here, never a second staging path. The heavy half (a checkout clone, an
//! env build) already lives behind verbs (`swe_bench::clone_at`, `ensure_env`); this
//! module is their on-claim orchestration, invoked from the `work/claim` command.
//!
//! Measured 2026-09-03 on the M5: a `--shared` clone from the local mirror lands in
//! ~1–2s (django, 293 MB mirror), so the checkout is staged synchronously inside the
//! claim; the env build (uv venv, cached per instance) is spawned behind it and
//! reported through the same `benchmark.env.*` probes the pre-warm uses.

use std::path::{Path, PathBuf};

use uuid::Uuid;

/// What staging produced for this claimer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Staging {
    /// Not a recipe card (or a recipe with no workspace step) — nothing to stage,
    /// her hands stay where they are.
    Ordinary,
    /// The card's work is ready in her workspace at `path` (workspace-relative
    /// coordinates are what the card body already speaks).
    Ready { path: PathBuf },
    /// A recipe card whose staging failed at `stage` — the claim stands, the failure
    /// is reported, and no scored solve fires on an unstaged workspace.
    Failed { stage: &'static str, error: String },
}

/// The recipe-resolved staging step for one card title.
enum Step {
    /// SWE-bench: clone the instance at its base commit, then build its env.
    Swe(Box<crate::cognition::swe_bench::SweInstance>),
    /// Gym: run the task's authored `setup_shell` in the workspace (idempotent by
    /// adapter convention).
    Shell(String),
    /// A recipe card with no workspace preparation.
    Nothing,
}

/// Stage `title`'s work into the workspace of `claimer` under `home`.
/// Stage for a claimed CARD: a benchmark card stages by its title recipe (below);
/// any other card of a repo this node has a checkout of gets airc's per-card
/// worktree (`airc_lib::work_worktree`, #1377 — the same one the CLI gives an
/// agent), so a citizen with no cwd can pull a continuum card and root her hands
/// there. A repo this node never checked out stages as Ordinary, said in a probe.
pub async fn stage_for_card(home: &Path, claimer: Uuid, card: &airc_lib::WorkCard) -> Staging {
    if crate::commands::benchmark::parse_card_title(&card.title).is_some() {
        return stage_for_claimer(home, claimer, &card.title).await;
    }
    let repo = card.repo.to_string();
    if let Some(existing) = airc_lib::work_worktree::worktree_path_for(card.card_id).filter(|p| p.join(".git").exists()) {
        return Staging::Ready { path: existing };
    }
    let Some(clone) = crate::modules::repo_registry::path_for(&repo) else {
        crate::probe!(
            class = "work.claim.repo_unstaged",
            claimer = %claimer,
            repo = %repo,
            "repo card claimed but this node has no recorded checkout of the repo — hands stay home"
        );
        return Staging::Ordinary;
    };
    let short = airc_lib::work_worktree::short_id(card.card_id);
    let slug: String = card
        .title
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .take(6)
        .collect::<Vec<_>>()
        .join("-");
    let branch = format!("{short}/{slug}");
    let started = std::time::Instant::now();
    let spec_card = card.card_id;
    let clone_for_spawn = clone.clone();
    let branch_for_spawn = branch.clone();
    let outcome = tokio::task::spawn_blocking(move || {
        airc_lib::work_worktree::ensure_worktree(&airc_lib::work_worktree::WorktreeSpec {
            card_id: spec_card,
            clone_path: &clone_for_spawn,
            branch: &branch_for_spawn,
            start_point: None,
        })
        .map(|o| o.path().to_path_buf())
    })
    .await
    .unwrap_or_else(|e| Err(format!("worktree task panicked: {e}"))); // unwrap_or_else: a panicked blocking task is a staging failure, never a crash
    let staged = match outcome {
        Ok(path) => Staging::Ready { path },
        Err(error) => Staging::Failed { stage: "worktree", error },
    };
    crate::probe!(
        class = "work.claim.staged",
        claimer = %claimer,
        bench = "repo",
        task = %repo,
        outcome = ?staged,
        ms = started.elapsed().as_millis() as u64,
        "on-claim staging — a repo card gets airc's per-card worktree"
    );
    staged
}

pub async fn stage_for_claimer(home: &Path, claimer: Uuid, title: &str) -> Staging {
    let Some((bench, task)) = crate::commands::benchmark::parse_card_title(title) else {
        return Staging::Ordinary;
    };
    let workspace =
        crate::identity::citizen_peer_dir(home, crate::identity::PeerId::from_uuid(claimer))
            .join("workspace");
    let step = match resolve_step(&bench, &task).await {
        Ok(step) => step,
        Err(error) => return failed(claimer, title, "resolve", error),
    };
    let started = std::time::Instant::now();
    let outcome = match step {
        Step::Nothing => Staging::Ordinary,
        Step::Shell(shell) => stage_shell(&workspace, &shell).await,
        Step::Swe(instance) => stage_swe(&workspace, &instance).await,
    };
    crate::probe!(
        class = "work.claim.staged",
        claimer = %claimer,
        bench = %bench,
        task = %task,
        outcome = ?outcome,
        ms = started.elapsed().as_millis() as u64,
        "on-claim staging — the claimer's workspace prepared per the card's recipe"
    );
    outcome
}

/// Ask the recipe what staging `task` means. Benchmark specs are the recipes today.
async fn resolve_step(bench: &str, task: &str) -> Result<Step, String> {
    let spec = crate::commands::benchmark::known_benchmarks()
        .iter()
        .find(|s| s.name == bench)
        .ok_or_else(|| format!("card names unknown benchmark '{bench}'"))?;
    if let Some(dataset) = spec.swe_dataset() {
        let rows = crate::cognition::swe_bench::load_dataset(dataset).await?;
        let instance = rows
            .into_iter()
            .find(|i| i.instance_id == task)
            .ok_or_else(|| format!("instance '{task}' not in dataset '{dataset}'"))?;
        return Ok(Step::Swe(Box::new(instance)));
    }
    let Some(reference) = spec.eval_set else {
        return Ok(Step::Nothing);
    };
    let (origin, text) = crate::cognition::gym::resolve_gym(reference)?;
    for (n, line) in text.lines().enumerate().filter(|(_, l)| !l.trim().is_empty()) {
        let t: crate::cognition::eval::EvalTask = serde_json::from_str(line.trim())
            .map_err(|e| format!("{origin} line {}: malformed EvalTask: {e}", n + 1))?;
        if t.id == task {
            return Ok(t.setup_shell.map(Step::Shell).unwrap_or(Step::Nothing));  // unwrap_or: a task without setup_shell stages nothing
        }
    }
    Err(format!("task '{task}' not in gym '{reference}'"))
}

async fn stage_shell(workspace: &Path, shell: &str) -> Staging {
    if let Err(e) = std::fs::create_dir_all(workspace) {
        return Staging::Failed { stage: "workspace", error: e.to_string() };
    }
    match tokio::process::Command::new("sh")
        .arg("-c")
        .arg(shell)
        .current_dir(workspace)
        .output()
        .await
    {
        Ok(out) if out.status.success() => Staging::Ready { path: workspace.to_path_buf() },
        Ok(out) => Staging::Failed {
            stage: "setup_shell",
            error: String::from_utf8_lossy(&out.stderr).trim().to_string(),
        },
        Err(e) => Staging::Failed { stage: "setup_shell", error: e.to_string() },
    }
}

async fn stage_swe(workspace: &Path, instance: &crate::cognition::swe_bench::SweInstance) -> Staging {
    use crate::cognition::swe_bench;
    let dir = workspace.join("swe").join(&instance.instance_id);
    if dir.join(".git").exists() {
        // Already staged (a prior claim, or a re-claim after a reboot). Self-heal
        // pre-shield checkouts so substrate artifacts never enter her patch.
        swe_bench::shield_workspace_excludes(&dir);
    } else if let Err(error) = swe_bench::clone_at(instance, &dir).await {
        return Staging::Failed { stage: "checkout", error };
    }
    // The env (pytest + the repo, per-instance uv venv) builds BEHIND the claim: cached
    // after the first build, minutes the first time — never on the claim's critical
    // path. Failure marks the instance broken this boot so a scored solve is not
    // fired into a known wall; the card stays claimable once the env heals.
    let inst = instance.clone();
    tokio::spawn(async move {
        match swe_bench::ensure_env(&inst, &dir).await {
            Ok(_) => {
                crate::modules::work::env_broken_this_boot().remove(&inst.instance_id);
                crate::probe!(
                    class = "benchmark.env.prewarmed",
                    instance = %inst.instance_id,
                    "env ready behind the claim"
                );
            }
            Err(e) => {
                crate::modules::work::env_broken_this_boot().insert(inst.instance_id.clone());
                crate::probe!(
                    class = "benchmark.env.prewarm_failed",
                    instance = %inst.instance_id,
                    stage = "env",
                    error = %e,
                    "env build FAILED behind the claim — an ENV failure, never a model result"
                );
            }
        }
    });
    Staging::Ready { path: workspace.join("swe").join(&instance.instance_id) }
}

fn failed(claimer: Uuid, title: &str, stage: &'static str, error: String) -> Staging {
    crate::probe!(
        class = "work.claim.stage_failed",
        claimer = %claimer,
        title = %title,
        stage = stage,
        error = %error,
        "on-claim staging failed — the claim stands, no scored solve fires on an unstaged workspace"
    );
    Staging::Failed { stage, error }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the dispatch→pull inversion at its seam. A card claimed by a
    // citizen who was NEVER its assignee must be staged into HER workspace (keyed by her
    // peer id), not into the round-robin assignee's — the coupling that made free pull
    // impossible. Uses a gym-shaped shell step so no dataset or mirror is needed.
    #[tokio::test]
    async fn a_claimer_who_was_never_the_assignee_is_staged_in_her_own_workspace() {
        let home = tempfile::tempdir().unwrap();  // test: temp dir creation
        let claimer = Uuid::new_v4();
        let workspace = crate::identity::citizen_peer_dir(
            home.path(),
            crate::identity::PeerId::from_uuid(claimer),
        )
        .join("workspace");
        let staged = stage_shell(&workspace, "mkdir -p staged/marker && touch staged/marker/ok").await;
        assert_eq!(staged, Staging::Ready { path: workspace.clone() });
        assert!(
            workspace.join("staged/marker/ok").exists(),
            "the setup ran in the CLAIMER's workspace, not an assignee's"
        );
    }

    // what this catches: a non-recipe card is Ordinary — hands stay put, no probe of
    // failure, no dataset touched. A plain work card must never be mistaken for a bench
    // card by a loose title parse.
    #[tokio::test]
    async fn a_plain_work_card_stages_as_ordinary() {
        let home = tempfile::tempdir().unwrap();  // test: temp dir creation
        let staged = stage_for_claimer(home.path(), Uuid::new_v4(), "Fix the login redirect").await;
        assert_eq!(staged, Staging::Ordinary);
    }

    // what this catches: a failing setup step is REPORTED as Failed with its stage
    // named — never swallowed into Ready (which would fire a scored solve on an
    // unstaged workspace) and never Ordinary (which would hide a recipe defect).
    #[tokio::test]
    async fn a_failing_setup_shell_reports_failed_not_ready() {
        let home = tempfile::tempdir().unwrap();  // test: temp dir creation
        let staged = stage_shell(&home.path().join("ws"), "echo boom >&2; exit 3").await;
        assert_eq!(
            staged,
            Staging::Failed { stage: "setup_shell", error: "boom".to_string() }
        );
    }
}
