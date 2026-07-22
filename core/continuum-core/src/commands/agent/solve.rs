//! `agent/solve` — the headless-agent keystone. Drop a persona's WHOLE self into a working
//! directory, hand her one task, let her work it with her hands, and return the patch.
//!
//! This is the primitive every external benchmark harness composes on
//! ([[terminal-bench-is-the-meta-harness-build-one-agent-adapter]],
//! `docs/architecture/BENCHMARK-HARNESS-INTEGRATION.md`): Terminal-Bench, SWE-bench, Aider
//! Polyglot all reduce to "put an agent in a sandbox cwd with a task, collect the diff, grade
//! with our own tests." A benchmark being "LLM-shaped" is an ergonomics problem solved by the
//! RAG (the task is layered into her situation) — NEVER by stripping her
//! ([[benchmark-must-never-score-persona-against-a-soul-stripped-copy]]).
//!
//! She competes WHOLE: `with_tools: true` (hands on), `suppress_recall: false` (memory/RAG on),
//! her genome paged in on the measurement lane. Same drive machinery as `cognition/eval`
//! (`fork_eval_cycle_with_adapter` → `drive_to_settle`), minus the grader — the external harness
//! grades. One drive, two consumers (eval scores; agent/solve emits a patch).

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use ts_rs::TS;
use uuid::Uuid;

use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx};

/// Max act→observe cycles she may take on one task before it counts as unfinished. Agentic
/// SWE tasks (read → edit → compile → fix) need several; default generously.
const DEFAULT_MAX_ACTS: u32 = 12;
/// How long to wait for the forked cognition template (post-spawn `register_from_cfg` race).
const FORK_WAIT_TRIES: u32 = 20;

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../protocol/typescript/agent/AgentSolveParams.ts")]
pub struct AgentSolveParams {
    /// The persona (UUID, spawned) whose FULL cognition works the task.
    pub persona_id: String,
    /// The model to measure her on — forged into a dedicated measurement lane (her genome pages
    /// in on top). A loadable id from `ai/inference/models`.
    pub base_model_id: String,
    /// The task instruction, verbatim (layered into her situation/RAG).
    pub task: String,
    /// The working directory she acts in — her tools (`code/write`, `code/shell`, …) are rooted
    /// here. Ideally a git repo so the result is a clean diff; a bare dir also works.
    pub workspace: String,
    /// Max act→observe cycles (default 12).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub max_acts: Option<u32>,
}

#[derive(Debug, Clone, Serialize, TS, JsonSchema)]
#[ts(export, export_to = "../../protocol/typescript/agent/AgentSolveResult.ts")]
pub struct AgentSolveResult {
    pub persona_id: String,
    pub model: String,
    /// How many times she acted (edited / ran / read) before settling.
    #[ts(type = "number")]
    pub acts: u32,
    /// Her final spoken answer (the "mouth" — some benchmarks grade this; SWE grades the patch).
    pub spoken: String,
    /// Unified `git diff` of everything she changed in the workspace — the "hands" artifact the
    /// SWE/Terminal-Bench harness applies + tests. Empty if she made no file changes (or the
    /// workspace is not a git repo).
    pub patch: String,
    /// Paths she touched (from `git diff --name-only`).
    pub files_changed: Vec<String>,
}

/// `agent/solve` — headless single-task agent run. Pure orchestration over the eval drive
/// machinery; never hand-spawns a llama-server (the serving system owns lifecycle) — it stands up
/// a measurement lane the same way `cognition/eval` does, holds it for the drive, drops it after.
#[derive(Default)]
pub struct AgentSolve;

#[async_trait]
impl ActionCommand for AgentSolve {
    const NAME: &'static str = "agent/solve";
    const ACCESS: AccessLevel = AccessLevel::Privileged;
    const DESCRIPTION: &'static str =
        "Headless agent: drop a persona's WHOLE self (genome, memory, RAG, tools, act→verify loop) \
         into a working directory, hand her one task, and return the git patch she produced + her \
         answer + act count. The primitive every external benchmark harness (Terminal-Bench, \
         SWE-bench, Aider Polyglot) composes on. She is never stripped to fit a benchmark.";
    type Params = AgentSolveParams;
    type Output = AgentSolveResult;

