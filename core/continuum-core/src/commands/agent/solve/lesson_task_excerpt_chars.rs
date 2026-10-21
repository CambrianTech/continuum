use super::*;

/// How much of the task text the durable lesson may carry. A lesson is a MEMORY OF
/// WORKING, not a copy of the assignment — and the task is caller-supplied text of
/// unbounded size. A SWE-bench problem statement is a full GitHub issue; six solve runs
/// put six of them, verbatim, into Anwen's episodic store, and the consolidator did what
/// it is supposed to do with repeated episodic content: it crystallized SEMANTIC beliefs
/// out of them ("When a Blueprint name in Flask contains a dot, raise ValueError…").
/// She now durably believes things about flask that she learned in an exam room.
///
/// The domain signal the dream's supersession review actually feeds on rides `files_changed`
/// (`mathlib.py` → python), which the lesson keeps in full. The task text is context, and
/// context does not need to be verbatim.
///
/// context-budget-exempt: this is a CONTAMINATION bound, not a context/prompt budget. It caps
/// how much EXAM TEXT may enter her durable memory, and deriving it from the served window
/// would invert the intent — a bigger window would admit MORE of the assignment, which is the
/// exact failure (#312, second vector) this constant exists to stop.
pub(super) const LESSON_TASK_EXCERPT_CHARS: usize = 200;

/// Build the durable EXPERIENCE string for one solve — what she worked on and how,
/// never what she produced (no patch content, no spoken answer: the lesson teaches
/// context, not solutions, so re-runs measure capability rather than memorization).
/// Pure; unit-testable.
pub(super) fn format_solve_lesson(task: &str, acts: usize, files_changed: &[String]) -> String {
    let worked = if files_changed.is_empty() {
        "I changed no files".to_string()
    } else {
        format!("I changed: {}", files_changed.join(", "))
    };
    let task = task.trim();
    let excerpt = match task.char_indices().nth(LESSON_TASK_EXCERPT_CHARS) {
        Some((cut, _)) => format!("{}…", &task[..cut]),
        None => task.to_string(),
    };
    format!(
        "I worked a real coding task in my workspace: {excerpt} — I acted {acts} time(s); {worked}."
    )
}

/// Admit one solve's experience lesson into the LIVING persona (never the fork) —
/// `cognition/eval::transfer_redacted_lessons`' one-way bridge, solve-shaped.
/// Returns 1 if a fresh lesson was admitted (identical re-run lessons dedup
/// idempotently via `admit_reflection`'s content hash), else 0.
pub(super) fn transfer_solve_experience(
    persona_uuid: &Uuid,
    room: crate::identity::ActivityRoom,
    task: &str,
    acts: usize,
    files_changed: &[String],
) -> usize {
    let Some(admission) = crate::cognition::persona_workspace::global()
        .get(persona_uuid)
        .and_then(|cycle| cycle.acting().map(|a| a.admission.clone()))
    else {
        tracing::warn!(
            persona = %persona_uuid,
            "agent/solve learn mode: no live admission — experience not transferred \
             (she was measured, but the living self is not resident to learn)"
        );
        return 0;
    };
    let mut recall_keys = vec!["agent-solve".to_string()];
    recall_keys.extend(files_changed.iter().cloned());
    let engram = crate::persona::engram::Engram {
        id: Uuid::new_v4(),
        context_id: Some(room.as_uuid()),
        kind: crate::persona::engram::EngramKind::Episodic,
        content: format_solve_lesson(task, acts, files_changed),
        origin: crate::persona::engram::EngramOrigin::SelfReflection {
            parent_engram_id: Uuid::nil(),
        },
        recall_keys,
        admitted_at_ms: EpochMs::now().millis(),
        trust_state_at_admission: crate::persona::engram::TrustState::SelfTrust,
        admission_trace_id: None,
    };
    match admission.admit_reflection(engram) {
        Ok(crate::persona::engram::AdmissionDecision::Admit { .. }) => 1,
        _ => 0,
    }
}

