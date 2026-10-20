use super::*;

/// Max act→observe cycles she may take on one task before it counts as unfinished. Agentic
/// SWE tasks (read → edit → compile → fix) need several; default generously.
// 32, up from 12 (glass-boxed 2026-08-08, benchy-sympy-22840-n9 attempt 1): the
// 12-act cap — not the clock — was the binding constraint. She was cut off
// mid-recovery (edit failed NOT-FOUND → the error taught verbatim-copy → she was
// re-reading the target region when the budget expired) with 2.5 of the 3
// deadline hours unused. Field SWE agents routinely take 30–80 steps; a count
// cap that binds before the deadline is the hardcoded-LCD-clamp shape the
// cognition pipeline doc forbids. The attempt CEILING stays derived from this budget
// (× ATTEMPT_CEILING_PER_ACT_SECS); the stall verdict itself is advancement-based.
pub(super) const DEFAULT_MAX_ACTS: u32 = 32;

/// A detached attempt is STALLED when its ledger has not advanced (no new act settled)
/// for this long: three act deadlines — an act that the turn driver itself would have
/// surrendered at `TICK_DEADLINE`, twice over, plus one for the settle. Below this a
/// slow lane is slow; past it nothing is moving.
pub(crate) const ACT_STALL_ALLOWANCE: Duration =
    Duration::from_secs(3 * crate::cognition::act_observe::TICK_DEADLINE.as_secs());

/// The far ceiling on one attempt, per act of budget — 45 min/act, over twice the
/// M5's measured median of 21 min under a 16-mind roster (2026-09-16). Named as a
/// ceiling when it fires; it is not the stall verdict.
pub(super) const ATTEMPT_CEILING_PER_ACT_SECS: u64 = 45 * 60;