    async fn run(&self, _ctx: &Ctx, p: AgentSolveParams) -> Result<AgentSolveResult, CommandError> {
        let persona_uuid = Uuid::parse_str(p.persona_id.trim())
            .map_err(|_| CommandError::Invalid(format!("persona_id '{}' is not a UUID", p.persona_id)))?;
        let workspace = p.workspace.trim().to_string();
        if !std::path::Path::new(&workspace).is_dir() {
            return Err(CommandError::Invalid(format!(
                "workspace '{workspace}' does not exist or is not a directory"
            )));
        }
        let max_acts = p.max_acts.unwrap_or(DEFAULT_MAX_ACTS).max(1) as usize;

        // 1) Stand up a dedicated measurement lane for the model (her genome pages in on top),
        //    exactly as cognition/eval does — held for the whole drive, dropped after.
        let lane = crate::cognition::eval::spawn_base_eval_lane(&p.base_model_id).await?;

        // 2) Fork her WHOLE cognition onto that lane, rooted at the workspace: tools ON, recall ON.
        //    A brief wait covers the post-spawn template race (same as the eval fork-waiter).
        let registry = crate::cognition::persona_workspace::global();
        let mut cycle = None;
        for attempt in 0..FORK_WAIT_TRIES {
            cycle = registry.fork_eval_cycle_with_adapter(
                &persona_uuid,
                lane.adapter.clone(),
                lane.served_ctx,
                true,             // with_tools — her hands are ON
                Some(&workspace), // roots the ToolExecutor at the sandbox cwd
                false,            // suppress_recall = false — her memory/RAG is ON
            );
            if cycle.is_some() {
                break;
            }
            if attempt + 1 < FORK_WAIT_TRIES {
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
        let cycle = cycle.ok_or_else(|| {
            CommandError::NotFound(format!(
                "no workspace template for persona {persona_uuid} — spawn her (register_from_cfg) \
                 before agent/solve can fork a measurement copy of her mind"
            ))
        })?;

        // 3) Layer the task into her situation as a directed request (same framing eval uses so
        //    a coder model engages rather than taking the silent PASS hatch), and DRIVE her to
        //    settlement — read → edit → compile → fix, her real act→observe loop.
        let room = Uuid::nil();
        let framed = format!(
            "This is a task for you to complete now. Provide your complete solution:\n\n{}",
            p.task.trim()
        );
        let task_delivery = crate::persona::rag_budget::RagDelivery {
            source_id: "airc".to_string(),
            items: vec![crate::persona::rag_budget::RagItem {
                content: framed,
                tokens: 0,
                metadata: serde_json::json!({
                    "peer_id": "peer",
                    "occurred_at_ms": crate::persona::trace::now_ms(),
                }),
            }],
            tokens_used: 0,
            continuation: None,
            resolution_used: crate::persona::rag_budget::ResolutionPreference::Raw,
        };
        let burst = crate::cognition::workspace::Burst::from_turns(
            room,
            crate::persona::service_loop::build_workspace_turns(
                std::slice::from_ref(&task_delivery),
                "",
                "",
                None,
            ),
        );
        let settled = crate::cognition::act_observe::drive_to_settle(
            &cycle,
            burst,
            room,
            max_acts,
            crate::cognition::workspace::TurnFraming::directed(),
        )
        .await;

        // 4) Collect the HANDS artifact: everything she changed in the workspace as a unified diff
        //    (new files included), plus the touched paths. This is what SWE/Terminal-Bench apply.
        let (patch, files_changed) = workspace_patch(&workspace).await;

        // Lane drops here (end of scope) — measurement copy torn down, living personas untouched.
        drop(lane);

        Ok(AgentSolveResult {
            persona_id: p.persona_id,
            model: p.base_model_id,
            acts: settled.acts as u32,
            spoken: settled.spoken.unwrap_or_default(),
            patch,
            files_changed,
        })
    }
}

/// Unified diff of every change in the workspace (tracked edits + new files), and the touched
/// paths. `git add -N` stages new files as intent-to-add so `git diff` includes them without
/// committing content; non-repo or git-less environments return empty (honest — no hands artifact).
async fn workspace_patch(workspace: &str) -> (String, Vec<String>) {
    let git = |args: &[&str]| {
        let mut c = tokio::process::Command::new("git");
        c.arg("-C").arg(workspace).args(args);
        c
    };
    // Non-fatal: a bare (non-git) workspace just yields no patch.
    let _ = git(&["add", "-A", "-N"]).output().await;
    let diff = git(&["diff"]).output().await.ok();
    let names = git(&["diff", "--name-only"]).output().await.ok();
    let patch = diff
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    let files_changed = names
        .filter(|o| o.status.success())
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .filter(|l| !l.trim().is_empty())
                .map(|l| l.to_string())
                .collect()
        })
        .unwrap_or_default();
    (patch, files_changed)
}

