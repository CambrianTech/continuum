//! How an answer is graded: the substring-or-silence fallback for descriptive tasks, the
//! definition-of-done shell verdict (with its infra classification), the held-out and workspace
//! DoD grades, and the perception grade that scores the UI her hands actually built.

use super::*;

/// The MOUTH grade, shared by the solo and team passes: a `silence` task passes when
/// she said nothing; otherwise the answer must contain `expect` (case-insensitive).
/// One definition so the two passes cannot drift on what "quiet" means.
pub(crate) fn substring_or_silence_grade(t: &EvalTask, answer: &str) -> (bool, String) {
    if t.silence {
        let quiet = answer.trim().is_empty();
        return (
            quiet,
            if quiet {
                "silence, as the task required".into()
            } else {
                format!("spoke {} chars where silence was required", answer.chars().count())
            },
        );
    }
    let m = !t.expect.is_empty() && answer.to_lowercase().contains(&t.expect.to_lowercase());
    (m, if m { "substring match".into() } else { "no match".into() })
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
    let bash = match crate::shell_portable::locate_bash() {
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
    crate::probe!(
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
pub(crate) async fn run_workspace_grade(root: Option<&std::path::Path>, t: &EvalTask, dod: &str) -> DodVerdict {
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
        let (lang_line, body_area) = after.split_once('\n').unwrap_or((after, "")); // unwrap_or: a fence with no line break is all info string and has no body
        let lang = lang_line.trim().to_ascii_lowercase();
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
        let wrote_with_hands = match std::fs::read_to_string(p) {
            Ok(page) => !page.trim().is_empty(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
            // A page that exists but cannot be read is not graded as "never built": name it.
            Err(e) => {
                return (
                    false,
                    format!("web-dev grade: FAIL — '{path}' exists but could not be read: {e}"),
                )
            }
        };
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
    // Generous char bound: the ObserveResult carries the structure tree + (inline) the rendered
    // PNG, which the model-facing path would fold. Grading is not latency-critical and needs the
    // WHOLE JSON, so we don't fold it here.
    const OBSERVE_RESULT_BOUND: usize = 16_000_000;
    let out = match acting
        .executor
        .execute_native_batch(std::slice::from_ref(&observe_call), &ws_ctx, OBSERVE_RESULT_BOUND)
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
