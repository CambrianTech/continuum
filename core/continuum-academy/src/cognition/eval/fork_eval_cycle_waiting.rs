use super::*;

/// Fork a measurement copy, WAITING out the post-reboot template race. After a reboot
/// `register_from_cfg` assembles each persona's fork-template asynchronously, so an eval
/// fired seconds after boot can race ahead of it and see `None`. This race is NOT specific
/// to one lane — it hits the live-lane, base-model-lane, AND gene-lane forks identically —
/// so ALL of them retry through here (previously only the live-lane branch did, and a
/// tool-using eval on a base_model_id fired right after `continuum reboot` failed loud instead of
/// waiting). `fork` is the per-lane fork call; retried up to
/// [`WORKSPACE_TEMPLATE_WAIT_TRIES`] times, 1s apart, then `None` (caller fails loud).
pub(super) async fn fork_eval_cycle_waiting(
    persona_uuid: &Uuid,
    mut fork: impl FnMut() -> Option<crate::cognition::workspace::WorkspaceCycle>,
) -> Option<crate::cognition::workspace::WorkspaceCycle> {
    for attempt in 0..WORKSPACE_TEMPLATE_WAIT_TRIES {
        if let Some(cycle) = fork() {
            return Some(cycle);
        }
        if attempt == 0 {
            tracing::info!(
                %persona_uuid,
                "eval: workspace template not ready (post-spawn register_from_cfg race) — waiting"
            );
        }
        // Emit the wait as an EVENT so positronic layers show "preparing…" instead of a
        // dead spinner — this null-progress window otherwise looked identical to a hang.
        emit_eval_phase(
            "preparing",
            &format!("waiting for workspace template ({}s)", attempt + 1),
        );
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
    None
}

/// Emit an eval LIFECYCLE phase on the bus (+ a probe) so every positronic layer can
/// show live feedback during the windows where `eval:progress` is silent — lane
/// spawn, model cold-load, template wait, teardown. Fire-and-forget, Noop when the
/// bus is unwired (tools/tests): observability at zero hot-path cost
/// [[observability-as-substrate]]. `eval:progress` carries per-task grading; this
/// carries the BEFORE/AROUND-generation states that were previously dark.
pub(crate) fn emit_eval_phase(phase: &str, detail: &str) {
    if let Some(bus) = crate::runtime::MessageBus::global() {
        bus.publish_async_only(
            "eval:phase",
            serde_json::json!({
                "phase": phase,
                "detail": detail,
                "atMs": EpochMs::now().millis(),
            }),
        );
    }
    continuum_base::probe!(
        class = "eval.phase",
        phase = phase,
        detail = detail,
        "eval lifecycle phase"
    );
}

/// Run a REAL definition-of-done: a shell command in the persona's workspace (cwd). Pass =
/// exit 0. Returns `(ok, verdict)`; on failure the verdict carries the real stdout+stderr
/// (tail-bounded) so the recovery loop hands the model the actual error to fix against.
/// This is how a persona works on REAL things — her file edits are checked by a real build/
/// test, not by grading text she typed into chat.
/// Run a task's setup/DoD shell IN THE RUN'S WORLD. `root` is the same
/// resolved run root the artifact grader uses (task pin or the #312 ephemeral
/// clone); `None` preserves process-CWD for callers that genuinely run there.
///
/// Defect 2's SIBLING, found live 2026-08-23 (MirrorCode maiden round):
/// setup_shell staged fixtures into the CORE's CWD (the real repo checkout —
/// a `mirrorcode/` tree appeared in the operator's working copy) while her
/// hands wrote into the clone, and dod_shell then graded a world containing
/// the fixtures but not her work — 100% false fails AND repo pollution.
/// DS-1000's clean sweep predates the ephemeral roots, when CWD *was* the
/// workspace — a coincidence, not a design.
/// The outcome of running a definition-of-done — a THREE-way verdict, not a bool.
/// The distinction is load-bearing and systemic (2026-08-25, Joel: "these are issues
/// they'd have in projects... make sure it's not coded into the wrong concern"): a
/// DoD that RAN and came back red is a real miss she can learn from; a DoD that could
/// not RUN (the grader spawn failed, the workspace path didn't resolve, a command was
/// refused / not found) is a HARNESS/infra fault — scoring it as a model miss and
/// teaching her "I failed" is the exact false-failure the gold-gate discipline exists
/// to prevent, and it hits EVERY dod-graded activity, benchmarks and real projects
/// alike. Distinguishing here, at the one place a DoD runs, is the right concern.
#[derive(Debug, Clone)]
pub enum DodVerdict {
    /// Ran, exited 0 — done.
    Pass(String),
    /// Ran to a real red result (tests failed, assertion, wrong output) — a genuine
    /// miss she can be handed and re-drive against.
    Fail(String),
    /// The grader itself could not run (spawn failure, missing workspace/path, a
    /// refused/not-found command, a harness infra exit). NOT a model miss.
    InfraError(String),
}

impl DodVerdict {
    /// True only when the DoD genuinely passed. (`Fail` and `InfraError` are both
    /// not-pass, but callers that must NOT conflate them use the variant directly.)
    pub fn passed(&self) -> bool {
        matches!(self, DodVerdict::Pass(_))
    }
    /// The human-facing verdict text (for the grade string / re-drive delivery).
    pub fn message(&self) -> &str {
        match self {
            DodVerdict::Pass(m) | DodVerdict::Fail(m) | DodVerdict::InfraError(m) => m,
        }
    }
}

/// Does this failing DoD output signal the GRADER itself broke (infra) rather than a
/// genuine red result? Unambiguous "could not run" markers only — a test failure
/// ("3 failed", an assertion traceback) is a REAL miss and must NOT match here.
pub(super) fn dod_output_is_infra(exit_code: Option<i32>, out: &str) -> bool {
    // 127 = command not found; the terminal-bench harness reserves 4/5 for
    // staging-corrupt / verifier-deps-missing (its own infra exits).
    if matches!(exit_code, Some(127) | Some(4) | Some(5)) {
        return true;
    }
    let o = out.to_ascii_lowercase();
    // Substrate/tooling failure phrases — the grader could not execute, distinct from
    // a graded red. Kept narrow on purpose: each is a "the command broke" signal, not
    // a "the code is wrong" signal.
    const INFRA_MARKERS: &[&str] = &[
        "substrate refused",
        "no such file or directory",
        "command not found",
        "could not read",
        "could not run",
        "permission denied",
        ": not found",
    ];
    INFRA_MARKERS.iter().any(|m| o.contains(m))
}

pub(super) async fn run_dod(root: Option<&std::path::Path>, cmd: &str) -> DodVerdict {
    let bash = match continuum_base::shell_portable::locate_bash() {
        Ok(bash) => bash,
        Err(why) => {
            return DodVerdict::InfraError(format!(
                "DoD `{cmd}` could not RUN (no usable shell): {why}"
            ));
        }
    };
    let mut command = tokio::process::Command::new(bash);
    command.arg("-lc").arg(cmd);
    if let Some(r) = root {
        command.current_dir(r);
    }
    match command.output().await {
        Ok(o) => {
            let ok = o.status.success();
            let mut s = String::from_utf8_lossy(&o.stdout).into_owned();
            s.push_str(&String::from_utf8_lossy(&o.stderr));
            let s = s.trim();
            let tail: String = {
                let n = s.chars().count();
                if n > 2000 {
                    format!("…{}", s.chars().skip(n - 2000).collect::<String>())
                } else {
                    s.to_string()
                }
            };
            if ok {
                DodVerdict::Pass(format!("DoD passed: `{cmd}`"))
            } else if dod_output_is_infra(o.status.code(), &tail) {
                DodVerdict::InfraError(format!(
                    "DoD `{cmd}` could not RUN (grader/infra fault, not a wrong answer):\n{tail}"
                ))
            } else {
                DodVerdict::Fail(format!("DoD `{cmd}` FAILED:\n{tail}"))
            }
        }
        Err(e) => DodVerdict::InfraError(format!("DoD `{cmd}` could not spawn: {e}")),
    }
}

/// The held-out half of a workspace grade ([`EvalTask::held_out_shell`]), applied to the
/// visible DoD's verdict: it runs only when that verdict passed, and its own verdict text
/// never carries the command or its output, because both would teach the rule it checks.
/// What the command printed goes to the probe stream, where an operator reads it.
pub(crate) async fn run_held_out(root: Option<&std::path::Path>, t: &EvalTask, visible: DodVerdict) -> DodVerdict {
    let (cmd, seen) = match (t.held_out_shell.as_deref(), visible) {
        (Some(cmd), DodVerdict::Pass(seen)) => (cmd, seen),
        (_, visible) => return visible,
    };
    let held = run_dod(root, cmd).await;
    continuum_base::probe!(
        class = "eval.task.held_out",
        task = %t.id,
        passed = held.passed(),
        infra = matches!(held, DodVerdict::InfraError(_)),
        output = %held.message(),
        "the held-out check ran on work whose visible definition of done passed"
    );
    match held {
        DodVerdict::Pass(_) => DodVerdict::Pass(format!("{seen}; the held-out check passed")),
        DodVerdict::Fail(_) => DodVerdict::Fail(format!(
            "{seen}, but the held-out check failed: the work misses a requirement the task did not state"
        )),
        DodVerdict::InfraError(_) => DodVerdict::InfraError(
            "the held-out check could not RUN (grader/infra fault, not a wrong answer; its output is on the eval.task.held_out probe)".into(),
        ),
    }
}

/// A workspace task's whole grade: its visible definition of done, then its held-out check.
/// The one composition every grader of a `dod_shell` task runs (the eval's fallback and team
/// arms, the room's gym grader), so no grader can skip the held-out half.
pub async fn run_workspace_grade(root: Option<&std::path::Path>, t: &EvalTask, dod: &str) -> DodVerdict {
    run_held_out(root, t, run_dod(root, dod).await).await
}

/// Grade a FUNCTIONAL WEB-DEV task by OBSERVING what the persona's UI actually rendered, then
/// scoring the element tree against the task's `ui_checks`. Returns `(ok, verdict)` in the same
/// shape as [`run_dod`].
///
/// # Purity — she is graded through HER OWN eyes
///
/// The observation routes `perception/observe` through the persona's OWN identity-bearing
/// executor (`cycle.acting().executor`), exactly like the workspace-root seam roots her file
/// engine — NOT a separate grader-only command path. The persona sees her work via the same
/// eye-node the benchmark scores it with; there is one path, so the first-class citizen stays one
/// ([[dispatch-path-purity-and-load-harness]]). The core is headless and cannot render, so a
/// perception task requires a connected eye-node adapter; with none, the observe call fails LOUD
/// and the task grades an honest infra-fail, never a fabricated pass ([[fallbacks-are-illegal-fail-loud]]).
/// Extract an HTML artifact the persona SPOKE — the last ```html (or doctype/`<html`)
/// fenced block, or a bare page answer — for the mouth-or-hands web-dev capture below.
/// Returns the inner HTML, or `None` when her answer carries no page.
///
/// The last qualifying fence wins: across a multi-turn build, the FINAL page she emitted
/// is the one to grade (earlier fences are superseded drafts).
pub(super) fn html_artifact_from_answer(answer: &str) -> Option<String> {
    let mut best: Option<String> = None;
    let mut rest = answer;
    while let Some(open) = rest.find("```") {
        let after = &rest[open + 3..];
        let nl = after.find('\n').unwrap_or(after.len());
        let lang = after[..nl].trim().to_ascii_lowercase();
        let body_area = if nl < after.len() {
            &after[nl + 1..]
        } else {
            ""
        };
        let Some(close) = body_area.find("```") else {
            break;
        };
        let body = body_area[..close].trim();
        let head = body.trim_start().to_ascii_lowercase();
        let looks_html =
            lang.starts_with("html") || head.starts_with("<!doctype") || head.starts_with("<html");
        if looks_html && !body.is_empty() {
            best = Some(body.to_string()); // keep the LAST qualifying fence
        }
        rest = &body_area[close + 3..];
    }
    if best.is_some() {
        return best;
    }
    // No fence — the whole answer may itself be the page.
    let trimmed = answer.trim();
    let head = trimmed.to_ascii_lowercase();
    if head.starts_with("<!doctype") || head.starts_with("<html") {
        return Some(trimmed.to_string());
    }
    None
}

pub(super) async fn perception_grade(
    cycle: &crate::cognition::workspace::WorkspaceCycle,
    target: &str,
    checks: &[crate::perception::scoring::UiCheck],
    threshold: f32,
    workspace_root: Option<&str>,
    answer: &str,
) -> (bool, String) {
    let acting = match cycle.acting() {
        Some(a) => a,
        None => {
            return (
                false,
                "web-dev grade needs hands (no acting body) to route perception/observe — \
                 cannot observe a pure-cognition persona's UI"
                    .to_string(),
            )
        }
    };
    // Absolutize a workspace-relative target to a `file://` URL. She wrote her UI through
    // `code/write`, whose hands were rooted at `workspace_root` when the caller pinned one
    // (the from-scratch build path: a clean per-benchmark dir) — so grade the file THERE,
    // not at the core cwd. Without a pin she uses the default root (core cwd) exactly as the
    // hands do. Writer and grader MUST agree on the root or a correct artifact scores a false
    // zero (the #49 dual-root gap). An explicit URL passes through untouched.
    let target_url = if target.contains("://") {
        target.to_string()
    } else {
        let base = match workspace_root {
            Some(root) => std::path::PathBuf::from(root),
            None => match std::env::current_dir() {
                Ok(cwd) => cwd,
                Err(e) => {
                    return (
                        false,
                        format!("could not resolve workspace cwd for target '{target}': {e}"),
                    )
                }
            },
        };
        format!(
            "file://{}/{}",
            base.display(),
            target.trim_start_matches('/')
        )
    };

    // Local static artifact → the STATIC-HTML eye grades it headless. The eval core has
    // no browser eye-node connected, so routing `perception/observe` for a file:// target
    // fails loud ("no adapter to fulfil it") and a correct render scores a false zero
    // (#206). Her `ui_checks` are structural (tags/roles/text/counts) — a pure html5ever
    // parse answers them the way a browser's a11y tree would. A remote URL / live surface
    // still routes to the browser eye below (a persona's real seeing loop).
    if let Some(path) = target_url.strip_prefix("file://") {
        let p = std::path::Path::new(path);
        // NO MOUTH-FOR-HANDS. There used to be a rescue here: if she never wrote the file
        // but SPOKE a page in a ```html fence, the harness materialized it and graded that.
        // It was disclosed and well-intentioned — it isolated "can you build a UI" from "can
        // you route a tool", after a 2026-07-21 run where Qwen2.5-Coder-7B wrote a flawless
        // login form in a fence, wrote no file, and scored 0/6 below a weaker model.
        //
        // It is deleted because it measures a persona we are not building. Ours DO the thing:
        // they operate tools and produce artifacts that exist. A page that lives only in an
        // utterance is not a page — it is a description of one, and grading it as a page is a
        // scam we play on ourselves. The file already says this 1800 lines up, about
        // `solution_file`: "the act→verify loop is only visible if we grade what she actually
        // wrote + compiled, not what she narrated." The rescue contradicted the doctrine its
        // own module declares.
        //
        // The cost of keeping it was not a wrong number, it was a HIDDEN GAP: because nothing
        // in the suite ever required hands, the fence→act failure was never scored, so it was
        // never fixed — and it is exactly the failure SWE-bench exposes at full strength (30
        // acts, 0 files, 0 refusals). What is rescued is never repaired.
        //
        // The spoken-artifact detector survives with its consequence inverted: it used to
        // launder the failure, it now NAMES it, so a zero is diagnosable instead of mute.
        // [[beat-oss-agentic-systems-as-whole-beings-never-strip-to-pass]],
        // [[fix-the-substrate-never-rig-the-persona-the-line-between-assist-and-scaffold]],
        // [[execute-dont-narrate]]
        let wrote_with_hands = std::fs::read_to_string(p)
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false);
        if !wrote_with_hands {
            let spoke_one = html_artifact_from_answer(answer).is_some();
            return (
                false,
                if spoke_one {
                    format!(
                        "web-dev grade: FAIL — she SPOKE a complete HTML artifact but never \
                         routed it through `code/write`, so '{path}' does not exist. The task \
                         is graded on the file her hands produced, never on her narration of \
                         it. This is the fence→act gap, and it is the finding, not a harness \
                         defect."
                    )
                } else {
                    format!(
                        "web-dev grade: FAIL — no artifact at '{path}' and no HTML in her \
                         answer either; she neither built it nor described it."
                    )
                },
            );
        }
        let obs = crate::perception::static_html::observe_file(p);
        let grade = crate::perception::scoring::grade_ui(&obs, checks, threshold);
        return (grade.passed, grade.summary);
    }

    let params = crate::perception::ObserveParams {
        target: target_url.clone(),
        viewport: None,
        selector: None,
    };
    let ws_ctx = crate::cognition::tool_executor::ToolExecutionContext {
        persona_id: acting.persona_id,
        persona_name: acting.persona_name.clone(),
        session_id: Uuid::new_v4(),
        context_id: Uuid::new_v4(),
        caller_context: serde_json::Value::Null,
        persona_config: crate::cognition::tool_executor::PersonaMediaConfigLite {
            auto_load_media: false,
            supported_media_types: vec![],
        },
    };
    let observe_call = crate::ai::types::ToolCall {
        id: "eval-perception-observe".to_string(),
        name: "perception/observe".to_string(),
        input: serde_json::json!(params),
    };
    // Grading needs the WHOLE JSON (structure tree + inline PNG); results come back whole.
    let out = match acting
        .executor
        .execute_native_batch(std::slice::from_ref(&observe_call), &ws_ctx)
        .await
    {
        Ok(o) => o,
        Err(e) => {
            return (
                false,
                format!("perception/observe could not run for '{target_url}' (no eye-node adapter connected?): {e}"),
            )
        }
    };
    let content = match out.results.first() {
        Some(r) if r.is_error.is_none() => &r.content,
        Some(r) => {
            return (
                false,
                format!(
                    "perception/observe failed for '{target_url}': {}",
                    r.content
                ),
            )
        }
        None => {
            return (
                false,
                format!("perception/observe returned no result for '{target_url}'"),
            )
        }
    };
    let obs: crate::perception::ObserveResult = match serde_json::from_str(content) {
        Ok(o) => o,
        Err(e) => {
            let preview: String = content.chars().take(400).collect();
            return (
                false,
                format!("could not parse observation for '{target_url}': {e} — got: {preview}"),
            );
        }
    };
    let grade = crate::perception::scoring::grade_ui(&obs, checks, threshold);
    (grade.passed, grade.summary)
}

