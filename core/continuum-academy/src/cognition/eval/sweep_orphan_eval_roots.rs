use super::*;

/// Remove sibling eval worlds that belong to no live run in this process. An
/// eval run cannot survive its process (the mind fork, the lane lease, and the
/// progress reporter all die with it), so any world under `continuum-eval/`
/// that is neither the run being provisioned nor in [`live_eval_roots`] is an
/// orphan from a killed process — re-creatable debris by construction (#312:
/// the world is a CoW clone of the checkout; nothing in it is the only copy).
/// Event-driven at the point of use — no boot hook, no background tick.
pub(super) fn sweep_orphan_eval_roots(parent: &std::path::Path, keep: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(parent) else {
        return; // parent not created yet — nothing to sweep
    };
    // Snapshot the live set and release the lock BEFORE deleting: a multi-GB
    // clone takes seconds to remove, and a concurrent provision must not queue
    // behind filesystem work.
    let live = match live_eval_roots().lock() {
        Ok(guard) => guard.clone(),
        Err(_) => return, // poisoned registry: skip the sweep, never guess at liveness
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() || path == keep || live.contains(&path) {
            continue;
        }
        if let Some(why) = unresolved_hands_keep_reason(&path) {
            tracing::warn!(root = %path.display(), reason = %why, "an eval world kept for unresolved hands is not swept");
            continue;
        }
        match std::fs::remove_dir_all(&path) {
            Ok(()) => continuum_base::probe!(
                class = "eval.workspace.orphan_swept",
                root = path.display().to_string().as_str(),
                "orphaned exam world from a killed process removed — Drop cannot run \
                 through SIGTERM, so provision-time sweep is the crash-path owner"
            ),
            Err(e) => tracing::warn!(
                root = %path.display(),
                error = %e,
                "orphaned eval world could not be removed — temp-dir debris persists"
            ),
        }
    }
}

/// RAII holder for the run's disposable world — `Drop` removes it on EVERY return
/// path (score, infra-abort, error), so a crashed exam never leaks fixtures into
/// the temp dir long-term and NEVER touches the shared checkout at all. The
/// crash path Drop cannot cover (SIGKILL/SIGTERM) is owned by
/// [`sweep_orphan_eval_roots`] at the next provision.
///
/// Once a fork's hands can act in it, the world is owned by an [`EvalWorld`], which keeps it
/// until every set of hands is confirmed ended (Astra on #5005).
pub struct EphemeralEvalRoot {
    pub(super) path: std::path::PathBuf,
    pub(super) retained: bool,
}

impl EphemeralEvalRoot {
    pub fn new(path: std::path::PathBuf) -> Self {
        if let Ok(mut live) = live_eval_roots().lock() {
            live.insert(path.clone());
        }
        Self { path, retained: false }
    }

    /// Keep this world past the run: a fork's hands in it weren't confirmed ended. It stays
    /// registered (this process's sweep spares it), and an [`UNRESOLVED_HANDS_MARKER`] records
    /// which pids weren't confirmed gone, so a later process's sweep deletes it only once each
    /// is positively absent. A restart alone proves nothing: a child needn't die with its parent.
    pub(super) fn retain_unresolved(mut self, pids: Option<Vec<u32>>) {
        let marker = UnresolvedHands { pids };
        let written = serde_json::to_string(&marker) // disk marker read by a later process's sweep — a true process boundary
            .map_err(|e| e.to_string())
            .and_then(|body| std::fs::write(self.path.join(UNRESOLVED_HANDS_MARKER), body).map_err(|e| e.to_string()));
        if let Err(e) = written {
            tracing::error!(root = %self.path.display(), error = %e, "an unresolved eval world's marker could not be written; only this process's registry keeps it from the sweep");
        }
        tracing::error!(
            root = %self.path.display(),
            pids = ?marker.pids,
            "an eval world is KEPT: a fork's hands in it were not confirmed ended, so it is not deleted under them"
        );
        self.retained = true;
    }
}

/// Written into an eval world kept because a fork's hands in it weren't confirmed ended.
pub(crate) const UNRESOLVED_HANDS_MARKER: &str = "unresolved-hands.json";