/// Unified diff of the SOLUTION changes in the workspace (tracked edits + new files), and the
/// touched paths — build/cache byproducts ([`PATCH_EXCLUDES`]) filtered out so the graded artifact
/// is source-only. `git add -N` stages new files as intent-to-add so `git diff` includes them
/// without committing content; the same excludes keep junk from being intent-added in the first
/// place. Non-repo or git-less environments return empty (honest — no hands artifact).
/// The touched-file → test-file mapping for the in-loop verifier. Deliberately
/// dumb v1: a source file maps to a sibling `tests/test_<stem>.py` (sympy's
/// layout) or a top-level `tests/test_<stem>.py` (flask/requests layout).
/// No mapping found → the verifier skips, loudly. Repo-specific tables can
/// grow here as data when the dumb version's misses are measured.
pub(super) fn mapped_test_files(workspace: &str, files_changed: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for f in files_changed {
        let path = std::path::Path::new(f);
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Some(parent) = path.parent() else {
            continue;
        };
        let candidates = [
            parent.join("tests").join(format!("test_{stem}.py")),
            std::path::PathBuf::from("tests").join(format!("test_{stem}.py")),
        ];
        for cand in candidates {
            if std::path::Path::new(workspace).join(&cand).exists() {
                let c = cand.to_string_lossy().to_string();
                if !out.contains(&c) {
                    out.push(c);
                }
            }
        }
    }
    out
}

/// ONE bounded re-drive with a structural fact injected as the next burst —
/// the shared plumbing of the empty-diff and identical-diff re-drives (the
/// two blocks differ only in trigger and fact text; a second inline copy
/// would drift on the fold rules). Folds the re-drive into the attempt's
/// outcome — totals sum, the final verdict/world-state are the re-drive's
/// (it is the attempt's true end), the spoken text falls back to the first
/// settle's if the re-drive ended un-spoken — and returns the workspace's
/// post-re-drive (patch, files_changed).
pub(super) async fn redrive_with_fact(
    cycle: &crate::cognition::workspace::WorkspaceCycle,
    room: crate::identity::ActivityRoom,
    framing: crate::cognition::workspace::TurnFraming,
    remaining: usize,
    fact: String,
    task: &str,
    settled: &mut crate::cognition::act_observe::SettleOutcome,
    workspace: &str,
) -> (String, Vec<String>) {
    // THE FACT ALONE IS BLIND (glass-boxed 2026-08-26): a resumed attempt's
    // re-drive runs on a fresh fork whose working memory never held the task,
    // and the fact says "the example in the task description" — she answered
    // 'which file?' and settled empty. The re-drive burst restates the WHOLE
    // task beneath the structural fact, so fresh-start and post-interrupt read
    // identically (continuity: one code path).
    let content = format!("{fact}\n\nThe task you are working, restated in full:\n\n{task}");
    let redelivery = crate::persona::rag_budget::RagDelivery {
        source_id: "airc".to_string(),
        items: vec![crate::persona::rag_budget::RagItem {
            content,
            tokens: 0,
            metadata: serde_json::json!({
                "peer_id": "peer",
                "occurred_at_ms": EpochMs::now().millis(),
            }),
        }],
        tokens_used: 0,
        continuation: None,
        resolution_used: crate::persona::rag_budget::ResolutionPreference::Raw,
    };
    let reburst = crate::cognition::workspace::Burst::from_turns(
        room,
        crate::persona::service_loop::build_workspace_turns(
            std::slice::from_ref(&redelivery),
            "",
            "",
            None,
        ),
    );
    let redriven =
        crate::cognition::act_observe::drive_to_settle(cycle, reburst, remaining, framing).await;
    settled.acts += redriven.acts;
    settled.decision = redriven.decision;
    settled.spoken = redriven.spoken.or(settled.spoken.take());
    settled.world_state = redriven.world_state;
    settled.inference_error = redriven.inference_error;
    for path in redriven.touched_paths {
        if !settled.touched_paths.contains(&path) {
            settled.touched_paths.push(path);
        }
    }
    settled.metrics.input_tokens += redriven.metrics.input_tokens;
    settled.metrics.output_tokens += redriven.metrics.output_tokens;
    settled.metrics.latency_ms += redriven.metrics.latency_ms;
    settled.metrics.cached_tokens += redriven.metrics.cached_tokens;
    settled.metrics.prefill_tokens += redriven.metrics.prefill_tokens;
    settled.metrics.prefill_ms += redriven.metrics.prefill_ms;
    settled.metrics.decode_ms += redriven.metrics.decode_ms;
    workspace_patch(workspace).await
}