// ── Live eval progress (#123/#141, Joel: "widgets and others like yourself should
// subscribe to these events, current totals, by command, and also display the
// process — things like this take forever so you really need to provide feedback").
//
// ONE source of truth (each graded task), THREE surfaces:
//  1. watch snapshot — the substrate-canonical live surface; `cognition/eval-status`
//     with no run_id returns it (poll, works for agents/scripts today);
//  2. `eval:progress` bus event — widgets / positron (#141) / persona observers
//     subscribe (published via the global bus, same set_global precedent as the
//     persona registry);
//  3. `eval.task` probe — the greppable log pulse.
//
// ONE progress row is correct by construction: the eval-preemption lease serializes
// evals on the GPU, so at most one pass is grading at any instant.
#[derive(Debug, Clone, Serialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/EvalPassProgress.ts"
)]
pub struct EvalPassProgress {
    /// Tasks graded so far in the CURRENT pass.
    #[ts(type = "number")]
    pub done: u32,
    /// Tasks in the current pass.
    #[ts(type = "number")]
    pub total: u32,
    /// Passes so far — `pass / done` is the running rate.
    #[ts(type = "number")]
    pub pass: u32,
    /// The task just graded.
    pub current_task: String,
    /// Whether it passed.
    pub last_ok: bool,
    /// Output tokens the model produced for this task. A near-zero value on a FAILED
    /// task is the decline/wedge signature (a bare "PASS" is ~2 tokens) — this is the
    /// LIVE signal a monitoring widget/persona needs to light a cell SUSPECT (harness
    /// noise) vs a real wrong answer, in real time instead of post-hoc from the ledger.
    #[ts(type = "number")]
    pub output_tokens: u32,
    /// Receiver-clock ms — staleness signal for readers (a row older than a task's
    /// typical latency means the pass ended or died; the ledger row is the truth).
    #[ts(type = "number")]
    pub updated_at_ms: u64,
    /// Live VRAM available (GB) at grade time — the resource-efficiency axis next to
    /// accuracy and latency, sampled from the ONE per-machine authority. `null` when
    /// VRAM is ungoverned (bare tests) — absence is honest, never a fabricated 0.
    #[ts(optional, type = "number")]
    pub vram_free_gb: Option<u64>,
    /// The run_id producing THIS snapshot, when the pass carries a handle. Lets a
    /// persona-only poll (no run_id arg) tell it's reading ITS run vs a prior one —
    /// the stale-progress trap where a fresh detached run reads as instantly-complete
    /// with the last run's numbers. `null` for a sync/handleless run.
    #[ts(optional)]
    pub run_id: Option<String>,
}