/// What [`UNRESOLVED_HANDS_MARKER`] holds: the pids not confirmed gone, or `None` when the
/// release couldn't report them (then nothing proves the world unused, and it is kept).
#[derive(Debug, Serialize, Deserialize)]
pub(super) struct UnresolvedHands {
    pub(super) pids: Option<Vec<u32>>,
}

/// Why a world must NOT be swept, or `None` when nothing recorded says so: a kept world is
/// swept only once every pid its marker names is positively absent.
pub(super) fn unresolved_hands_keep_reason(root: &std::path::Path) -> Option<String> {
    let body = match std::fs::read_to_string(root.join(UNRESOLVED_HANDS_MARKER)) {
        Ok(body) => body,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => return Some(format!("its unresolved-hands marker can't be read: {e}")),
    };
    let marker: UnresolvedHands = match serde_json::from_str(&body) {
        Ok(marker) => marker,
        Err(e) => return Some(format!("its unresolved-hands marker isn't its own shape: {e}")),
    };
    let Some(pids) = marker.pids else {
        return Some("its fork's release reported no pids, so nothing proves the world unused".into());
    };
    let present: Vec<u32> = pids
        .into_iter()
        .filter(|pid| !matches!(crate::inference::engine_residency::probe_process(*pid), crate::inference::engine_residency::ProcessProbe::Absent))
        .collect();
    (!present.is_empty()).then(|| format!("pids {present:?} are not confirmed gone"))
}

/// An eval run's disposable world together with every fork's hands that may act in it. Once
/// hands are held the world is RETAINED by default: it is deleted only after each set of hands
/// is confirmed closed. [`Self::finish`] does that in-band; a cancel or an early return drops
/// the world first, and its drop hands the same close to an owned task that outlives the
/// caller (Astra's re-review of #5005). With no hands held, nothing can act in the world, and
/// it goes as an [`EphemeralEvalRoot`] always did.
pub struct EvalWorld {
    pub(super) root: Option<EphemeralEvalRoot>,
    /// Hands not yet confirmed closed. One leaves only after its close returned.
    pub(super) hands: Vec<crate::cognition::persona_workspace::ActingHands>,
    /// Pids not confirmed gone so far, or `None` once a release couldn't report them.
    pub(super) unresolved: Option<Vec<u32>>,
    pub(super) any_unresolved: bool,
    /// This world is already the owned cleanup: if it is dropped too (its runtime shutting
    /// down), it keeps the world rather than spawning again.
    pub(super) owned_cleanup: bool,
}

impl EvalWorld {
    pub fn new(root: Option<EphemeralEvalRoot>) -> Self {
        Self { root, hands: Vec::new(), unresolved: Some(Vec::new()), any_unresolved: false, owned_cleanup: false }
    }

    /// Hold a fork's hands BEFORE they are rooted in this world.
    pub fn hold(&mut self, hands: Option<crate::cognition::persona_workspace::ActingHands>) {
        self.hands.extend(hands);
    }

    /// Close every set of hands, awaited, then delete the world, or keep it when any close
    /// is unresolved. A close that fails is logged loudly; it never replaces the measurement.
    pub async fn finish(mut self) {
        while let Some(hands) = self.hands.last() {
            let outcome = crate::cognition::persona_workspace::close_acting_context(hands).await;
            // Popped only AFTER the close returned: a cancel mid-close leaves it held, for drop.
            self.hands.pop();
            if let Err(not_closed) = outcome {
                tracing::error!(error = %not_closed, "cognition/eval could not cleanly end a fork's own hands; its world is kept");
                self.any_unresolved = true;
                self.unresolved = match (self.unresolved.take(), not_closed.unconfirmed) {
                    (Some(mut known), Some(pids)) => {
                        known.extend(pids);
                        Some(known)
                    }
                    _ => None,
                };
            }
        }
        if self.any_unresolved {
            if let Some(root) = self.root.take() {
                root.retain_unresolved(self.unresolved.take());
            }
        }
        // Otherwise `self` drops with no hands held: the root deletes itself.
    }
}

