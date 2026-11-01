//! `apply_act`: execute ONE `Act` verdict — run the calls through the persona's
//! hands, admit the outcome as an Episodic engram (the result becomes memory), and
//! return the observation text so the caller can fold it into the next perception.
//! Extracted verbatim from `act_observe` (pure code-motion, #386 decomposition).

use continuum_base::time::EpochMs;
use uuid::Uuid;

use crate::ai::types::{ToolCall, ToolResult};
use crate::cognition::workspace::WorkspaceCycle;

use super::observation::{extract_paths, ActOutcome, ActStatus, Observation, ToolOutput, ToolVerb};

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

/// One settle chain's causal thread — owned by the DRIVER of the chain
/// (`drive_to_settle`, or one turn-frame), passed by reference through
/// `settle_step` into `apply_act`. Holds the engram id of the most recently
/// ADMITTED act observation so the next act in the same chain can carry a
/// `CausedBy` edge to it (CAUSAL-MEMORY-GRAPH.md §3a). One concern, one
/// chain; the driver drops it when the turn settles, so an edge can never
/// cross turns or rooms by construction — the wire is the scope, never an
/// inference over timestamps.
#[derive(Default)]
pub struct ActChain(std::sync::Mutex<Option<Uuid>>, Option<super::EffectGate>);

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
        Self(std::sync::Mutex::new(cause.root()), None)
    }

    /// The turn's pre-effect mark (stop-drain S2): held by the chain the turn owns, so it is
    /// dropped when the turn settles and can never mark another turn's effects.
    pub fn with_effects(mut self, gate: Option<super::EffectGate>) -> Self {
        self.1 = gate;
        self
    }

    /// Record `effect` before it happens, through this turn's mark when it has one.
    pub async fn mark_before(&self, effect: &super::Effect) -> Result<(), String> {
        super::EffectGate::before(self.1.as_ref(), effect).await
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
    hands: Option<crate::identity::hands_context::HandsContext>,
    issuing_generation: Option<String>,
) -> ActOutcome {
    // no hands → cannot act (and tools were never offered)
    let Some(body) = cycle.acting() else {
        return ActOutcome::NoHands;
    };
    // THE TURN'S OWN HANDS, bound at its admission (`Burst::hands`): the card claim it works,
    // or her default hands. Every act of the turn runs in these and nothing here moves any
    // hands (card fc9e9fd7). Hands that can't act in the context run nothing: never hers.
    let executor = match hands {
        None => body.executor.clone(),
        Some(context) => match body.executor.in_hands(body.persona_id, context) {
            Some(executor) => executor,
            None => return ActOutcome::NoHands,
        },
    };
    // The generation whose calls these are rides the caller (card fe8554d2), so work/submit
    // can bind what she submits to the turn that made it. Hands that cannot stamp run the
    // calls as before, with no provenance.
    let executor = match issuing_generation.and_then(|g| executor.issued_by(g)) {
        Some(stamped) => stamped,
        None => executor,
    };
    // Perception and execution share the activity resolved by the bound-turn owner.
    // Private turns retain their mind room at that boundary.
    let private = crate::cognition::mind_room::is_private_room(body.persona_id, room_id);

    // Call history is not a cache-validity proof. Reads can change, failures can
    // recover, and earlier results may no longer fit the prompt. Execute through
    // the owning command; authorization and operation-specific idempotency stay there.
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
    // Her checkout before the batch: `wrote` is what the batch DID to it, by any verb.
    let acting_root = crate::cognition::persona_workspace::acting_root_in(body.persona_id, executor.hands_context());
    let disk_before = super::disk_change::fingerprint_at(acting_root.clone()).await;
    let mut bg_notes: Vec<String> = Vec::new();
    // The long-running calls that were dispatched (not run synchronously) — retained so a
    // pure-background batch (no foreground calls) can still emit a typed act, keeping
    // `produced_an_act()` true exactly as the old `Some(bg_notes)` return did.
    let mut dispatched_calls: Vec<ToolCall> = Vec::new();
    // Stop-drain S2: the batch is recorded BEFORE anything is dispatched, foreground or
    // background, so a turn the stop cuts after this point is known to have acted and is never
    // re-run blindly. An unconfirmed mark refuses the whole batch: nothing runs unrecorded.
    // A reported sentinel (`tools/<cut at the output limit>`, …) never runs, so it is no effect:
    // a cut turn recorded as acting would read as uncertain and never be handed back (enrico,
    // 2026-10-27 02:54Z: Eitan's only "call" was the cut-at-limit sentinel). A batch of only
    // sentinels takes no mark.
    let effect_calls: Vec<(String, String)> = calls
        .iter()
        .filter(|call| !crate::cognition::tool_executor::command_executor::is_reported_sentinel(&call.name))
        .map(|call| (call.id.clone(), call.name.clone()))
        .collect();
    let marked = match effect_calls.is_empty() {
        true => Ok(()),
        false => chain.mark_before(&super::Effect::ToolBatch { calls: effect_calls }).await,
    };
    if let Err(refusal) = marked {
        continuum_base::probe!(
            class = "persona.act.effect_unmarked",
            persona = %body.persona_name,
            tools = calls.len(),
            error = %refusal,
            "the tool batch could not be recorded before it ran, so it did not run (fail closed)"
        );
        // She perceives it: an unfulfilled act otherwise drops its message.
        body.working_memory.record_fact(&format!(
            "[not run] my last {} tool call(s) did not run: the substrate could not record them \
             before running them ({refusal})",
            calls.len()
        ));
        return ActOutcome::ExecutorError {
            calls: calls.to_vec(),
            message: format!(
                "this batch did not run: the substrate could not record it before running it, \
                 so a restart could not tell whether it happened ({refusal})"
            ),
        };
    }
    let fg_calls: Vec<ToolCall> = match executor.command_executor() {
        Some(exec) => {
            let mut fg = Vec::with_capacity(calls.len());
            for call in calls {
                let cmd = call.name.replace('_', "/");
                if is_long_running(&cmd) {
                    let handle = exec.dispatch_background(
                        cmd.clone(),
                        call.input.clone(),
                        // In THESE hands: a long-running command a card turn dispatches runs in
                        // the card's workspace, never her default one (card fc9e9fd7).
                        Some({
                            let identity = crate::routing::CallerIdentity::local_persona(
                                crate::identity::PeerId::from_uuid(body.persona_id),
                            );
                            match executor.hands_context() {
                                Some(context) => identity.in_hands(context),
                                None => identity,
                            }
                        }),
                    );
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
                    continuum_base::probe!(
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
    // Every result this batch saves stays live from before its file is written until working
    // memory has recorded the batch below (card 12b080c3).
    let pin = crate::cognition::tool_executor::spill::LivePin::new();
    // Her reads with no range answer with their first page when the whole file is more than
    // their share of her view's measured per-act headroom (plan S1): the tool answering a
    // smaller question, never her view cutting a result. Only this dispatched copy carries it.
    let dispatched = super::page_budget::with_page_budgets(&fg_calls, body.working_memory.view_headroom());
    let batch_result = tokio::time::timeout(
        TOOL_BATCH_CEILING,
        executor
            .execute_native_batch_pinned(&dispatched, &ctx, &pin),
    )
    .await
    .unwrap_or_else(|_| { // timeout elapsed = the hang this bound exists to convert; the closure builds the honest error
        continuum_base::probe!(
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
                custody: crate::ai::types::ResultCustody::Unsaved,
            });
        // The COMMAND's own verdict, decoded once by the executor from the
        // PRE-FOLD value and carried here by tool_use_id. Absent (a test double, or
        // a command that never opted in) ⇒ `Unprojected`, which behaves exactly as
        // this seam did before.
        let projected = outcome
            .verdicts
            .iter()
            .find(|v| v.tool_use_id == call.id);
        let verdict = projected.map(|v| v.verdict).unwrap_or_default(); // JUSTIFIED: no record for this call means the command never opted in (or a test double built the batch) — `Unprojected` IS the right answer there, never a failure

        // PUSHED SHELL COMPLETION, receive side (2026-08-24): a `code/shell` whose
        // inline window elapsed hands back a RUNNING handle. Register that handle as a
        // dispatch NOW so the exit fold's command:completed event (shell_session.rs)
        // has a label to fold against — dispatch_listener drops completions for
        // unregistered handles. Without this she must remember to poll; with it the
        // finished build lands in her working memory like any dispatched background job.
        //
        // This used to re-parse `ShellExecuteResponse` out of `typed_result.content`
        // and match `call.name` by hand. Both halves were broken: the name test was
        // `replace('_', "/") == "code/shell"`, so a model calling the declared alias
        // `bash` never reached it; and the content it parsed is the FOLDED preview,
        // so a flood-sized result no longer decoded. The command now DECLARES its
        // handle and the executor decodes it once from the full value.
        if verdict.running() {
            if let Some(handle) = projected.and_then(|v| v.dispatch_handle) {
                // Her command whole, as she wrote it: this label is what her view shows for the
                // handle, and a cut command reads as a different one.
                let label: String = call
                    .input
                    .get("command")
                    .and_then(|v| v.as_str())
                    .unwrap_or(call.name.as_str()) // label only — the handle is the key
                    .to_owned();
                body.working_memory.record_dispatch_event(
                    handle,
                    &format!("{}: {label}", call.name),
                    "handle handed back — still running",
                    crate::cognition::working_memory::DispatchStatus::Running,
                );
            }
        }
        let obs = Observation {
            call: call.clone(),
            output: ToolOutput {
                image: projected.and_then(|v| v.image.clone()),
                additional_images: projected.and_then(|v| v.additional_images.clone()),
                result: typed_result,
                verb: ToolVerb::classify(&call.name),
                paths: extract_paths(&call.input),
                verdict,
            },
            status: ActStatus::Executed,
        };
        observation.push_str(&obs.render_recency());
        recall_observation.push_str(&obs.render_recall(intent));
        // (The canvas feed publishes at the tool-executor seam, PRE-fold — a
        // flood-sized ObserveResult here is already a spilled preview that no
        // longer parses. See CommandToolExecutor::execute_native_batch.)
        acts.push(obs);
    }
    // Every requested call needs its own truthful receipt, including background
    // calls in a mixed batch. Native continuation must never omit their IDs.
    append_dispatched_observations(&mut acts, &dispatched_calls);
    let call_order: std::collections::HashMap<_, _> = calls.iter()
        .enumerate().map(|(ordinal, call)| (call.id.as_str(), ordinal)).collect();
    // Loop-awareness filtering: if multiple calls share a fingerprint and produce
    // identical result content, keep only RESPONSE_LOOP_THRESHOLD + 1 of them —
    // beyond that the mind is looping on unchanged evidence.
    const RESPONSE_LOOP_THRESHOLD: usize = 3;

    let mut groups: std::collections::HashMap<String, Vec<usize>> = std::collections::HashMap::new();
    for (i, act) in acts.iter().enumerate() {
        let fp = format!(
            "{}|{}",
            act.call.name,
            serde_json::to_string(&act.call.input).unwrap_or_default()
        );
        groups.entry(fp).or_default().push(i);
    }

    let mut keep: Vec<bool> = vec![true; acts.len()];
    for (_fp, indices) in groups {
        if indices.len() <= RESPONSE_LOOP_THRESHOLD + 1 {
            continue;
        }

        // Check if all results are identical within this group
        let result_content = acts[indices[0]].output.result.content.clone();
        let all_identical = indices.iter().skip(1).all(|&idx| 
            acts[idx].output.result.content == result_content
        );

        if all_identical {
            // Keep only the first RESPONSE_LOOP_THRESHOLD + 1, drop the rest
            for &idx in indices.iter().skip(RESPONSE_LOOP_THRESHOLD + 1) {
                keep[idx] = false;
            }
        }
    }

    let mut keep_iter = keep.into_iter();
    acts.retain(|_| keep_iter.next().unwrap());
    acts.sort_by_key(|observation| call_order.get(observation.call.id.as_str()).copied());
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
        let now_ms = EpochMs::now().millis();
        // Her reasoning for this batch rides its FIRST receipt, whole: the human's view of
        // her thinking, which the renderer collapses or opens (Joel, 2026-10-22: "could
        // always interrogate the think in ui"). It never goes to the room below, and a turn
        // in her mind room shows no one her reasoning.
        let mut reasoning =
            Some(intent.trim()).filter(|r| !private && !r.is_empty()).map(str::to_owned);
        for obs in &acts {
            // The human OBJECT of the act: the first extracted path (reads,
            // writes, edits), else the shell command head, else empty —
            // honest-thin, never dumped raw JSON args.
            let summary = act_object(obs);
            let update = crate::cognition::mind_events::PersonaActUpdate {
                act_id: Uuid::new_v4(),
                room_id,
                actor_id: body.persona_id,
                actor_name: body.persona_name.clone(),
                tool: obs.call.name.clone(),
                summary,
                // `ok` keeps its existing meaning for the desktop that already
                // consumes it: false ONLY for a real failure. A still-RUNNING act
                // stays `ok: true` here rather than silently reading as a failure in
                // a client this PR does not change — the truth it could not carry
                // rides `state` beside it.
                ok: !obs.is_err(),
                state: obs.output.verdict.label().to_string(),
                thought: reasoning.take(),
                timestamp: now_ms,
            };
            match serde_json::to_value(&update) {
                Ok(payload) => bus.publish_async_only(crate::cognition::mind_events::PERSONA_ACT, payload),
                Err(e) => {
                    tracing::warn!(error = %e, "persona:act receipt failed to serialize — receipt dropped, act unaffected")
                }
            }
        }
        // THE SAME HISTORY, FOR THE OTHER KIND (Joel, 2026-08-30: "they should
        // be chatting within their activity as they work and see each other's
        // work, thoughts, chats, actions like any good history"). The loop
        // above feeds the HUMAN console; teammates' perception reads the airc
        // room transcript — which carried nothing, so a charged reviewer
        // watched an empty room while the solver worked beside her. One
        // coalesced, honest-thin receipt message per act batch, authored by
        // the actor herself: her own self-filter keeps it from waking HER;
        // roommates perceive live work and can speak the moment something
        // looks wrong. Receipts are history, not imperatives — coalescing is
        // correct here (the one-block-per-imperative law governs kickoffs,
        // not transcripts).
        // What she SAID with these calls leads the receipt (Joel, 2026-08-30: "they
        // should be chatty as they work like you are"): the words she wrote beside them,
        // recorded by the verdict as her action statement. Never `intent`, which is her
        // reasoning: that was published here until 2026-10-22 (PRIVACY-OF-THOUGHT.md, sink
        // 6; Joel: "Why would thinking be broadcast?"). Her reasoning reaches her own next
        // turn through working memory and the human through the act receipt above. A batch
        // she said nothing about leads with its acts.
        let mut lines: Vec<String> = Vec::with_capacity(acts.len() + 1);
        if let Some(said) = body.working_memory.action_statement(room_id) {
            lines.push(crate::cognition::presence_glyph::said_line(said.trim()));
        }
        lines.extend(acts.iter().map(|obs| {
                let object = act_object(obs);
                crate::cognition::presence_glyph::act_line(
                    &obs.call.name,
                    &object,
                    // The command's own word where it gave one; the transport bool
                    // only where it did not (an unprojected command is unchanged).
                    if obs.output.verdict == crate::sdk_codegen::ActVerdict::Unprojected
                        && obs.output.result.is_error == Some(true)
                    {
                        crate::sdk_codegen::ActVerdict::Declared(
                            crate::sdk_codegen::ToolVerdict::Failed,
                        )
                    } else {
                        obs.output.verdict
                    },
                )
            }));
        // The vitals ACT PULSE: executed acts are the thinking-right-now
        // signal during a held-work turn (the cycle-delta is blind inside one).
        crate::cognition::vitals_pulse::record_acts(body.persona_id, acts.len() as u64);
        // Receipts radiate into the CARD's activity room when a held card
        // rooted her hands — the room her reviewer and teammates watch — not
        // the room whose line triggered the turn (`acting_card_of`).
        let receipt_room = room_id;
        if private {
            crate::cognition::mind_room::note_withheld(body.persona_id, "thought_line");
        } else if !lines.is_empty() {
            if let Some(published) =
                crate::cognition::extensions::publish_in_room(body.persona_id, receipt_room, lines.join("\n")).await
            {
                match published {
                    Ok(()) => continuum_base::probe!(
                        class = "room.work_receipt.published",
                        room = %receipt_room,
                        actor = %body.persona_id,
                        acts = lines.len() as u64,
                        "act receipts radiated into the activity transcript — roommates see the work"
                    ),
                    Err(e) => continuum_base::probe!(
                        class = "room.work_receipt.failed",
                        room = %room_id,
                        actor = %body.persona_id,
                        error = %e,
                        "act receipts did not reach the room transcript — console feed unaffected"
                    ),
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

    // Count requests without claiming their results are identical or still retained.
    // The actual result below is authoritative, including errors and changed state.
    if max_repeat >= 2 {
        observation = format!(
            "[repetition] A call in this batch has been requested {max_repeat} times. \
             This execution's result follows.\n\n{observation}"
        );
    }

    // Admit the outcome as an Episodic engram through the ONE production admit
    // path (a self-observation message from the persona to itself). This is the
    // result-as-memory choice: next tick, recall can surface "I ran X → got Y" the
    // same way it surfaces anything else the persona knows. Best-effort — an
    // admission hiccup must never wedge the act→observe loop.
    let now_ms = EpochMs::now().millis();
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
    let obs_hash = crate::cognition::inbox_admission::content_hash_sha256(&recall_observation);
    let self_observation = crate::cognition::engram::Engram {
        id: Uuid::new_v4(),
        context_id: Some(room_id),
        kind: crate::cognition::engram::EngramKind::Episodic,
        content: recall_observation,
        origin: crate::cognition::engram::EngramOrigin::Tool(
            crate::cognition::engram::ToolInvocationRef {
                invocation_id: Uuid::new_v4(),
                tool_name,
                invoked_at_ms: now_ms,
                input_hash: obs_hash.clone(),
                output_hash: obs_hash,
            },
        ),
        recall_keys: Vec::new(),
        admitted_at_ms: now_ms,
        trust_state_at_admission: crate::cognition::engram::TrustState::SelfTrust,
        admission_trace_id: None,
    };
    match body.admission.admit_reflection(self_observation) {
        Ok(crate::cognition::engram::AdmissionDecision::Admit { engram, .. }) => {
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
                    crate::cognition::engram_graph::EdgeKind::CausedBy,
                );
                continuum_base::probe!(
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
    // The ring keeps this exact text; save THAT, so the pointer it leaves when it collapses
    // resolves to what she saw, every call of the batch included.
    let custody = pin.retain(body.persona_id, observation.trim());
    let disk = super::disk_change::DiskChange::between(
        disk_before,
        super::disk_change::fingerprint_at(acting_root).await,
    );
    body.working_memory
        .record_receipt_measured(&acts, &observation, Some(room_id), custody, disk);
    // Each open view this act touched keeps what she understood when she read it.
    body.working_memory.annotate_open_views(intent);
    // Recorded and published: working memory now names every handle this batch saved, its
    // receipt's and each call's, so the pin that held them since before they were written can go.
    drop(pin);
    body.working_memory.refresh_recent_action_status();

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
    // MEASURED, by any verb: her checkout's fingerprint after the batch against before it.
    // A write through code/shell (sed -i, a heredoc, git apply, git commit) used to count
    // as reading, because `wrote` read the verb NAME (Joel, 2026-10-06: "0 writes always a
    // serious plumbing bug"). An executed mutating verb still counts on its own, for a
    // write that lands outside the checkout. A mutating verb that ERRORED reached no disk
    // (Freya, 2026-09-05: a `code/edit` answering "File not found" read as a write).
    let wrote_by_verb = acts.iter().any(Observation::mutated_workspace);
    // and the board: a submission, a review verdict, a ledger note or a card she put up is
    // written work that reaches no disk, and counted as reading until now
    let published = acts.iter().any(|a| {
        a.completed_successfully() && a.output.verb.publishes()
    });
    let wrote = wrote_by_verb || published || disk == super::disk_change::DiskChange::Changed;
    continuum_base::probe!(
        class = "persona.act.observed",
        persona = %body.persona_name,
        room_id = %room_id,
        tools = calls.len(),
        verbs = %verbs.join(","),
        wrote,
        disk = disk.as_str(),
        published,
        chars = observation.len(),
        "acted and observed the result"
    );
    crate::cognition::health_ledger::note_act(wrote);
    crate::cognition::health_ledger::note_act_of(body.persona_id, wrote);

    // An act that changed a checkout wakes the one carry owner (modules::card_carry), which
    // carries every resident's committed card work; a turn's end is no longer the only push.
    if disk == super::disk_change::DiskChange::Changed {
        crate::cognition::extensions::request_card_carry();
    }

    ActOutcome::Acted { acts }
}

fn append_dispatched_observations(acts: &mut Vec<Observation>, dispatched_calls: &[ToolCall]) {
        for call in dispatched_calls {
            acts.push(Observation {
                call: call.clone(),
                output: ToolOutput {
                    image: None,
                    additional_images: None,
                    result: ToolResult {
                        tool_use_id: call.id.clone(),
                        content: "dispatched — running in background".to_string(),
                        custody: crate::ai::types::ResultCustody::Unsaved,
                        is_error: None,
                    },
                    verb: ToolVerb::classify(&call.name),
                    paths: extract_paths(&call.input),
                    // A dispatched background call is RUNNING by construction —
                    // that is what "dispatched" means. Saying so here is what stops
                    // the receipt rendering a tick for work that has not started
                    // producing anything yet.
                    verdict: crate::sdk_codegen::ActVerdict::Declared(
                        crate::sdk_codegen::ToolVerdict::Running,
                    ),
                },
                status: ActStatus::Executed,
            });
        }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the long-running set matches the REAL command names (the live bug
    // was `cargo/test` vs the actual `code/cargo/test`), and ordinary fast commands are NOT
    // backgrounded — so a `code/read` still returns synchronously in the same turn.
    #[test]
    fn long_running_set_matches_real_command_names() {
        let call = |id: &str| ToolCall { id: id.into(), name: "code/cargo/test".into(), input: serde_json::json!({}) };
        let mut receipts = Vec::new();
        append_dispatched_observations(&mut receipts, &[call("existing")]);
        append_dispatched_observations(&mut receipts, &[call("new-dispatch")]);
        assert_eq!(receipts.len(), 2, "a nonempty batch must not suppress dispatched replies");
        assert_eq!(receipts[1].output.result.tool_use_id, "new-dispatch");
        assert!(receipts[1].output.verdict.running());
        assert!(!receipts[1].completed_successfully());
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

/// The human object of an act, for the clients' act line and presence glyph: its first extracted
/// path (a read, a write, an edit), else its shell command, else empty, never raw JSON arguments.
/// Whole: how much of it a client shows is that client's rendering, not a cut made here.
fn act_object(obs: &Observation) -> String {
    obs.output
        .paths
        .first()
        .map(|p| p.display().to_string())
        .or_else(|| obs.call.input.get("command").and_then(|c| c.as_str()).map(str::to_owned))
        .unwrap_or_default() // unwrap_or_default: an act with no path and no command has no object
}