/// Identity of the pass currently grading — set at [`run_eval`] entry (RAII), read
/// by [`report_task_graded`] into the live snapshot AND the durable per-task ledger
/// row. Correct-by-construction because the eval-preemption lease serializes evals:
/// at most one pass grades at any instant (the same invariant that makes ONE
/// progress row correct). Grew from a bare run_id (2026-08-24): the per-task row
/// needs the persona (which ledger file) and the eval set (which gym) at grade time,
/// and threading them through both pass loops would be the param-hell the scope
/// exists to avoid.
pub(super) struct RunMeta {
    pub(super) run_id: Option<String>,
    pub(super) persona_id: String,
    pub(super) eval_set: String,
}

pub(super) static CURRENT_RUN_META: std::sync::Mutex<Option<RunMeta>> = std::sync::Mutex::new(None);

/// RAII: stamp the current pass's identity for the live snapshot; clear on drop so it
/// rides early-return AND panic (Drop runs on unwind) — never a stale run_id bleeding
/// into the next pass or an idle board.
pub(super) struct RunIdScope;

impl RunIdScope {
    pub(super) fn enter(run_id: Option<String>, persona_id: String, eval_set: String) -> Self {
        if let Ok(mut g) = CURRENT_RUN_META.lock() {
            *g = Some(RunMeta {
                run_id,
                persona_id,
                eval_set,
            });
        }
        RunIdScope
    }
}