impl Drop for EvalWorld {
    fn drop(&mut self) {
        if self.hands.is_empty() {
            if self.any_unresolved {
                if let Some(root) = self.root.take() {
                    root.retain_unresolved(self.unresolved.take());
                }
            }
            return;
        }
        let rest = EvalWorld {
            root: self.root.take(),
            hands: std::mem::take(&mut self.hands),
            unresolved: self.unresolved.take(),
            any_unresolved: self.any_unresolved,
            owned_cleanup: true,
        };
        match tokio::runtime::Handle::try_current() {
            Ok(rt) if !self.owned_cleanup => {
                rt.spawn(rest.finish());
            }
            _ => {
                // No runtime to close on, or the owned cleanup itself was dropped: keep the
                // world, and say nothing proves it unused. Dropping `rest` with no hands held
                // and an unresolved verdict retains its root.
                let mut rest = rest;
                rest.hands.clear();
                rest.any_unresolved = true;
                rest.unresolved = None;
            }
        }
    }
}

impl Drop for EphemeralEvalRoot {
    fn drop(&mut self) {
        if self.retained {
            return;
        }
        if let Ok(mut live) = live_eval_roots().lock() {
            live.remove(&self.path);
        }
        if let Err(e) = std::fs::remove_dir_all(&self.path) {
            tracing::warn!(
                root = %self.path.display(),
                error = %e,
                "ephemeral eval workspace cleanup failed — temp-dir debris only, shared checkout unaffected"
            );
        }
    }
}

/// Ask an OpenAI-compatible provider what context window it ACTUALLY serves for
/// `model_id` (`/v1/models` `context_length`). `None` when unreachable or unreported —
/// callers fall back to the catalog row.
pub(super) async fn external_served_context_window(base_url: &str, model_id: &str) -> Option<u32> {
    let url = format!("{}/v1/models", base_url.trim_end_matches('/'));
    let resp = reqwest::Client::new()
        .get(&url)
        .timeout(std::time::Duration::from_secs(3))
        .send()
        .await
        .ok()?;
    let body: serde_json::Value = resp.json().await.ok()?;
    parse_provider_context_length(&body, model_id)
}

/// Pure `/v1/models` → `context_length` extraction, split from the fetch so the
/// provider-shape contract is unit-testable without a live sidecar.
pub(super) fn parse_provider_context_length(body: &serde_json::Value, model_id: &str) -> Option<u32> {
    body.get("data")?
        .as_array()?
        .iter()
        .find(|m| m.get("id").and_then(|i| i.as_str()) == Some(model_id))?
        .get("context_length")?
        .as_u64()
        .map(|n| n as u32)
}

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