crate::register_stateless_command!(AgentSolve);

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::process::Command;

    async fn git(dir: &std::path::Path, args: &[&str]) {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .await
            .expect("git runs");
        assert!(out.status.success(), "git {:?} failed: {}", args, String::from_utf8_lossy(&out.stderr));
    }

    // what this catches: the patch is the benchmark's HANDS artifact — the SWE/Terminal-Bench
    // harness applies it and runs the repo's tests. If workspace_patch dropped NEW files (the
    // common case for a from-scratch task) it would silently score 0 on every such task while
    // looking like the agent "did nothing". This pins that `git add -A -N` + `git diff` includes
    // both an edit to a tracked file AND a brand-new untracked file.
    #[tokio::test]
    async fn workspace_patch_includes_edits_and_new_files() {
        let dir = std::env::temp_dir().join(format!("cu-agent-solve-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        git(&dir, &["init", "-q"]).await;
        git(&dir, &["config", "user.email", "t@t"]).await;
        git(&dir, &["config", "user.name", "t"]).await;
        std::fs::write(dir.join("tracked.txt"), "one\n").unwrap();
        git(&dir, &["add", "-A"]).await;
        git(&dir, &["commit", "-qm", "base"]).await;

        // The agent's "hands": edit a tracked file + create a new one.
        std::fs::write(dir.join("tracked.txt"), "one\ntwo\n").unwrap();
        std::fs::write(dir.join("brand_new.rs"), "fn main() {}\n").unwrap();

        let (patch, files) = workspace_patch(dir.to_str().unwrap()).await;
        assert!(patch.contains("tracked.txt"), "edit missing from patch:\n{patch}");
        assert!(patch.contains("brand_new.rs"), "NEW file missing from patch:\n{patch}");
        assert!(patch.contains("+two"), "edit content missing:\n{patch}");
        assert!(patch.contains("fn main()"), "new-file content missing:\n{patch}");
        assert!(files.iter().any(|f| f == "tracked.txt"));
        assert!(files.iter().any(|f| f == "brand_new.rs"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    // what this catches: a non-git workspace must degrade to an empty patch, never panic — a
    // bare sandbox dir (some benchmarks hand the agent a plain cwd) is a legitimate input.
    #[tokio::test]
    async fn workspace_patch_is_empty_on_non_git_dir() {
        let dir = std::env::temp_dir().join(format!("cu-agent-solve-bare-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("x.txt"), "hi\n").unwrap();
        let (patch, files) = workspace_patch(dir.to_str().unwrap()).await;
        assert!(patch.is_empty(), "bare dir should yield no patch, got:\n{patch}");
        assert!(files.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    // what this catches: the wire name must mirror the file path (commands/agent/solve.rs ⟺
    // "agent/solve") and stay Privileged — it drives arbitrary shell + writes files in the cwd,
    // the same authority boundary as agent/start. Drift silently breaks routing or widens the ACL.
    #[test]
    fn name_and_access_hold_the_contract() {
        assert_eq!(AgentSolve::NAME, "agent/solve");
        assert!(matches!(AgentSolve::ACCESS, AccessLevel::Privileged));
    }
}