impl Drop for RunIdScope {
    fn drop(&mut self) {
        if let Ok(mut g) = CURRENT_RUN_META.lock() {
            *g = None;
        }
    }
}

/// The run_id of the exam pass grading RIGHT NOW, `None` when idle. Public for
/// the positron bench emitter (#141's third surface made real): a live exam is
/// WORK the room's rail must show, through the same pipe every other run row
/// rides — never a separate poller.
pub fn live_eval_run_id() -> Option<String> {
    CURRENT_RUN_META
        .lock()
        .ok()
        .and_then(|g| g.as_ref().and_then(|m| m.run_id.clone()))
}

pub(super) static EVAL_PROGRESS: std::sync::OnceLock<tokio::sync::watch::Sender<Option<EvalPassProgress>>> =
    std::sync::OnceLock::new();

pub(super) fn eval_progress_tx() -> &'static tokio::sync::watch::Sender<Option<EvalPassProgress>> {
    EVAL_PROGRESS.get_or_init(|| tokio::sync::watch::channel(None).0)
}

/// Subscribe to live eval progress — the watch every reader shares.
pub fn subscribe_eval_progress() -> tokio::sync::watch::Receiver<Option<EvalPassProgress>> {
    eval_progress_tx().subscribe()
}

/// RAII abort for a spawned helper task — dropped with the pass, so the help
/// listener can never outlive the exam it serves.
pub(super) struct AbortOnDrop(pub(super) tokio::task::JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

pub(super) fn scopeguard_abort(h: tokio::task::JoinHandle<()>) -> AbortOnDrop {
    AbortOnDrop(h)
}

/// The idle-backstop verdict: an infra outcome whose probe is a SUBSTRATE BUG
/// REPORT (a hang that survived every event-based guard — a lost await between
/// inference calls), never routine control flow. Fires only after the idle
/// window passed with ZERO real lane work stamped; a slow-but-decoding task can
/// never reach it (2026-08-24: the flat wall-clock predecessor killed a task
/// mid-decode twice in one day).
pub(super) fn hang_backstop_outcome(
    task_id: &str,
    room: crate::identity::ActivityRoom,
    window: std::time::Duration,
) -> crate::cognition::act_observe::SettleOutcome {
    continuum_base::probe!(
        class = "eval.task.hang_backstop",
        task = %task_id,
        backstop_s = window.as_secs(),
        "idle backstop fired — no settle, no propagated error, and NO lane work for the \
         whole window; find the await that lost its error propagation"
    );
    crate::cognition::act_observe::SettleOutcome::infra_failure(
        room.as_uuid(),
        format!(
            "idle backstop ({}s with zero lane work) — no settle and no propagated error; \
             substrate bug, not a wrong answer",
            window.as_secs()
        ),
    )
}

/// Report one graded task on all three surfaces. Called by BOTH pass loops (solo +
/// team) — one reporter, no drift.
pub(super) fn report_task_graded(
    task_id: &str,
    ok: bool,
    acts: u32,
    latency_ms: u64,
    output_tokens: u32,
    pass: u32,
    done: usize,
    total: usize,
) {
    // Efficiency axis: sample the live board (lock-free watch read) so every graded
    // task carries the VRAM state it ran under — the tuning signal for knobs like
    // max_acts / context budget / lane count ([[self-improvement-is-a-control-loop]]:
    // the reward is only as trustworthy as the metric, and a score without its
    // resource cost is half a metric).
    let vram_free_gb = crate::resources::ResourceDaemon::global().and_then(|d| {
        d.board()
            .kinds
            .iter()
            .find(|k| k.kind == crate::resources::ResourceKind::Vram)
            .map(|k| k.available_bytes / 1_000_000_000)
    });
    let snap = EvalPassProgress {
        done: done as u32,
        total: total as u32,
        pass,
        current_task: task_id.to_string(),
        last_ok: ok,
        output_tokens,
        updated_at_ms: EpochMs::now().millis(),
        vram_free_gb,
        run_id: CURRENT_RUN_META
            .lock()
            .ok()
            .and_then(|g| g.as_ref().and_then(|m| m.run_id.clone())),
    };
    // DURABLE per-task grade row (the one-command-round resume seam): the run summary
    // row only lands when the whole pass completes, so a killed/rebooted round left NO
    // record of which tasks already graded — resume meant re-running everything. Stream
    // one `kind:"task"` row into the SAME per-persona ledger at grade time (the file
    // eval-status already owns; `row_with_run_id` skips task rows) so
    // `benchmark/round` can continue from the first ungraded task.
    if let Ok(guard) = CURRENT_RUN_META.lock() {
        if let Some(meta) = guard.as_ref() {
            if let Some(rid) = &meta.run_id {
                append_task_ledger_row(&meta.persona_id, rid, &meta.eval_set, task_id, ok, acts);
            }
        }
    }
    let _ = eval_progress_tx().send_replace(Some(snap.clone()));
    if let Some(bus) = crate::runtime::MessageBus::global() {
        if let Ok(v) = serde_json::to_value(&snap) {
            bus.publish_async_only("eval:progress", v);
        }
    }
    // run_id ON the row (2026-08-24, "not good to be so dumb about what's graded"):
    // an unscoped consumer counting eval.task matches summed every round, sentinel
    // A/B, and smoke eval ever run — "51 graded" while the live round had zero. The
    // row itself must carry which round it belongs to.
    let run_id = CURRENT_RUN_META
        .lock()
        .ok()
        .and_then(|g| g.as_ref().and_then(|m| m.run_id.clone()))
        .unwrap_or_default(); // sync/handleless run: empty = "no round", honest for a probe field
    continuum_base::probe!(
        class = "eval.task",
        task = task_id,
        run_id = %run_id,
        ok = ok,
        acts = acts,
        done = done,
        total = total,
        pass = pass,
        running_rate = format!("{:.3}", pass as f64 / done.max(1) as f64).as_str(),
        latency_ms = latency_ms,
        vram_free_gb = vram_free_gb.unwrap_or(0),
        "task graded",
    );
}

/// Remove every entry INSIDE `root` (files + subdirs) but keep `root` itself, so a persona
/// whose hands are rooted there keeps a valid, EMPTY working directory. Used by the eval to
/// give each from-scratch build task a clean slate (#209): a correct render on task N must
/// never be graded against task N-1's leftover files. A missing root is not an error (nothing
/// to clean); a genuine IO failure propagates so the caller can warn without aborting the run.
pub(super) fn clean_dir_contents(root: &str) -> std::io::Result<()> {
    let p = std::path::Path::new(root);
    if !p.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(p)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            std::fs::remove_dir_all(&path)?;
        } else {
            std::fs::remove_file(&path)?;
        }
    }
    Ok(())
}

