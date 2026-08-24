//! `apply_act`: execute ONE `Act` verdict — run the calls through the persona's
//! hands, admit the outcome as an Episodic engram (the result becomes memory), and
//! return the observation text so the caller can fold it into the next perception.
//! Extracted verbatim from `act_observe` (pure code-motion, #386 decomposition).

use uuid::Uuid;

use crate::ai::types::{ToolCall, ToolResult};
use crate::cognition::context_budget::ContextBudget;
use crate::cognition::workspace::WorkspaceCycle;

use super::observation::{extract_paths, ActOutcome, ActStatus, Observation, ToolOutput, ToolVerb};
use super::perception::{all_calls_already_satisfied, is_redundant_orientation};
use super::settle::now_ms;

/// Recall salience for an action-observation receipt (#166). Below the neutral
/// default (0.5) so genuine findings/facts win recall, but well above zero so the
/// receipt stays recallable for "what did I just do" when nothing better matches.
const PROPRIOCEPTION_RECALL_SALIENCE: f32 = 0.25;

/// Execute ONE `Act` verdict: run its calls through the persona's hands, admit
/// the outcome as an Episodic engram (the result becomes memory), and return the
/// observation text so the caller can fold it into the next perception.
///
/// `room_id` is the room THIS act is about — passed per-call because one mind is
/// in many rooms at once (a persona, like a Claude tab, is in multiple rooms
/// simultaneously); the [`ActingBody`](crate::cognition::workspace::ActingBody) itself is
/// room-agnostic.
///
/// Returns [`ActOutcome`] — `NoHands` when the mind has no hands, `ExecutorError`
/// when the tool batch failed at the channel level (both formerly a bare `None`,
/// never a fabricated success), or `Acted { acts }` carrying the TYPED
/// [`Observation`]s (call + result correlated by `tool_use_id == call.id`, verb
/// and paths precomputed). The admission is best-effort: an un-admitted
/// observation still re-enters via the typed acts, so re-perception works
/// regardless; admission is what makes it durable long-term memory.
/// Commands that run long enough that BLOCKING the turn on them starves the mind — they
/// are DISPATCHED in the background (fire-and-poll) and stream their result back into
/// working memory via the dispatch listener when they finish. Seed set; the durable home is
/// a per-command `long_running` flag on the command spec (#86). Matched on the slash form
/// (models may emit the underscore form — normalize before calling).
fn is_long_running(command: &str) -> bool {
    matches!(
        command,
        "code/cargo/check"
            | "code/cargo/test"
            | "code/cargo/build"
            | "cognition/full-evaluate"
            | "forge/train"
    )
}

/// Build one typed [`Observation`] per demoted call for a short-circuit path
/// (already-satisfied / redundant-orientation): NO tool executed, so the OUTPUT
/// carries the nudge as its result (the perception the mind gets back instead of
/// a re-execution) and the STATUS names why it was demoted. `verb`/`paths` are
/// still precomputed from the call. Non-empty ⇒ `produced_an_act()` stays true,
/// so the settle loop treats this exactly as the old `Some(nudge)` return did.
fn short_circuit_acts(calls: &[ToolCall], nudge: &str, status: ActStatus) -> Vec<Observation> {
    calls
        .iter()
        .map(|c| Observation {
            call: c.clone(),
            output: ToolOutput {
                result: ToolResult {
                    tool_use_id: c.id.clone(),
                    content: nudge.to_string(),
                    spill_handle: None,
                    is_error: None,
                },
                verb: ToolVerb::classify(&c.name),
                paths: extract_paths(&c.input),
            },
            status: status.clone(),
        })
        .collect()
}

/// One settle chain's causal thread — owned by the DRIVER of the chain
/// (`drive_to_settle`, or one turn-frame), passed by reference through
/// `settle_step` into `apply_act`. Holds the engram id of the most recently
/// ADMITTED act observation so the next act in the same chain can carry a
/// `CausedBy` edge to it (CAUSAL-MEMORY-GRAPH.md §3a). One concern, one
/// chain; the driver drops it when the turn settles, so an edge can never
/// cross turns or rooms by construction — the wire is the scope, never an
/// inference over timestamps.
#[derive(Default)]
pub struct ActChain(std::sync::Mutex<Option<Uuid>>);

impl ActChain {
    /// A chain with no recorded antecedent — its first act links to nothing.
    /// Prefer [`rooted_in`](Self::rooted_in): a chain that knows what caused it is
    /// what makes "which acts were done FOR this card" answerable.
    pub fn new() -> Self {
        Self::default()
    }