/// One eval task. Both the JSONL rows and inline `tasks` deserialize into this;
/// every field is optional so an authoring typo degrades to a benign empty rather
/// than failing the whole run. A task is TEST-GRADED when it carries `test`, else
/// substring-graded against `expect`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
// Exported to the canonical protocol root (#247/#80) so the parents that embed it
// (GenomeTeachParams / MinedTask / RedactMemoryParams — all already emitting under
// protocol/typescript/) import a SIBLING instead of reaching back into the legacy
// crate bindings/ dir. A derive without an export once left this file a silently
// rotting orphan; the explicit export is the fix, the canonical path is #424.
#[ts(export, export_to = "../../../protocol/typescript/cognition/EvalTask.ts")]
pub struct EvalTask {
    /// Stable id for the task (echoed in results so a regression is identifiable).
    #[serde(default)]
    pub id: String,
    /// Per-task act→observe budget override — budget-as-data, so a gym can size
    /// patience to its task class (a mirror task needs analysis + implementation
    /// + verification; a one-liner doesn't). `None` inherits the run's budget.
    #[serde(default)]
    #[ts(optional)]
    pub max_acts: Option<u32>,
    /// The prompt posed to the persona, framed as a room message.
    #[serde(default)]
    pub prompt: String,
    /// Substring the answer must contain (case-insensitive) for descriptive tasks.
    /// Ignored when `test` is present.
    #[serde(default)]
    pub expect: String,
    /// SILENCE-graded: the task passes when she says NOTHING (a Pass, no spoken
    /// text). The speech-discipline gym's quiet half — a wake-shaped burst with
    /// nothing to answer must end in silence, not in the burst read back. `expect`
    /// is ignored when this is set.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub silence: bool,
    /// A test program appended to her extracted code and RUN; pass = exit 0. When
    /// present, this objective grade supersedes `expect`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub test: Option<String>,
    /// Language of `test` (the gym grades `rust` only). Defaults to `rust`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub lang: Option<String>,
    /// A REAL definition-of-done: a shell command run in the persona's workspace AFTER she
    /// acts (edits files with her tools). Pass = exit 0. This is what makes a task REAL vs
    /// a HumanEval toy — the grade checks the actual repo state her edits produced (e.g.
    /// `cargo test --test foo`), not code extracted from her chat answer. Supersedes
    /// `test`/`expect`; the recovery loop feeds its stdout+stderr back on failure so she
    /// iterates against the real compiler/test output until it goes green.
    #[serde(default, alias = "dodShell", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub dod_shell: Option<String>,
    /// A HELD-OUT grade: a shell command run in her workspace once `dod_shell` passes, and
    /// never shown to her. Not in the card, not in the verify loop's re-drive, not in a
    /// verdict line (a failure says only that a requirement the task did not state was not
    /// met). `dod_shell` is the definition of done she works against; this checks something
    /// she should already know without being told, which is what a learned correction is.
    /// Without it the only workspace oracle was the visible DoD, so a check of a learned rule
    /// had to be printed in the card, which taught every arm the rule. Pass = both exit 0.
    #[serde(default, alias = "heldOutShell", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub held_out_shell: Option<String>,
    /// ARTIFACT grade: a relative in-workspace path she is told to write her solution to. When
    /// set alongside `test`, the grade reads HER FILE (her hands) instead of extracting a code
    /// block from her spoken answer (her mouth), then runs the SAME harness (strip her `main`,
    /// append `test`, compile, run). This is how an ACTING persona is measured — the act→verify
    /// loop is only visible if we grade what she actually wrote + compiled, not what she narrated.
    /// The file lands in the workspace root (= core cwd, where `code/write` sandboxes writes).
    #[serde(
        default,
        alias = "solutionFile",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub solution_file: Option<String>,
    /// Task-state SETUP: a shell command run BEFORE the prompt is posed, restoring the
    /// task's initial workspace state so runs are repeatable (a `gym/mine` task re-breaks
    /// its checkout: `git checkout <commit>^ -- src/lib.rs`). Setup failure is a named
    /// infra grade, never a silent broken workspace ([[fallbacks-are-illegal-fail-loud]]).
    #[serde(default, alias = "setupShell", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub setup_shell: Option<String>,
    /// FUNCTIONAL WEB-DEV grade: structural acceptance criteria checked against what the
    /// persona's UI ACTUALLY RENDERED. When non-empty, the grade OBSERVES `target` through
    /// HER OWN eyes (the `perception/observe` eye-node path — the same purity as every other
    /// tool) and scores the element tree with `perception::scoring::grade_ui`. This measures a
    /// persona on building a UI that WORKS, on equal footing for every model: the structure tree
    /// is plain text a non-visual model reads exactly like a VLM ([[built-to-teach-lesser-tuned-intelligences-win]]).
    /// The SAME `UiScore` is her self-check, her training label, and this benchmark score.
    #[serde(default, alias = "uiChecks", skip_serializing_if = "Vec::is_empty")]
    pub ui_checks: Vec<crate::perception::scoring::UiCheck>,
    /// What to observe for a web-dev task: a workspace-relative path (absolutized to a `file://`
    /// URL against her workspace root — where `code/write` sandboxes her files) or an explicit
    /// URL (e.g. an `http://localhost` dev server she started). Defaults to `index.html`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub target: Option<String>,
    /// Fraction of `ui_checks` that must hold to PASS (`1.0` = every criterion; the fractional
    /// score always rides along in the grade line). Defaults to `1.0` — "the UI works".
    #[serde(
        default,
        alias = "uiPassThreshold",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub ui_pass_threshold: Option<f32>,
    /// Does answering this task REQUIRE tools — regardless of how it's graded? The derived
    /// default (see [`needs_tools`](Self::needs_tools)) keys off the GRADING modality: a task
    /// whose grade reads a file she wrote (`solution_file`), a workspace DoD (`dod_shell`), a
    /// pinned repo (`workspace_root`), or a rendered UI (`ui_checks`) obviously needs hands.
    /// But a MOUTH-graded task (`expect`/`test`) can ALSO require tools to PRODUCE the answer —
    /// a repo-navigation exam ("Which file defines `fn build_workspace_burst`?") is graded by a
    /// spoken substring yet is unanswerable without `code/search`/`code/read`. The derived
    /// signal misses that, so the whole gym runs speak-only and scores a SILENT 0 for every
    /// model (#208, coder-eval). This EXPLICIT declaration overrides the default: `Some(true)`
    /// forces tools on for the run; `Some(false)` pins a task speak-only even if it looks
    /// hands-graded. `None` (the common case) falls back to the grading-modality derivation.
    #[serde(default, alias = "needsTools", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub needs_tools: Option<bool>,
    /// The checkout THIS task lives in — her hands get rooted here before the task runs.
    ///
    /// The run-level `workspace_root` param pins ONE repo for a whole exam, which is right for a
    /// SWE-bench instance (one clone, one task). It is wrong for a MINED gym: `gym/mine` gives
    /// every task its own git worktree so tasks are independent and repeatable, so the root is a
    /// property of the TASK. Without this the evaluator hands her one root for all N tasks — the
    /// wrong one for every task — and she is told to fix a bug in a directory her sandboxed tools
    /// cannot reach. That is not a hard exam, it is an unrunnable one, and it is why mined gyms
    /// had never produced a number.
    ///
    /// It is DECLARED here, not applied mid-run. Rooting a persona is two operations — her hands
    /// (`root_acting_workspace`, callable any time) and her eyes (`repoint_workspace_map_if_pinned`,
    /// fork-time only, because the cycle's faculties are immutable once built). Applying just the
    /// first splits her: hands in one tree, map of another, which is the #206 shape and produces a
    /// zero that lies about the solver with no probe to show it. So a task whose root differs from
    /// the run's is REFUSED with a named infra grade telling the caller to run one eval per root —
    /// the run-level pin does both halves at fork, so that path is correct today. When the map
    /// derives its root from her hands (one source of truth), this becomes a live re-root and the
    /// refusal is deleted.
    /// [[re-rooting-a-persona-is-two-operations-moving-one-is-worse-than-moving-neither]]
    #[serde(
        default,
        alias = "workspaceRoot",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub workspace_root: Option<String>,
}