/// How long to wait for the forked cognition template (post-spawn `register_from_cfg` race).
pub(super) const FORK_WAIT_TRIES: u32 = 20;

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/agent/AgentSolveParams.ts"
)]
pub struct AgentSolveParams {
    /// The persona (UUID, spawned) whose FULL cognition works the task.
    pub persona_id: crate::identity::PersonaRef,
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
    #[ts(optional)]
    #[ts(optional, type = "number")]
    pub max_acts: Option<u32>,
    /// The ROOM this run happens in — `benchmark/dispatch`'s per-run activity room
    /// (#329). Every act she executes radiates a `persona:act` receipt into it, so
    /// the run's work lands in the room's transcript as collapsed receipts (#243)
    /// and anyone standing there — human screen or citizen mind — perceives it
    /// through the ONE ViewState pipe.
    ///
    /// Provided → the run REJOINS that activity (resume and dispatch are the same
    /// motion). Omitted → the run MINTS its own fresh activity room at birth
    /// (probe `agent.solve.room_minted`). THE LAW (Joel, 2026-08-26): an activity
    /// without a room is unrepresentable — the old `Uuid::nil()` invisible-run
    /// mode (13,209 unmeasured roomless turns, #425) no longer exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(optional, type = "string")]
    pub room: Option<Uuid>,
    /// Fire-and-poll (#86): when true, the solve is spawned DETACHED — `run` returns a job
    /// handle NOW (arms empty, `detached: true`) and the REAL result (patch + acts) lands in
    /// `~/.continuum/progress/agent-solve-<run_id>.json`. A real agentic drive (N full-generation
    /// acts) outlives the IPC client timeout — a Devstral write→compile→fix loop took 12 min —
    /// so a Terminal-Bench/SWE-bench harness MUST fire-and-poll, never block on the socket.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub detach: Option<bool>,
    /// Correlation id for a detached run (echoed in the ack + the result file). Omit → minted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub run_id: Option<String>,
    /// LEARN mode (#221 slice 3 — the loop-closer): after the solve, admit the EXPERIENCE
    /// of the work (task, act count, files touched — never the patch content, never any
    /// grader state) into the LIVING persona's memory, exactly as `cognition/eval`'s
    /// learn mode transfers redacted exam lessons. This is what lets work supersede
    /// stale beliefs: a day of python tasks consolidates into python facts, and the
    /// dream's supersession review demotes "you work with main.rs" — work IS training.
    /// The measurement fork itself stays #59-isolated either way; only the lesson
    /// crosses back.
    ///
    /// There is NO default on the Rust side — [`LearningPolicy`] has no `Default` impl, so
    /// every construction site must state whether this run is a measurement or her life. See
    /// [`crate::cognition::learning_policy`] for why (BigMama, 2026-08-06: two modules had
    /// opposite defaults for this one field, and a fail-safe default would only have changed
    /// which forgetful caller got burned). An OMITTED wire field resolves to
    /// [`LearningPolicy::DoNotLearn`] — this command is the headless BENCHMARK entrypoint
    /// (#218), so its population is measurement-heavy and omission must fail safe.
    #[serde(default = "LearningPolicy::wire_default")]
    #[ts(type = "boolean", optional)]
    pub learn: LearningPolicy,
    /// GLASS-BOX (opt-in): directory for the JSONL turn-capture sink — every tick's bids +
    /// DECISION + timings append to `<dir>/<persona_id>.jsonl`, same sink `cognition/eval`
    /// wires (task #14). THE tool for diagnosing an acts=1 silent settle: the capture says
    /// whether she chose Act/Respond/Pass and why, where the bare ledger only shows the
    /// aftermath. Fork-only, never her live mind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub capture_dir: Option<String>,
    /// HARNESS-INTERNAL (set by the attempts loop, never by callers): sha256 of
    /// the previous FAILED attempt's patch. When this attempt settles with a
    /// byte-identical diff, ONE bounded re-drive fires with the hash-proven
    /// fact — catching the resubmission BEFORE a redundant grade burns the
    /// attempt (round E receipts: BOTH citizens copied their failed patch on
    /// attempt 3, and post-grade detection could only warn an attempt 4 that
    /// never exists).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(skip)]
    pub prev_failed_patch_sha: Option<String>,
    /// DIAGNOSTIC ONLY (default false — she competes WHOLE, memory ON). When true, her
    /// durable episodic/semantic recall is suppressed for this run — the same probe
    /// `cognition/eval` exposes ([[eval-measures-the-true-full-being-not-a-stripped-copy]]).
    /// Its ONLY legitimate use is glass-boxing session contamination: run a failing task
    /// with recall OFF — PASS ⇒ the failure is recall-mediated (stale/cross-task engrams
    /// surfacing), STILL FAIL ⇒ the failure is deeper (pipeline/model). NEVER a scoring
    /// mode — a benchmark number produced with recall off measures a stripped copy, not the
    /// being ([[benchmark-must-never-score-persona-against-a-soul-stripped-copy]]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub suppress_recall: Option<bool>,
    /// What the CALLER will grade — the turn's contract, declared by the harness that
    /// holds the answer key. Default [`Deliverable::Answer`]: her utterance is the
    /// result (#220's answer-graded tasks). A SWE-style harness that applies the DIFF
    /// and never reads the speech declares [`Deliverable::Workspace`], and the settle
    /// driver stops treating a zero-change explanation as a finished turn. Structural,
    /// caller-owned; it steers nothing about WHAT she does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub deliverable: Option<Deliverable>,
    /// This run is SCORED — a benchmark instance whose verdict comes from applying her diff and
    /// running the tests, not from anything she says. Hardens her write path: an edit whose code
    /// would land inside a string literal is refused rather than warned (#317), because the run
    /// cannot recover from a file it believes it fixed.
    ///
    /// Default `false`, and deliberately NOT inferred from `deliverable` — `agent/solve` also
    /// does real work for real teammates, and a citizen doing real work writes code as text
    /// whenever she means to (a docstring example, a fixture). Only the caller that is GRADING
    /// her knows the ambiguity is gone, so only that caller sets this.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub scored: Option<bool>,
    /// Directories to PREPEND to `PATH` for her shell — the interpreter/toolchain this
    /// task needs in order to be RUNNABLE. A SWE harness passes the era-matched venv's
    /// `bin` here so `python` and `pytest` exist for her.
    ///
    /// Without it she can write a fix but never execute anything to check it. Measured
    /// on sympy-21379: a correct reproduction script met `bash: python: command not
    /// found`, and the run scored as a capability failure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub path_prepend: Option<Vec<String>>,
    /// N CHANCES (Joel, 2026-08-08: "they too need to learn how to investigate and fix
    /// their code"): how many graded attempts this detached run may take. After a
    /// non-resolved auto-grade, the NEXT attempt re-enters the SAME workspace (her
    /// previous edits intact) with the grader's verdict — named failing tests — appended
    /// to the task, so investigating her own failure IS the work. Only meaningful on a
    /// detached, auto-graded run; a resolved grade, an ungradeable tree, or a harness
    /// fault ends the run early. Default 1 (one shot, exactly the old behavior) — the
    /// per-benchmark adapter that dispatches the run owns its N, not this abstraction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(optional, type = "number")]
    pub attempts: Option<u32>,
    /// Teammates on this solve (team-proof gap 3, 2026-08-30): peers who joined the
    /// solve room to review. The measured-work quiesce lease excepts them alongside
    /// the solver — the first team round proved a reviewer quiesced for the solve's
    /// whole duration is structurally unable to review (Kira: 28 min, zero turns).
    /// The lease's lane-demand override counts them too, so serving budgets a warm
    /// slot per PARTICIPANT, not per resident.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(optional, type = "Array<string>")]
    pub teammates: Vec<Uuid>,
}