pub(super) async fn workspace_patch(workspace: &str) -> (String, Vec<String>) {
    let git = |args: &[&str]| {
        let mut c = tokio::process::Command::new("git");
        c.arg("-C").arg(workspace).args(args);
        c
    };
    // Build the `-- . :(exclude…)` pathspec tail shared by add-N and both diffs.
    let mut pathspec: Vec<&str> = vec!["--", "."];
    pathspec.extend_from_slice(PATCH_EXCLUDES);
    let with_paths = |head: &[&str]| -> Vec<String> {
        head.iter()
            .chain(pathspec.iter())
            .map(|s| s.to_string())
            .collect()
    };
    // Non-fatal: a bare (non-git) workspace just yields no patch.
    let _ = git(&with_paths(&["add", "-A", "-N"])
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>())
    .output()
    .await;
    let diff_args = with_paths(&["diff"]);
    let names_args = with_paths(&["diff", "--name-only"]);
    let diff = git(&diff_args.iter().map(String::as_str).collect::<Vec<_>>())
        .output()
        .await
        .ok();
    let names = git(&names_args.iter().map(String::as_str).collect::<Vec<_>>())
        .output()
        .await
        .ok();
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

/// The generic solve CONTRACT wrapped around a task.
///
/// It states HOW acts take effect (only tool calls do; narration does not) and never WHAT the
/// deliverable looks like — the task owns that, and the two used to contradict each other.
///
/// The old text said "writing files with code/write" and "graded on the files your tools WRITE",
/// which is right for a from-scratch build gym. Nested beneath it, the SWE task text says the
/// opposite: "do not add new top-level files … find the existing source of the fault and edit it
/// in place" — that text is the dispatch CARD BODY (`benchmark::BenchmarkSweSetup`), which owns
/// the deliverable shape because the card IS the task.
/// Outer contract first, inner constraint under "Task:" — and she obeyed
/// the outer one. Three consecutive full-effort sympy-21379 runs wrote NEW repro scripts and never
/// edited the library (v3: 1 file, v4: 3 files, v5: 2 files; 0 edits every time). That read as a
/// judgement gap for a whole session; it was two halves of one framing disagreeing.
///
/// FORCE vs SHAPE, learned the hard way. The first attempt at this fix removed the
/// contradiction and the tool-forcing PRESSURE in the same edit — "graded exactly as your
/// tools leave it" is shape-neutral but passive. Measured immediately (v6, same instance,
/// same persona): 8 acts, ZERO files, 0 patch bytes — worse than the three contradictory
/// runs before it, which at least produced repro scripts. She drifted out of task mode
/// entirely and ended the run replying to her OWN `work/list` output as though a peer had
/// posted it in chat.
///
/// So the wording must be BOTH: imperative about the contract, silent about the artifact.
/// "graded ONLY on the CHANGES your tools make" is forceful and neutral — an edit and a new
/// file are both changes; a narration is not.
///
/// Pure so the contract is testable in isolation ([[the-compression-principle]]: one place).
pub(super) fn frame_task(task: &str) -> String {
    format!(
        "This is a task you must COMPLETE NOW by USING YOUR TOOLS in your workspace — editing \
         files with code/edit, writing them with code/write, running commands with code/shell, \
         etc. Only what your tools actually do takes effect: code shown in a message, or a claim \
         that you saved a file, does NOT create or change anything — you are graded ONLY on the \
         CHANGES your tools make to this workspace — an explanation earns nothing. You are ALREADY in the task's workspace: work on the \
         files that are here. Do not create a new workspace or start a new project — grading only \
         sees this one. Follow the task's own instructions about WHAT to change. Do the work with \
         tool calls, then stop.\n\nTask:\n{}",
        task.trim()
    )
}