impl EvalTask {
    /// Is this task GRADED ON THE WORKSPACE (a DoD shell reads files, a
    /// solution file is collected, UI checks screenshot artifacts) rather than
    /// on her spoken answer? Decides the turn's [`TurnFraming::on_workspace`]
    /// contract — which in turn arms the discovery-saturation gate, the
    /// no-deliverable nudge, and the act-budget proprioception. Before this
    /// predicate the eval framed EVERY task as directed-speech, so none of
    /// those protections ever applied to the tasks they were built for
    /// (glass-boxed 2026-08-23: MirrorCode graded an empty src/ after a
    /// discovery-only turn the saturation gate should have interrupted).
    pub fn workspace_deliverable(&self) -> bool {
        self.dod_shell.is_some() || self.solution_file.is_some() || !self.ui_checks.is_empty()
    }

    /// Is this task graded by something that RUNS: a compiled `test`, or a workspace
    /// definition of done? The ONE predicate for "a lesson whose verdict is objective":
    /// coursework keeps only these, and the room's gym grader has an arm for each.
    pub fn has_run_oracle(&self) -> bool {
        self.test.is_some() || self.dod_shell.is_some()
    }
}

impl EvalTask {
    /// Whether answering THIS task requires the persona's hands. An explicit
    /// [`needs_tools`](Self::needs_tools) declaration wins; otherwise it's derived from the
    /// GRADING modality — a grade that reads a written file / DoD / rendered UI needs hands.
    /// The run-level decision is `any task needs tools` (tools are offered to the whole run),
    /// so a mixed gym with one repo-nav task correctly arms tools for all. See #208.
    pub fn needs_tools(&self) -> bool {
        self.needs_tools.unwrap_or_else(|| {
            self.solution_file.is_some()
                || self.dod_shell.is_some()
                || self.workspace_root.is_some()
                || !self.ui_checks.is_empty()
        })
    }