    /// A chain rooted in whatever CAUSED the turn (CAUSAL-MEMORY-GRAPH.md §3a) — the
    /// stimulus engram for a real arrival, nothing for an ambient or synthetic burst.
    ///
    /// Seeding rather than special-casing is the whole trick: the write site already
    /// links each act to `prior()`, so rooting the chain makes the FIRST act link to
    /// its trigger through the same line of code. No new branch, no second rule, and
    /// the thread has a head instead of starting mid-air.
    ///
    /// Takes the whole [`Cause`] rather than a pre-extracted id so the decision about
    /// what counts as a root lives in ONE place (`Cause::root`) instead of at every
    /// driver that builds a chain.
    pub fn rooted_in(cause: &crate::cognition::workspace::Cause) -> Self {
        Self(std::sync::Mutex::new(cause.root()))
    }

    /// The CAUSE of whatever act comes next in this chain: the latest admitted act
    /// engram, or — before any act has run — the trigger the chain was rooted in.
    /// `None` only when the chain has no antecedent at all.
    pub fn prior(&self) -> Option<Uuid> {
        *self.0.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn advance(&self, id: Uuid) {
        *self.0.lock().unwrap_or_else(|p| p.into_inner()) = Some(id);
    }
}

pub async fn apply_act(
    cycle: &WorkspaceCycle,
    calls: &[ToolCall],
    intent: &str,
    room_id: Uuid,
    chain: &ActChain,
) -> ActOutcome {
    // no hands → cannot act (and tools were never offered)
    let Some(body) = cycle.acting() else {
        return ActOutcome::NoHands;
    };

    // Repeat-perception (proprioception, content-driven — NOT an agentic counter).
    // If this exact batch was ALREADY carried out this settle, its result is already
    // in working memory. Re-running it burns a tool round-trip + a redundant (content-
    // deduped, so no-op) engram and returns byte-identical perception — off which a
    // greedy instruct model re-emits the identical `Act` forever. The `[action #n]`
    // stamp shift was supposed to break this and does not (see
    // `all_calls_already_satisfied`). So do NOT re-execute: record an EXPLICIT
    // "already satisfied" trace so the redundancy is PERCEIVED rather than merely
    // present, and let the caller re-perceive. The trace states ONLY the fact — it
    // must not privilege answering over a DIFFERENT act (the first mined exam showed
    // the earlier "I should ANSWER the question now" phrasing being obeyed literally:
    // she settled with a diagnosis instead of trying the repair edit). Context
    // hygiene, not cognition steering; [[no-hardcoded-heuristics-to-steer-cognition]].
    // ESCALATING loop-awareness for the SHORT-CIRCUIT paths, mirroring the executed
    // path's `max_repeat` warning (line ~546). A satisfied/redundant call is demoted
    // (never re-executed) and previously recorded a byte-IDENTICAL static nudge every
    // tick. `record_fact` doesn't dedup but the recency window is capacity-bounded, so
    // identical-nudge spam EVICTS the useful result receipt and leaves a window of clones
    // — and off unchanged perception a greedy (temp-0) model re-emits the identical call
    // FOREVER (#206, glass-boxed: `commands/help(code/write)` ×54, the "already ran" nudge
    // fired 104× and never broke the loop). Bumping the DURABLE fingerprint counter here
    // and embedding the count makes each demotion DISTINCT and monotonically climbing — the
    // perception genuinely shifts every tick, which is what lets cognition move on. Only the
    // short-circuit branches (which early-return, never reaching line ~546) call this, so
    // the executed path's own bump is never double-counted. Honest proprioception, never a
    // steer toward a specific next act ([[no-hardcoded-heuristics-to-steer-cognition]],
    // [[repetition-brick-fires-but-does-not-break-the-loop]]).
    let bump_repeat = || {
        calls
            .iter()
            .map(|c| {
                let fp = c.loop_fingerprint();
                body.working_memory.note_action_fingerprint(&fp)
            })
            .max()
            .unwrap_or(0)
    };

    /// The ORIENTATION counter, keyed by CLASS rather than by `name|args`.
    ///
    /// `is_redundant_orientation` is deliberately class-based — its own doc says demoting
    /// "by CLASS + prior-receipt (ignoring args entirely) is immune to that jitter". The
    /// DETECTOR learned that lesson; the COUNTER did not. `bump_repeat` fingerprints
    /// `name|args`, so every jittered variant is a fresh key returning 1.
    ///
    /// Measured on sympy-21379, all 8 orientation calls of one run:
    ///   commands/list({"filter":"code"}) ×2, commands/list({}), commands/list({"filter":"sympy"}),
    ///   code/tree({"path":"."}), code/tree({include_hidden,max_depth,path:"sympy"}),
    ///   commands/help({"name":"code/read"}), commands/help({"name":"code/edit"})
    /// Nearly all distinct → the nudge read "I have now run orientation 1 times this
    /// concern" EVERY time. Byte-identical perception off a greedy decoder is a fixed
    /// point, which is exactly the #206 failure the escalation was built to break —
    /// reintroduced through the argument axis.
    ///
    /// One stable key makes the count climb across variants, so each demotion genuinely
    /// shifts perception. Still a FACT about her own history, never a steer
    /// ([[repetition-brick-fires-but-does-not-break-the-loop]], [[discovery-loop-broken-by-escalating-short-circuit-nudge]]).
    const ORIENTATION_FINGERPRINT: &str = "orientation|<class>";
    let bump_orientation_repeat = || {
        body.working_memory
            .note_action_fingerprint(ORIENTATION_FINGERPRINT)
    };

    // EVERY re-injection bound in this act comes from her LIVE served window, never a
    // constant — an unknown window folds NOTHING rather than inventing a number
    // ([[never-hardcode-a-context-window-4k-defaults-destroy-the-moe-thesis]]).
    let budget = cycle
        .model_loadout()
        .map(|(_, w)| ContextBudget::from_window(w))
        .unwrap_or_else(ContextBudget::unknown);
    // WM clips inside its OWN record path (it owns its truncation), so it needs the live
    // window too — pushed from here, the seam that has the cycle. Re-pushed every act so a
    // lane relaunch at a different `-c` is picked up without a restart.
    if let Some((_, w)) = cycle.model_loadout() {
        body.working_memory.set_served_window(w);
    }
    let fold = Some(budget.echoed_arg_chars());
    let recent = body.working_memory.recent();
    if all_calls_already_satisfied(&recent, calls, fold) {
        let names = calls
            .iter()
            .map(|c| {
                let args = serde_json::to_string(&c.input).unwrap_or_else(|_| "{}".to_string());
                format!("{}({})", c.name, args)
            })
            .collect::<Vec<_>>()
            .join(", ");
        let n = bump_repeat();
        let nudge = format!(
            "I have now issued {names} {n} times this turn — the result is already in my \
             working memory above, and re-running the identical call returns nothing new. \
             Repeating it will not progress; whatever I do next must be something DIFFERENT: \
             a different action, or an answer built from what I already have."
        );
        body.working_memory.record_fact(&nudge);
        crate::probe!(
            class = "persona.act.repeat_short_circuited",
            persona = %body.persona_name,
            room_id = %room_id,
            calls = calls.len(),
            "identical act already satisfied this turn — recorded already-satisfied proprioception, skipped re-execution"
        );
        // Each demoted call becomes a typed act whose OUTPUT is the nudge (the
        // perception the mind gets back instead of a re-execution) and whose
        // STATUS names the short-circuit. produced_an_act() stays true, so the
        // settle loop treats this exactly as the old `Some(nudge)` did.
        let acts = short_circuit_acts(calls, &nudge, ActStatus::AlreadySatisfied { repeat: n });
        return ActOutcome::Acted { acts };
    }

    // Redundant-orientation demotion (Joel-approved "demote discovery at the seam",
    // 2026-07-16). `commands/help`/`commands/list` only RE-LIST the tool surface the
    // mind already carries; they never touch the workspace. The FIRST orientation per
    // concern is honest — once a discovery receipt is already in the concern, another
    // is the act-pressure filler the glass box exposed (1855/3288 live tool calls were
    // this; nine straight `commands/help` turns while the answer sat ready in prose).
    // Demote it exactly as the repeat guard above does: do NOT execute (no catalog
    // re-dump, no room receipt), record the redundancy as proprioception, and let the
    // mind re-perceive with the fact present. A CLASS distinction (orientation is not
    // settlement), never a steer toward a specific next act — the nudge offers BOTH a
    // real action and an answer, privileging neither ([[no-hardcoded-heuristics-to-steer-cognition]]).
    if is_redundant_orientation(&recent, calls) {
        let names = calls
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let n = bump_orientation_repeat();
        let nudge = format!(
            "I have now run orientation ({names}) {n} times this concern — my tool menu and \
             the workspace map are already in my working memory above, and running it again \
             returns the same survey and changes nothing. My next move must be something \
             DIFFERENT: read a SPECIFIC file, make an edit, run something, or answer from \
             what I have."
        );
        body.working_memory.record_fact(&nudge);
        crate::probe!(
            class = "persona.act.redundant_orientation",
            persona = %body.persona_name,
            room_id = %room_id,
            calls = calls.len(),
            "orientation call with a discovery receipt already in the concern — recorded redundant-orientation proprioception, skipped re-execution"
        );
        let acts = short_circuit_acts(calls, &nudge, ActStatus::RedundantOrientation { repeat: n });
        return ActOutcome::Acted { acts };
    }

    let ctx = crate::cognition::tool_executor::ToolExecutionContext {
        persona_id: body.persona_id,
        persona_name: body.persona_name.clone(),
        // Session is the EPHEMERAL connection instance and is NEVER load-bearing
        // for where an action lands (per IDENTITY-SCOPE-PEER-LIVENESS-MODEL §A.5).
        // The room is the context the action scopes to.
        session_id: Uuid::nil(),
        context_id: room_id,
        caller_context: serde_json::Value::Null,
        persona_config: crate::cognition::tool_executor::PersonaMediaConfigLite {
            auto_load_media: false,
            supported_media_types: Vec::new(),
        },
    };

    // Long-running commands are DISPATCHED in the background (fire-and-poll): the turn never
    // blocks on a compile/train/eval, and the result streams back into working memory via the
    // dispatch listener when it lands. Requires the core executor (a live persona's hands
    // expose it; harnesses don't → everything runs synchronously). No long-running calls —
    // the overwhelmingly common case — means `fg_calls == calls` and this is inert.
    let mut bg_notes: Vec<String> = Vec::new();
    // The long-running calls that were dispatched (not run synchronously) — retained so a
    // pure-background batch (no foreground calls) can still emit a typed act, keeping
    // `produced_an_act()` true exactly as the old `Some(bg_notes)` return did.
    let mut dispatched_calls: Vec<ToolCall> = Vec::new();
    let fg_calls: Vec<ToolCall> = match body.executor.command_executor() {
        Some(exec) => {
            let mut fg = Vec::with_capacity(calls.len());
            for call in calls {
                let cmd = call.name.replace('_', "/");
                if is_long_running(&cmd) {
                    let handle = exec.dispatch_background(cmd.clone(), call.input.clone(), None);
                    dispatched_calls.push(call.clone());
                    body.working_memory.record_dispatch_event(
                        handle,
                        &cmd,
                        "dispatched — running",
                        crate::cognition::working_memory::DispatchStatus::Running,
                    );
                    bg_notes.push(format!(
                        "I dispatched {cmd} in the background (handle {handle}). I should NOT wait \
                         on it — the result will appear in my working memory when it completes, and \
                         I can carry on meanwhile."
                    ));
                    crate::probe!(
                        class = "persona.act.dispatched_background",
                        persona = %body.persona_name,
                        command = %cmd,
                        "long-running command dispatched fire-and-poll; result streams back to working memory"
                    );
                } else {
                    fg.push(call.clone());
                }
            }
            fg
        }
        None => calls.to_vec(),
    };

    // #186 compass: the hands are moving — fire the Act axis on the live glass-box the
    // instant a real tool batch executes (skip an empty foreground batch: a
    // background-only dispatch already lit its own path). Pure observability.
    if !fg_calls.is_empty() {
        cycle.note_acting();
    }
    // BOUNDED (2026-08-24): this await was the last UNGUARDED hang in the act
    // chain — inference has the stream-liveness ladder, but a wedged IPC
    // socket or a runaway command could hold this forever with NO propagated
    // error. Measured: bitwise ran 03:51→05:51 in silence until the 2h
    // GLOBAL backstop fired as an infra fault. This per-act ceiling converts
    // that into a 30-min act-level ERROR OBSERVATION she perceives and can
    // react to. Generous by design (a real build can take many minutes; the
    // shell window hands back handles long before this) — firing means a
    // substrate hang, and the receipt says so.
    const TOOL_BATCH_CEILING: std::time::Duration = std::time::Duration::from_secs(30 * 60);
    let batch_result = tokio::time::timeout(
        TOOL_BATCH_CEILING,
        body.executor
            .execute_native_batch(&fg_calls, &ctx, budget.result_fold_chars()),
    )
    .await
    .unwrap_or_else(|_| { // timeout elapsed = the hang this bound exists to convert; the closure builds the honest error
        crate::probe!(
            class = "persona.act.tool_batch_hung",
            ceiling_s = TOOL_BATCH_CEILING.as_secs(),
            tools = fg_calls.len(),
            "tool batch exceeded the act ceiling with no result and no error —              substrate hang converted to a perceptible act error (find the              unpropagated await beneath)"
        );
        Err(crate::cognition::tool_executor::ToolError::ExecutionFailed {
            tool: "batch".to_string(),
            underlying: format!(
                "tool batch hung past {}s with no result — the substrate lost this \
                 act; the workspace may hold partial effects",
                TOOL_BATCH_CEILING.as_secs()
            ),
        })
    });
    let outcome = match batch_result {
        Ok(o) => o,
        Err(e) => {
            // Fail loud-ish: the hand could not run. Abstain — do NOT synthesize a
            // result the mind would then "remember" as fact ([[fallbacks-are-illegal-fail-loud]]).
            // Typed as `ExecutorError` (was a bare `None`, indistinguishable from
            // no-hands) so a future backstop can tell a broken hand from an absent one.
            tracing::warn!(
                persona = %body.persona_name,
                error = %e,
                "act→observe: tool batch failed; abstaining (no fabricated outcome)"
            );
            return ActOutcome::ExecutorError {
                calls: calls.to_vec(),
                message: e.to_string(),
            };
        }
    };

    // Form the observation: what she did and what came back. First person, because
    // this is the persona observing her OWN hands — the engram reads like a memory
    // of acting, not a log line.
    // TWO renderings, one per memory channel (the universal collapse/expand primitive,
    // PX side): `observation` is the FULL trace for the RECENCY channel (working memory
    // keeps the latest whole so the mind can act on what it just fetched); `recall_observation`
    // is the COLLAPSED reference for the EPISODIC engram that RECALL re-injects on later turns.
    let mut observation = String::new();
    let mut recall_observation = String::new();
    // The TYPED acts this batch produced — the value that replaces the flattened String.
    // Each carries the call (INCLUDING `.id`), the id-correlated ToolResult, the precomputed
    // verb, and the typed paths, so no downstream consumer re-parses the receipt string.
    let mut acts: Vec<Observation> = Vec::new();
    // Loop-awareness (experience structure): fingerprint each call (name+args) so the mind
    // perceives when it is RE-ISSUING an identical call. The result HEAD varies turn to turn,
    // so `entries`/`#seq` alone make a repeat look "new"; the fingerprint keys on the call.
    let mut max_repeat = 0usize;
    for call in fg_calls.iter() {
        let fp = format!(
            "{}|{}",
            call.name,
            serde_json::to_string(&call.input).unwrap_or_default()
        );
        max_repeat = max_repeat.max(body.working_memory.note_action_fingerprint(&fp));
        // The TYPED act is the SINGLE source now (Step 5). Correlate the result by
        // `tool_use_id == call.id` (NOT the positional `.get(i)` the old string render
        // used) — id-correlation is immune to the fg-vs-original index hazard (invariant
        // 5). Both channel strings are then rendered ONCE, from the act, via the pure
        // `render_recency`/`render_recall` helpers (which own the humanize/bound/args
        // shaping — the card-0a4c0648 code-as-code decode and the #165 bound live there):
        // byte-identical to the old inline `format!`, but render is now a pure function of
        // the Observation so no consumer re-derives structure from prose.
        let typed_result = outcome
            .results
            .iter()
            .find(|r| r.tool_use_id == call.id)
            .cloned()
            .unwrap_or_else(|| ToolResult {
                tool_use_id: call.id.clone(),
                content: "(no result returned)".to_string(),
                is_error: None,
                spill_handle: None,
            });
        // PUSHED SHELL COMPLETION, receive side (2026-08-24): a `code/shell` whose
        // inline window elapsed hands back a RUNNING handle. Register that handle as a
        // dispatch NOW so the exit fold's command:completed event (shell_session.rs)
        // has a label to fold against — dispatch_listener drops completions for
        // unregistered handles. Without this she must remember to poll; with it the
        // finished build lands in her working memory like any dispatched background job.
        if call.name.replace('_', "/") == "code/shell" && typed_result.is_error != Some(true) {
            if let Ok(resp) = serde_json::from_str::<crate::code::shell_types::ShellExecuteResponse>(
                &typed_result.content,
            ) {
                if resp.status == crate::code::shell_types::ShellExecutionStatus::Running {
                    if let Ok(handle) = uuid::Uuid::parse_str(&resp.execution_id) {
                        let label: String = call
                            .input
                            .get("command")
                            .and_then(|v| v.as_str())
                            .unwrap_or("code/shell") // label only — the handle is the key
                            .chars()
                            .take(80)
                            .collect();
                        body.working_memory.record_dispatch_event(
                            handle,
                            &format!("code/shell: {label}"),
                            "handle handed back — still running",
                            crate::cognition::working_memory::DispatchStatus::Running,
                        );
                    }
                }
            }
        }
        let obs = Observation {
            call: call.clone(),
            output: ToolOutput {
                result: typed_result,
                verb: ToolVerb::classify(&call.name),
                paths: extract_paths(&call.input),
            },
            status: ActStatus::Executed,
        };
        observation.push_str(&obs.render_recency(intent, &budget));
        recall_observation.push_str(&obs.render_recall(intent));
        // (The canvas feed publishes at the tool-executor seam, PRE-fold — a
        // flood-sized ObserveResult here is already a spilled preview that no
        // longer parses. See CommandToolExecutor::execute_native_batch.)
        acts.push(obs);
    }
    // A pure-background batch (every call was long-running → dispatched, `fg_calls`
    // empty) still counts as an act — one typed Observation per dispatch so
    // `produced_an_act()` stays true, matching the old `Some(bg_notes)` return.
    if acts.is_empty() && !dispatched_calls.is_empty() {
        for call in &dispatched_calls {
            acts.push(Observation {
                call: call.clone(),
                output: ToolOutput {
                    result: ToolResult {
                        tool_use_id: call.id.clone(),
                        content: "dispatched — running in background".to_string(),
                        spill_handle: None,
                        is_error: None,
                    },
                    verb: ToolVerb::classify(&call.name),
                    paths: extract_paths(&call.input),
                },
                status: ActStatus::Executed,
            });
        }
    }
    // #243 RECEIPT RADIATION: every executed act reaches the room's transcript
    // as a collapsed receipt ("Ran 4 commands ›", the Claude-iOS pattern) —
    // this is the ONE choke point every hand action passes through (live,
    // directed, agent/solve), so publishing here covers them all. The chat
    // projection (`positron_source::apply_act`) folds each into
    // `ChatViewState.acts`; clients render, collapse, expand. PURE
    // OBSERVABILITY — nothing on the decision path reads these events, and a
    // handless/mock executor (no `command_executor`) simply radiates nothing.
    // An act with no ROOM has no transcript home: headless agent/solve runs pass
    // `Uuid::nil()` (solve.rs), and radiating those stole the single-room chat
    // projection onto a phantom room (live-proven 2026-08-12: the first real
    // receipt cleared academy's view). Skip until solves thread their bench
    // room (#329's per-run rooms make every solve act a room act).
    if room_id.is_nil() {
        // fall through to the working-memory record below — the receipt is
        // transcript-only observability; her own proprioception is unaffected.
    } else if let Some(bus) = body
        .executor
        .command_executor()
        .and_then(|e| e.message_bus())
    {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        for obs in &acts {
            // The human OBJECT of the act: the first extracted path (reads,
            // writes, edits), else the shell command head, else empty —
            // honest-thin, never dumped raw JSON args.
            let summary = obs
                .output
                .paths
                .first()
                .map(|p| p.display().to_string())
                .or_else(|| {
                    obs.call
                        .input
                        .get("command")
                        .and_then(|c| c.as_str())
                        .map(|c| c.chars().take(80).collect::<String>())
                })
                .unwrap_or_default();
            let update = crate::ipc::positron_source::PersonaActUpdate {
                act_id: Uuid::new_v4(),
                room_id,
                actor_id: body.persona_id,
                actor_name: body.persona_name.clone(),
                tool: obs.call.name.clone(),
                summary,
                ok: obs.output.result.is_error != Some(true),
                timestamp: now_ms,
            };
            match serde_json::to_value(&update) {
                Ok(payload) => bus.publish_async_only("persona:act", payload),
                Err(e) => {
                    tracing::warn!(error = %e, "persona:act receipt failed to serialize — receipt dropped, act unaffected")
                }
            }
        }
    }

    // The background dispatches are part of what she just did — record them as
    // proprioception so the mind knows it sent them away (and won't re-dispatch or block).
    // They are already concise, so both channels carry them verbatim.
    for note in &bg_notes {
        observation.push_str(note);
        observation.push_str("\n\n");
        recall_observation.push_str(note);
        recall_observation.push_str("\n\n");
    }
    let mut observation = observation.trim().to_string();
    let recall_observation = recall_observation.trim().to_string();

    // If the mind just re-issued an IDENTICAL call, make that redundancy a VIVID perception —
    // not just the implicit `#seq` window-shift that smaller models don't interpret. A true
    // fact about her OWN hands: she perceives she is looping and moves on organically. It never
    // says what to do instead (that would be steering). Glass-boxed: a 14B re-ran the exact
    // `code/search` 18× with the found file already in memory — structure the experience so the
    // loop is felt. [[write-cognition-as-a-parent-above-lowered-expectations]]
    if max_repeat >= 2 {
        observation = format!(
            "⚠ I have now issued this EXACT tool call {max_repeat} times; its result has not \
             changed and is already in my working memory above. Repeating it tells me nothing \
             new — I already have what this call can give me.\n\n{observation}"
        );
    }

    // Investigation-shape perception: once a concern has accumulated a few acts,
    // render the mind's own act DISTRIBUTION as a standing structural fact. The
    // fingerprint note above catches exact repeats; this catches the wider
    // pattern a mind can't otherwise see about itself — e.g. "9 acts so far,
    // all code/search" (glass-boxed on SWE flask-4045: distinct-but-all-search
    // acts never tripped the exact-repeat note, and the imbalance itself was
    // invisible). A tally of her own hands is truth, not steering: it names
    // what happened, never what to do next.
    let tally = body.working_memory.action_verb_tally();
    let tally_total: usize = tally.iter().map(|(_, c)| c).sum();
    // Recorded as its own FACT entry below (never folded into the receipt):
    // the tally is truth ABOUT the acts, not an act — folding it in gave a
    // Fact receipt-numbering (found in Asha's volatile.json, the exact type
    // confusion WmKind exists to kill).
    let tally_fact = (tally_total >= 3).then(|| {
        let dist = tally
            .iter()
            .map(|(n, c)| format!("{n} ×{c}"))
            .collect::<Vec<_>>()
            .join(", ");
        format!("[investigation] my acts this concern so far: {dist}.")
    });

    // Admit the outcome as an Episodic engram through the ONE production admit
    // path (a self-observation message from the persona to itself). This is the
    // result-as-memory choice: next tick, recall can surface "I ran X → got Y" the
    // same way it surfaces anything else the persona knows. Best-effort — an
    // admission hiccup must never wedge the act→observe loop.
    let now_ms = now_ms();
    // Admit the outcome as a TOOL-ORIGIN Episodic engram via the self-produced path.
    // The origin is load-bearing (#166): a tool receipt is PROPRIOCEPTION, and
    // `recall_candidates` now gates `EngramOrigin::Tool` OUT of the SEMANTIC recall
    // pool — so tagging it Tool HERE is what actually keeps "code/list(…) → ok"
    // chatter from drowning genuine knowledge in recall. The prior path admitted it
    // as a plain `SenderType::Persona` message → NON-Tool origin → it slipped the
    // gate (verified live 2026-07-13). The recency/working-memory channel (below)
    // still keeps the FULL trace so she sees her own hands. SelfTrust: her own act,
    // no external envelope to verify ([[act-results-need-a-recency-channel-not-
    // semantic-recall]]).
    let tool_name = fg_calls
        .first()
        .map(|c| c.name.clone())
        .unwrap_or_else(|| "action".to_string());
    let obs_hash = crate::persona::inbox_admission::content_hash_sha256(&recall_observation);
    let self_observation = crate::persona::engram::Engram {
        id: Uuid::new_v4(),
        context_id: Some(room_id),
        kind: crate::persona::engram::EngramKind::Episodic,
        content: recall_observation,
        origin: crate::persona::engram::EngramOrigin::Tool(
            crate::persona::engram::ToolInvocationRef {
                invocation_id: Uuid::new_v4(),
                tool_name,
                invoked_at_ms: now_ms,
                input_hash: obs_hash.clone(),
                output_hash: obs_hash,
            },
        ),
        recall_keys: Vec::new(),
        admitted_at_ms: now_ms,
        trust_state_at_admission: crate::persona::engram::TrustState::SelfTrust,
        admission_trace_id: None,
    };
    match body.admission.admit_reflection(self_observation) {
        Ok(crate::persona::engram::AdmissionDecision::Admit { engram, .. }) => {
            // Belt-and-braces with the recall gate: even excluded from semantic
            // recall, keep the receipt's stored salience low so any OTHER surface
            // (recency ranking, dashboards) treats it as proprioception, not
            // durable knowledge.
            body.admission
                .set_recall_salience(engram.id, PROPRIOCEPTION_RECALL_SALIENCE);
            // The causal spine (CAUSAL-MEMORY-GRAPH.md): this act was decided by
            // a deliberation that perceived the chain's previous act result, so
            // wire the CausedBy edge HERE, where the relation is known — the
            // `because` clause as structure. A fact about what happened, never
            // a rail on what she does next.
            if let Some(prior) = chain.prior() {
                body.admission.link_engrams(
                    engram.id,
                    prior,
                    crate::persona::engram_graph::EdgeKind::CausedBy,
                );
                crate::probe!(
                    class = "engram.edge.caused_by",
                    persona = %body.persona_name,
                    room_id = %room_id,
                    from = %engram.id,
                    to = %prior,
                    "act chained to its predecessor in the causal memory graph"
                );
            }
            chain.advance(engram.id);
        }
        Ok(_) => {} // Drop (dedup) — nothing admitted to weight.
        Err(e) => {
            tracing::debug!(
                persona = %body.persona_name,
                error = %e,
                "act→observe: self-observation not admitted (folds into perception anyway)"
            );
        }
    }

    // Proprioception: record the act + result head into VOLATILE working memory too.
    // The engram admit above is content-deduped (correct for long-term memory: don't
    // store the same fact twice), which means a REPEATED identical act is a no-op
    // there — and with thinking suppressed (gateway default) the reasoning channel is
    // dark too. Both channels that carry "what just happened" go silent on a repeat,
    // so perception is byte-identical and greedy decode re-emits the identical Act
    // forever. Working memory is the recency/proprioception channel that fixes it: it
    // is append-only and `#n`-stamped, so a repeat still SHIFTS the perception window
    // next tick and the mind can see its own hands (and that it's repeating itself).
    // This is the shared live↔eval channel, not the eval-only `[you just acted]` fold.
    // See [[act-results-need-a-recency-channel-not-semantic-recall]].
    // Pass the FULL observation — WorkingMemory keeps the latest whole (so the mind can
    // work with what it just fetched) and derives the trail head itself. This is the fix
    // for live agents being starved to the head of their own tool results.
    // TYPED: store the receipt string (byte-identical to the old `record_receipt`)
    // ALONGSIDE the typed acts, so `active_act()`/`recent_acts()` read the tool result
    // by field instead of re-parsing this prose (run-18057-f1). `observation` is the
    // one-time recency rendering; `acts` are the id-correlated typed observations.
    body.working_memory
        .record_receipt_typed(&acts, &observation, Some(room_id));
    if let Some(f) = &tally_fact {
        body.working_memory.record_fact(f);
    }

    // WHICH verbs ran — not just how many. Measured 2026-08-06: this probe carried
    // `tools=1` and a char count and NOTHING ELSE, so the substrate could not answer
    // "are the citizens writing files or only looking around?" from its own telemetry.
    // Answering it required reconstructing verbs from prose receipts in prompt-captures
    // — and only 5 of 84 live captures carried a receipt at all, so the reconstruction
    // was unfalsifiable. A count tells you an act happened; the NAME tells you whether
    // it was work. Same class as the ACL gate that logged nothing (routing.acl.refused).
    //
    // `wrote` is the question we actually keep asking, precomputed so it is a filter and
    // not a substring guess at query time: did anything in this batch reach DISK?
    let verbs: Vec<&str> = calls.iter().map(|c| c.name.as_str()).collect();
    let wrote = verbs.iter().any(|n| {
        let n = n.replace('_', "/");
        n.contains("write") || n.contains("edit") || n.contains("apply") || n.contains("commit")
    });
    crate::probe!(
        class = "persona.act.observed",
        persona = %body.persona_name,
        room_id = %room_id,
        tools = calls.len(),
        verbs = %verbs.join(","),
        wrote,
        chars = observation.len(),
        "acted and observed the result"
    );

    ActOutcome::Acted { acts }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the long-running set matches the REAL command names (the live bug
    // was `cargo/test` vs the actual `code/cargo/test`), and ordinary fast commands are NOT
    // backgrounded — so a `code/read` still returns synchronously in the same turn.
    #[test]
    fn long_running_set_matches_real_command_names() {
        assert!(is_long_running("code/cargo/test"));
        assert!(is_long_running("code/cargo/check"));
        assert!(is_long_running("cognition/full-evaluate"));
        assert!(
            !is_long_running("code/read"),
            "a file read stays synchronous"
        );
        assert!(!is_long_running("chat/send"));
        assert!(
            !is_long_running("cargo/test"),
            "the wrong short name must NOT match"
        );
    }
}