/// What the caller grades when the solve returns. Two genuinely different contracts,
/// so it is an enum on the wire, never a magic string ([[strings-to-enums]]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "lowercase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/agent/Deliverable.ts"
)]
pub enum Deliverable {
    /// Her spoken answer is the result (the default — every non-diff task).
    #[default]
    Answer,
    /// The state of the workspace is the result; the grader applies the diff.
    Workspace,
}

#[derive(Debug, Clone, Serialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/agent/AgentSolveResult.ts"
)]
pub struct AgentSolveResult {
    pub persona_id: crate::identity::PersonaRef,
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
    /// Paths her acts NAMED (reads, searches, edit attempts — any `file_path`/`path`
    /// arg on an executed call), first-touch order. The investigation trail as STATE:
    /// a failed edit or a read appears here and nowhere else. The N-chances retry
    /// threads these into the next attempt's task, because a retry is a FRESH turn
    /// with fresh working memory — without this, "the file you already identified"
    /// names knowledge the next attempt does not have (glass-boxed 2026-08-08,
    /// benchy-sympy-22840-n4 attempt 2: 10 of 12 acts re-deriving cse_main.py).
    #[serde(default)]
    pub files_examined: Vec<String>,
    /// True when this is the immediate ACK of a detached run (`acts`/`patch` empty — poll the
    /// result file `agent-solve-<run_id>.json` for the real outcome).
    pub detached: bool,
    /// The run's correlation id (set on a detached ack + the written result file).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub run_id: Option<String>,
    /// INFRASTRUCTURE FAILURE, never a wrong answer. `Some(cause)` when the settle loop
    /// stopped because the deliberation model call FAILED (lane torn down mid-drive, a
    /// serving lane refusing a model it isn't hosting, a timeout) rather than because she
    /// finished. `SettleOutcome::inference_error` has carried this all along and its own
    /// doc says the grader MUST treat it as infra — but NOTHING READ IT, so a run
    /// truncated at act 7 of 30 reported `acts: 7, patchBytes: 0` and was indistinguishable
    /// from a persona who simply failed. Measured 2026-08-04 on sympy-21379: the lane went
    /// `serving: <none>, ready: false` mid-drive and the verdict said nothing at all.
    /// A number produced with this set is NOT a score
    /// ([[a-benchmark-zero-is-a-claim-about-the-harness-until-proven-otherwise]]).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub infra_error: Option<String>,
}

/// `agent/solve` — headless single-task agent run. Pure orchestration over the eval drive
/// machinery; never hand-spawns a llama-server (the serving system owns lifecycle) — it stands up
/// a measurement lane the same way `cognition/eval` does, holds it for the drive, drops it after.
#[derive(Default)]
pub struct AgentSolve;