    /// CODE IS BUILT, NEVER SPOKEN. A task carrying a compile-and-run `test` grades a PROGRAM,
    /// and a program's deliverable is a FILE. If the task didn't name one, name it here — the
    /// grade then reads what her hands wrote, and the spoken-code payout stops existing.
    ///
    /// Joel, 2026-08-04, on why this is not merely a measurement fix: *"any benchmark, at least
    /// for code, that has them speak code is actually more of a problem for learning persona with
    /// minds. It's going to train them to think speak code (or anything that needs an action) is
    /// building projects. That's exactly the opposite of our goals for first class citizens."*
    ///
    /// That is the load-bearing argument. Our graded episodes do not stop at a scoreboard — they
    /// enter the SAME experience loop as lived ones (engram → sentinel → curriculum → genome,
    /// [[one-experience-loop-benchmark-lessons-are-engrams-dream-sentinels-train-them]]). So a
    /// grader that pays out for a fence does not just mis-measure the fence→act gap, it TEACHES
    /// it, and the layer promoted on that reward carries "describing is doing" into every room —
    /// not just the exam. 404 of 458 gym tasks paid out that way when this was written.
    ///
    /// Doing it at LOAD is the compression: the alternative already existed as
    /// `hard-rs-acted.jsonl`, a hand-forked twin of `hard-rs.jsonl` differing only by this
    /// preamble and this filename. One rule in the loader replaces N forked corpora that would
    /// each drift. A gym author still WINS by naming `solution_file` (and its prompt) explicitly;
    /// this only ensures no code task can be graded by mouth because someone forgot.
    ///
    /// `expect`-graded knowledge tasks are untouched — answering a question IS speaking, and
    /// nothing is confused about that. The rule keys on `test`: only a task that compiles and
    /// runs code has a program as its deliverable.
    ///
    /// `pub(crate)` because THIS is the one derivation of a code task's artifact name:
    /// the card writer (`benchmark/import`) must run the same rule before composing a work card, or the card
    /// tells the citizen nothing about WHERE to write while the grade reads the derived path
    /// (glass-boxed 2026-08-15: a frontier-rs claimer inventing `swe/benchmarks/.../lib.rs`).
    pub fn require_hands_for_code(&mut self) {
        if self.test.is_none()
            || self.solution_file.is_some()
            || self.dod_shell.is_some()
            || self.workspace_root.is_some()
        {
            return;
        }
        let ext = match self.lang.as_deref().unwrap_or("rust") {
            "rust" | "rs" => "rs",
            "python" | "py" => "py",
            "typescript" | "ts" => "ts",
            "javascript" | "js" => "js",
            "go" => "go",
            "c" => "c",
            "cpp" | "c++" => "cpp",
            other => other,
        };
        let safe: String = self
            .id
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        let file = format!("sol_{safe}.{ext}");
        // The prompt must ASK for what the grade READS — a derived file behind an unchanged
        // "return the complete function" prompt would score an honest 0 for the wrong reason
        // (she answered the question she was asked). Same wording the hand-authored acted gym
        // used, so migrated tasks read identically to ones an author wrote by hand.
        self.prompt = format!(
            "Implement the following, and VERIFY it before finishing. Write your solution to \
             the file `{file}` (a relative path in your workspace) using your write tool, then \
             compile and run it with your shell tool against a few checks you devise; if it \
             fails, read the compiler/test output and fix it. Only finish once it actually \
             compiles and runs. Leave the final, working code in `{file}`.\n\n{}",
            self.prompt
        );
        self.solution_file = Some(file);
    }
}