/// The outcome of running a whole task set once — the pass count and per-task rows,
/// PLUS the infra-fault accounting that decides whether the number is trustworthy.
/// The keystone of the Proctored Exam Session (Slice B): an inference call that FAILS
/// (lane not-ready / connect-refused / "not the active served model" / compute-error /
/// stream-idle) is an INFRA fault, never a wrong answer. A transient fault is re-verified
/// and retried; an UNRECOVERABLE one (lane never returns to decode-ready) makes the run
/// [`InfraUnavailable`] — the caller reports that, NEVER a fake `pass_rate: 0.0`.
/// [[proctored-exam-session-dependable-benchmark]]
pub(super) struct PassOutcome {
    /// Tasks that PASSED (graded on a verified lane).
    pub(super) pass: u32,
    /// Per-task rows (includes any infra-graded row for forensics).
    pub(super) results: Vec<EvalTaskResult>,
    /// How many tasks hit an UNRECOVERABLE infra fault — the lane never came back to
    /// decode-ready within the re-verify+retry budget. ≥1 ⇒ the run is void.
    pub(super) infra_faults: u32,
    /// The cause of the FIRST unrecoverable infra fault, naming its task — the reason the
    /// whole run aborts as [`InfraUnavailable`]. `None` when every task reached a verdict.
    pub(super) infra_reason: Option<String>,
}