/// A gene to page in for the candidate arm of an A/B. The persona runs the eval
/// once on the base model (gene paged OUT), then again with this gene paged IN;
/// the result carries both pass-rates and their `lift`. The gene must already be
/// REGISTERED with the serving endpoint (the adapter resolves `name`/`path` → the
/// server's LoRA load-index); continuum holds the handle, the custodian owns the
/// bytes ([[model-endpoint-fabric-adapter-router]]).
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct EvalGene {
    /// The gene's name — how the serving endpoint catalogs it (`/lora-adapters`).
    pub name: String,
    /// The gene's on-disk path (the custodian-owned GGUF/safetensors LoRA). The
    /// adapter matches this against the registered catalog when `name` doesn't.
    #[serde(default)]
    pub path: String,
    /// Influence dial in [0,1+] — 0 = base, 1 = full gene. The page-in is analog:
    /// the per-request scale rides into `"lora":[{id,scale}]`. Defaults to 1.0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(optional, type = "number")]
    pub scale: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct CognitionEvalParams {
    /// Which persona to put through the gym — a full UUID, an 8-char short-id, or a
    /// name. Resolved against the live roster before anything runs, so a garbage or
    /// unknown reference fails loud here instead of dying later as a misleading
    /// "not assembled at spawn". Must be spawned (have a live `WorkspaceCycle`) —
    /// the eval drives her real cognition, not a stand-in.
    pub persona_id: crate::identity::PersonaRef,
    /// Optional gene to MEASURE: when set, the eval runs base vs gene as an A/B and
    /// reports the `lift`. When omitted, a single pass on whatever genome is
    /// currently paged in (base, by default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub gene: Option<EvalGene>,
    /// Room context the eval turns are scoped to. Omit for the nil room.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub room_id: Option<String>,
    /// Inline tasks. When set, takes precedence over `eval_set`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub tasks: Option<Vec<EvalTask>>,
    /// Path to a JSONL eval set (one `EvalTask` per line). Defaults to the committed
    /// `docs/genome/coder-eval.jsonl` when neither this nor `tasks` is given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub eval_set: Option<String>,
    /// Measure THIS base model through the full loop, in its OWN ephemeral lane, without
    /// re-homing or disturbing the living persona (#59). The clean same-model control:
    /// hold the harness fixed, vary the model. Ignored when a `gene` is set (a gene names
    /// its own forged base). None → fork onto her live lane as before. Must be a loadable
    /// id from `ai/inference/models`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub base_model_id: Option<String>,
    /// Team mode: with `Some(n>=1)`, add a REVIEWER teammate (a fresh fork of the SAME
    /// persona/model) that reviews + corrects the writer's answer before grading — the
    /// undeniable team proof (same model, same tasks, +1 teammate → does coordination lift?).
    /// None/0 = solo. First slice supports one reviewer; live single-pass path only (no gene,
    /// no base_model_id).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(optional, type = "number")]
    pub reviewers: Option<u32>,
    /// Max act→observe cycles per task before it counts as unfinished. Default 32
    /// (see DEFAULT_MAX_ACTS); a task row's own max_acts overrides per task.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(optional, type = "number")]
    pub max_acts: Option<u32>,
    /// Agentic-recovery budget: how many times a FAILED test-graded task is handed its
    /// compiler/test output to fix before it scores a miss. Default [`MAX_FAIL_RETRIES`].
    /// Set `0` for the ONE-SHOT baseline (what plain inference gets on the same
    /// weights) — so a `0` vs `N` A/B on the identical model+benchmark measures exactly the
    /// edge our agentic loop adds, repeatably, from one param.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(optional, type = "number")]
    pub max_retries: Option<u32>,
    /// Decoding temperature pin for the WHOLE run. **Omitted → her LIVED
    /// sampling** (the engineerable form — Joel 2026-08-23: "the old form is
    /// useless"): greedy 0.0 measured pathological on long-context agentic
    /// work (16k-token think spirals hitting the reply ceiling exactly, 4+min
    /// decodes) while every serious agentic harness samples. Pass
    /// `Some(0.0)` to opt INTO the old byte-reproducible greedy pin. A run's
    /// number is only comparable to runs at the same pin either way.
    #[serde(default)]
    #[ts(optional)]
    pub temperature: Option<f32>,
    /// Free-text label for THIS run, written to the progress ledger so a trend
    /// line is readable: "baseline", "taught show-output", "genome v2", etc.
    /// The ledger is how you "mark improvement as you go" — every eval leaves a
    /// dated, labelled, test-anchored mark at `~/.continuum/progress/<id>.jsonl`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub note: Option<String>,
    /// Fire-and-poll (#86): when true, the eval is spawned DETACHED — `run` returns a job
    /// handle immediately (so it survives the IPC client disconnecting on a long acting run)
    /// and the REAL result lands in the progress ledger (`~/.continuum/progress/<persona>.jsonl`),
    /// which the caller polls by `note`. Default/false = run inline and block as before.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub detach: Option<bool>,
    /// Handle for a detached run (#86 fire-and-poll). Minted by the command when omitted;
    /// the progress-ledger row carries it, and `cognition/eval-status` polls by it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub run_id: Option<String>,
    /// Repo-work seam (#49): pin the persona's file engine at this directory (e.g. a SWE-bench
    /// target-repo clone) BEFORE her cycle, by invoking `code/create-workspace` through HER OWN
    /// identity-bearing executor. Deterministic rooting — no reliance on the model choosing to
    /// call create-workspace itself. `None` → she uses the default root (core cwd) exactly as
    /// before, so this is opt-in with zero change to every existing eval path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub workspace_root: Option<String>,
    /// Glass-box seam (task #14): if set, wrap the eval-fork's cognition in the JSONL
    /// [`JsonlWorkspaceCaptureSink`] so every tick's bids + DECISION + timings land in
    /// `<capture_dir>/<persona_id>.jsonl` — makes a MEASURED run inspectable (did she Act or
    /// Respond? which tool? why 0 edits?) without touching her live mind. `None` → Noop capture
    /// exactly as before. Opt-in: zero change to every existing eval path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub capture_dir: Option<String>,
    /// LEARN mode (the exam as a legitimate teacher). When true, after the exam the
    /// redacted lesson of each task — "I was asked X; I solved it / did not (grade Y)" with
    /// the held-out answer key scrubbed — is admitted into the LIVING persona via
    /// `admit_reflection`, so she carries the *experience* forward and gets better across
    /// retakes WITHOUT ever memorizing the crib sheet. The exam still runs on the fork
    /// (#59: living persona never frozen/degraded); only the redacted lesson crosses back.
    /// This is what makes "learn from the exam" honest — provably clean, encouraged. Default
    /// false = pure measurement (the discarded fork teaches nothing, as before). Single-pass
    /// only in this slice (ignored under a `gene` A/B). [[redaction-makes-exam-learning-honest-so-encourage-it]]
    /// No `Default` on [`LearningPolicy`] — every Rust caller must state which this run is.
    #[serde(default = "LearningPolicy::wire_default")]
    #[ts(type = "boolean", optional)]
    pub learn: LearningPolicy,
    /// REPRODUCIBLE-ABSOLUTE mode (#207): suppress the fork's episodic recall so a
    /// self-contained proctored exam scores the SAME absolute number run-to-run. Each eval
    /// re-forks from her LIVING durable engram store, which grows as she lives between runs;
    /// injecting it into a self-contained task (HumanEval, a repo-nav question, a from-scratch
    /// UI build) recalls unrelated room chatter that drifts the prompt — noise AND
    /// nondeterminism, unrelated to sampling (greedy is already pinned) or serving. `true`
    /// omits the recall faculty from the fork; system + task + grounding (roster/doctrine/
    /// workspace-map) remain. The LIFT (base vs gene in one fork) is reproducible either way;
    /// this pins the ABSOLUTE baseline so today's number compares to last week's. `None`/false =
    /// memories intact (the natural persona), unchanged for every existing path. NOT a life
    /// knob — a benchmark control, sibling of the greedy-temperature and directed-turn pins.
    /// [[eval-reproducibility-is-two-tier-lift-controlled-absolute-drifts]]
    #[serde(
        default,
        alias = "suppressRecall",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub suppress_recall: Option<bool>,
    /// HELP ARM (rung one of room collaboration, 2026-08-24): when true and the
    /// eval is room-scoped, the exam condition DECLARES that asking peers is
    /// legal — the task prompts say so, and a listener folds live room speech
    /// from OTHERS into the fork's working memory so she perceives replies
    /// between acts. Exchanges are receipts in the transcript; score under this
    /// arm is reported as OURS+help, never conflated with solo. Default off.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub help: Option<bool>,
}
