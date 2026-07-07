//! The act→observe motion — the organism's drive to settle.
//!
//! See [docs/cognition/ACTING-ORGANISM.md]. Action is not a return value the
//! deliberation faculty loops on internally; it is a [`Decision::Act`] verdict
//! the ORGANISM drives. This module holds that drive as a free function over a
//! [`WorkspaceCycle`] — never a method on the cycle, so `run_in_room` stays a
//! pure single tick (§3.3).
//!
//! Two callers share ONE motion (the load-bearing compression):
//! - the live heartbeat (`persona::service_loop`) calls [`apply_act`] ONCE per
//!   tick and lets the metronome bring the next perception — she re-perceives at
//!   heartbeat cadence, never on a synchronous inner counter (§4 Live = no
//!   deadline);
//! - the eval grader (`modules::cognition` `cognition/eval`) calls
//!   [`drive_to_settle`], which loops `apply_act` → re-tick under an EXTERNAL
//!   budget (the grader's stopwatch — the only special power an observer holds),
//!   because the synthetic eval room has no heartbeat servicing it.
//!
//! The load-bearing choice is **result-as-engram**: executing an action admits
//! its outcome as an Episodic engram (the persona observing its own hands), so
//! the result becomes a thing the mind remembers and can be reminded of next
//! tick — unifying it with how she carries every other fact. The disposition to
//! act (build→run→test) is the GENOME's to grow, never a Rust `if`; this module
//! only gives her the hands and the memory of using them.

use uuid::Uuid;

use super::workspace::{Burst, Decision, TurnFraming, TurnMetrics, WorkspaceCycle};
use crate::ai::types::ToolCall;
use crate::persona::types::{InboxMessage, SenderType};

/// Max chars of a single tool result folded into the next perception / engram.
/// A bound on what we re-inject, NOT a clamp on the model's own generation: the
/// model owns its output length (a hung child is bounded by `code/run`'s wall
/// clock instead). Generous — a traceback she needs to read to self-correct must
/// survive intact.
const RESULT_FOLD_MAX_CHARS: usize = 16_000;

// The working-memory trail-head bound lives in `working_memory.rs` now (its home — WM owns
// its own truncation). Still used here for the settlement answer-head.
use crate::cognition::working_memory::WM_ACTION_HEAD_CHARS;

/// The result of driving a mind to settlement.
pub struct SettleOutcome {
    /// The verdict the mind settled on: `Speak`/`RaiseUnprompted`/`Pass` when it
    /// settled, or the final un-driven `Act` if the external budget ran out
    /// mid-action (the grader grades that as "did not finish" — honest, never a
    /// fabricated answer).
    pub decision: Decision,
    /// The spoken text, present only when the settled decision is `Speak` /
    /// `RaiseUnprompted`. This is what an external observer (the grader, or a peer
    /// in the room) reads.
    pub spoken: Option<String>,
    /// How many actions were executed before settling.
    pub acts: usize,
    /// The final world-state, with each action's observation folded in — what the
    /// last tick perceived. Captured for replay/forensics.
    pub world_state: String,
    /// The accumulated cost of settling this task: every act→observe deliberation
    /// generation's latency + tokens, summed. `tokens_per_second()` re-derives
    /// throughput from the totals. This is the speed/latency the eval reports next
    /// to the accuracy grade — the same number a live turn could surface for the
    /// serving governor.
    pub metrics: TurnMetrics,
    /// `Some(cause)` when the settle loop stopped because the deliberation model
    /// call FAILED (timeout, 5xx, a serving lane refusing a model it isn't hosting)
    /// rather than settling on a verdict. The grader MUST treat this as an
    /// infrastructure failure — NOT a wrong answer — so an inference hiccup never
    /// corrupts the accuracy metric ([[self-improvement-is-a-control-loop]]: the
    /// reward is only as trustworthy as the metric). `None` on every settled turn.
    pub inference_error: Option<String>,
}

/// The tail of the working-memory buffer SINCE the persona last settled (produced an
/// utterance). Everything before the most recent [`WM_SETTLEMENT_PREFIX`] boundary
/// belongs to a concern she already ANSWERED — an identical tool call over there is
/// not a spin, so re-issuing it for the current concern is legitimate. This is the
/// scope that separates "how many commands?" → answer → "list them again" (both emit
/// `commands/list({})`, legitimately) from re-issuing the SAME call within one
/// unanswered settling (the spin). Mirrors the `responded_through` boundary the
/// `ActThenSpeak` test faculty tracks — content-driven, not a turn counter.
fn entries_since_last_settlement(recent: &[String]) -> &[String] {
    match recent
        .iter()
        .rposition(|e| e.starts_with(crate::cognition::working_memory::WM_SETTLEMENT_PREFIX))
    {
        Some(i) => &recent[i + 1..],
        None => recent,
    }
}

/// True when EVERY call in this batch has ALREADY been carried out SINCE THE LAST
/// SETTLEMENT — the identical `(name, args)` already appears as a satisfied `I ran …`
/// trace in the not-yet-answered tail of the working-memory recency channel. Keyed on
/// the SAME `name(args)` rendering [`apply_act`] records below (`I ran {name}({args})`),
/// so detection and recording can never drift; scoped to the current concern by
/// [`entries_since_last_settlement`] so an identical call the mind already answered in
/// a PRIOR concern (still lingering in the volatile buffer) does not false-trigger.
///
/// This is content-driven proprioception, NOT an iteration counter: it fires only on a
/// byte-identical re-request whose result the mind already holds THIS concern —
/// precisely the signal the `[action #n]` stamp shift was MEANT to convey but
/// empirically does not. A greedy instruct model re-emits the identical `Act` despite
/// the shifted window (proven live 2026-07-02 via `cognition/prompt`, nil-room eval:
/// working memory carried `commands/list` + its full result and she re-issued
/// `commands/list` on every act, never converting the result to an answer). A MIXED
/// batch — any genuinely new call — is NOT a repeat: the new call yields new
/// perception and must run.
fn all_calls_already_satisfied(recent: &[String], calls: &[ToolCall]) -> bool {
    if calls.is_empty() {
        return false;
    }
    let scope = entries_since_last_settlement(recent);
    calls.iter().all(|call| {
        let args = serde_json::to_string(&call.input).unwrap_or_else(|_| "{}".to_string());
        let signature = format!("I ran {}({})", call.name, args);
        scope.iter().any(|trace| trace.contains(&signature))
    })
}

/// Execute ONE `Act` verdict: run its calls through the persona's hands, admit
/// the outcome as an Episodic engram (the result becomes memory), and return the
/// observation text so the caller can fold it into the next perception.
///
/// `room_id` is the room THIS act is about — passed per-call because one mind is
/// in many rooms at once (a persona, like a Claude tab, is in multiple rooms
/// simultaneously); the [`ActingBody`](super::workspace::ActingBody) itself is
/// room-agnostic.
///
/// Returns `None` (abstain — never a fabricated success) when the mind has no
/// hands or the executor errors. The admission is best-effort: an un-admitted
/// observation still flows back via the returned text, so re-perception works
/// regardless; admission is what makes it durable long-term memory.
/// Commands that run long enough that BLOCKING the turn on them starves the mind — they
/// are DISPATCHED in the background (fire-and-poll) and stream their result back into
/// working memory via the dispatch listener when they finish. Seed set; the durable home is
/// a per-command `long_running` flag on the command spec (#86). Matched on the slash form
/// (models may emit the underscore form — normalize before calling).
fn is_long_running(command: &str) -> bool {
    matches!(
        command,
        "code/cargo/check" | "code/cargo/test" | "cognition/full-evaluate" | "forge/train"
    )
}

pub async fn apply_act(
    cycle: &WorkspaceCycle,
    calls: &[ToolCall],
    intent: &str,
    room_id: Uuid,
) -> Option<String> {
    let body = cycle.acting()?; // no hands → cannot act (and tools were never offered)

    // Repeat-perception (proprioception, content-driven — NOT an agentic counter).
    // If this exact batch was ALREADY carried out this settle, its result is already
    // in working memory. Re-running it burns a tool round-trip + a redundant (content-
    // deduped, so no-op) engram and returns byte-identical perception — off which a
    // greedy instruct model re-emits the identical `Act` forever. The `[action #n]`
    // stamp shift was supposed to break this and does not (see
    // `all_calls_already_satisfied`). So do NOT re-execute: record an EXPLICIT
    // "already satisfied — answer now" trace so the redundancy is PERCEIVED rather
    // than merely present, and let the caller re-perceive. This decides nothing about
    // WHAT she answers; it only stops her being blind to the fact she already acted —
    // symmetric to the recency channel itself and the loop-filler dedup (context
    // hygiene, not cognition steering; [[no-hardcoded-heuristics-to-steer-cognition]]).
    let recent = body.working_memory.recent();
    if all_calls_already_satisfied(&recent, calls) {
        let names = calls
            .iter()
            .map(|c| {
                let args = serde_json::to_string(&c.input).unwrap_or_else(|_| "{}".to_string());
                format!("{}({})", c.name, args)
            })
            .collect::<Vec<_>>()
            .join(", ");
        let nudge = format!(
            "I already ran {names} this turn — the result is in my working memory above. \
             Running it again returns nothing new. I have what I need; I should ANSWER the \
             question now from that result instead of acting again."
        );
        body.working_memory.record_action(&nudge);
        crate::probe!(
            class = "persona.act.repeat_short_circuited",
            persona = %body.persona_name,
            room_id = %room_id,
            calls = calls.len(),
            "identical act already satisfied this turn — recorded answer-now proprioception, skipped re-execution"
        );
        return Some(nudge);
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
    let fg_calls: Vec<ToolCall> = match body.executor.command_executor() {
        Some(exec) => {
            let mut fg = Vec::with_capacity(calls.len());
            for call in calls {
                let cmd = call.name.replace('_', "/");
                if is_long_running(&cmd) {
                    let handle = exec.dispatch_background(cmd.clone(), call.input.clone(), None);
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

    let outcome = match body
        .executor
        .execute_native_batch(&fg_calls, &ctx, RESULT_FOLD_MAX_CHARS)
        .await
    {
        Ok(o) => o,
        Err(e) => {
            // Fail loud-ish: the hand could not run. Abstain — do NOT synthesize a
            // result the mind would then "remember" as fact ([[fallbacks-are-illegal-fail-loud]]).
            tracing::warn!(
                persona = %body.persona_name,
                error = %e,
                "act→observe: tool batch failed; abstaining (no fabricated outcome)"
            );
            return None;
        }
    };

    // Form the observation: what she did and what came back. First person, because
    // this is the persona observing her OWN hands — the engram reads like a memory
    // of acting, not a log line.
    let mut observation = String::new();
    for (i, call) in fg_calls.iter().enumerate() {
        let result = outcome.results.get(i);
        let body_text = match result {
            Some(r) => r.content.as_str(),
            None => "(no result returned)",
        };
        let args = serde_json::to_string(&call.input).unwrap_or_else(|_| "{}".to_string());
        observation.push_str(&format!(
            "I ran {}({}) because {}.\nResult:\n{}\n\n",
            call.name,
            args,
            intent.trim(),
            body_text.trim(),
        ));
    }
    // The background dispatches are part of what she just did — record them as
    // proprioception so the mind knows it sent them away (and won't re-dispatch or block).
    for note in &bg_notes {
        observation.push_str(note);
        observation.push_str("\n\n");
    }
    let observation = observation.trim().to_string();

    // Admit the outcome as an Episodic engram through the ONE production admit
    // path (a self-observation message from the persona to itself). This is the
    // result-as-memory choice: next tick, recall can surface "I ran X → got Y" the
    // same way it surfaces anything else the persona knows. Best-effort — an
    // admission hiccup must never wedge the act→observe loop.
    let now_ms = now_ms();
    let self_observation = InboxMessage {
        id: Uuid::new_v4(),
        room_id,
        sender_id: body.persona_id,
        sender_name: body.persona_name.clone(),
        sender_type: SenderType::Persona,
        content: observation.clone(),
        timestamp: now_ms,
        priority: 1.0,
        source_modality: None,
        voice_session_id: None,
    };
    if let Err(e) = body.admission.admit(&self_observation, None) {
        tracing::debug!(
            persona = %body.persona_name,
            error = %e,
            "act→observe: self-observation not admitted (folds into perception anyway)"
        );
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
    body.working_memory.record_action(&observation);

    crate::probe!(
        class = "persona.act.observed",
        persona = %body.persona_name,
        room_id = %room_id,
        tools = calls.len(),
        chars = observation.len(),
        "acted and observed the result"
    );

    Some(observation)
}

/// Drive the mind to SETTLEMENT: tick → if `Act`, run it + fold the observation
/// into the next perception → re-tick → until it `Speak`s/`Pass`es or the
/// external `max_acts` budget is spent.
///
/// `max_acts` is the EXTERNAL observer's stopwatch — the grader holds it because
/// the eval room has no heartbeat to pace re-perception. It is NOT a cap that
/// lives in the persona's head (the live path has no such bound; an "acts
/// forever" persona is a fitness gap to train away, never a substrate ceiling —
/// §4). When the budget runs out mid-action, the final un-driven `Act` is
/// returned and the grader scores it as unfinished — never a fabricated answer.
pub async fn drive_to_settle(
    cycle: &WorkspaceCycle,
    burst: impl Into<Burst>,
    room_id: Uuid,
    max_acts: usize,
    framing: TurnFraming,
) -> SettleOutcome {
    let burst: Burst = burst.into();
    let mut acts = 0usize;
    // Fold each tick's deliberation cost in, so the settled outcome reports the
    // task's TOTAL speed/latency (a multi-act task pays for every generation).
    let mut metrics = TurnMetrics::default();

    loop {
        // ONE settlement step through the SHARED primitive the live heartbeat uses
        // (`settle_step`). The only thing this driver adds is the LOOP — because the
        // eval room has no metronome, the grader re-perceives by calling step again.
        // `may_act = acts < max_acts` gates ACTING (not speaking): past the budget
        // she may still settle into a Speak, but a fresh Act is returned un-driven.
        let (step, step_metrics) =
            settle_step(cycle, burst.clone(), room_id, acts < max_acts, framing).await;
        if let Some(m) = step_metrics {
            metrics.accumulate(m);
        }
        match step {
            SettleStep::Spoke(text) => {
                return SettleOutcome {
                    spoken: Some(text.clone()),
                    decision: Decision::Speak { text },
                    acts,
                    world_state: burst.rendered.clone(),
                    metrics,
                    inference_error: None,
                };
            }
            SettleStep::Acted { .. } => {
                acts += 1;
                // The observation re-enters perception through MEMORY + the volatile
                // working-memory recency channel — `apply_act` admitted it and
                // recorded a stamped proprioception trace, and the next `settle_step`
                // re-perceives. This is BYTE-FOR-BYTE the live heartbeat motion
                // (service_loop apply via the SAME `settle_step`, then the metronome
                // re-perceives next tick). `world` is held CONSTANT across iterations:
                // memory is the only thing that changes, exactly as in life — no
                // eval-only `[you just acted]` fold. See
                // [[act-results-need-a-recency-channel-not-semantic-recall]].
            }
            // Budget spent on a fresh Act, OR the act could not be carried out (no
            // hands / exec error). Either way she did not settle in the observer's
            // window — return the un-driven Act so the grader scores it as unfinished,
            // never a fabricated answer.
            SettleStep::WouldAct { calls, intent } | SettleStep::ActUnfulfilled { calls, intent } => {
                return SettleOutcome {
                    decision: Decision::Act { calls, intent },
                    spoken: None,
                    acts,
                    world_state: burst.rendered.clone(),
                    metrics,
                    inference_error: None,
                };
            }
            SettleStep::Passed => {
                return SettleOutcome {
                    decision: Decision::Pass,
                    spoken: None,
                    acts,
                    world_state: burst.rendered.clone(),
                    metrics,
                    inference_error: None,
                };
            }
            // The model call FAILED — no verdict this task. Return LOUD: carry the
            // cause so the grader scores an infra failure, never a fabricated answer
            // and never a silent `Pass` ([[fallbacks-are-illegal-fail-loud]]). We do
            // not loop/retry here — the grader owns retry policy; the settle loop's
            // job is to report the truth of THIS attempt.
            SettleStep::InferenceFailed { error } => {
                return SettleOutcome {
                    decision: Decision::Pass,
                    spoken: None,
                    acts,
                    world_state: burst.rendered.clone(),
                    metrics,
                    inference_error: Some(error),
                };
            }
        }
    }
}

/// The outcome of ONE settlement [`settle_step`].
#[derive(Debug)]
pub enum SettleStep {
    /// She settled on speech (`Speak`/`RaiseUnprompted`) — the prose turn an
    /// observer (a peer, or the grader) reads.
    Spoke(String),
    /// She reached for her hands AND the act was carried out; the result is admitted
    /// as memory + a stamped proprioception trace. The caller re-perceives next
    /// (live: next metronome tick; eval: next loop step). The calls+intent ride
    /// along so a caller that paces acting (the eval budget) can report the final
    /// Act if its budget runs out on the following step.
    Acted { calls: Vec<ToolCall>, intent: String },
    /// She decided to act but the caller's budget said no this step (`may_act =
    /// false`) — the act was NOT executed. Only the eval driver passes `may_act =
    /// false`; the live heartbeat always permits its one act, so it never sees this.
    WouldAct { calls: Vec<ToolCall>, intent: String },
    /// She chose silence (`Pass`) — honored as a turn that produces no utterance.
    Passed,
    /// She reached for an act that could NOT be carried out (no hands / executor
    /// error). No utterance; the intent rides along for honest logging/grading.
    ActUnfulfilled { calls: Vec<ToolCall>, intent: String },
    /// The deliberation model call itself FAILED — a timeout, a 5xx, or the serving
    /// lane refusing a model it isn't hosting (the swept-model bug). NO verdict was
    /// produced. This is NOT a `Passed`: a failed model is not a chosen silence
    /// ([[fallbacks-are-illegal-fail-loud]]). Every caller surfaces the cause LOUD —
    /// the command reports `inferenceFailed`, the live heartbeat logs + retries next
    /// tick, the eval grades it an infra failure — never a serene no-op that hides a
    /// broken lane. `error` names the cause verbatim from the adapter.
    InferenceFailed { error: String },
}

impl SettleStep {
    /// Project a fully-driven [`SettleOutcome`] back onto the SAME `SettleStep` the
    /// live heartbeat already handles — so a DIRECTED live turn can `drive_to_settle`
    /// (converge to an answer in-turn, exactly as the eval path does) and feed the
    /// result through the one existing turn handler, no parallel match.
    ///
    /// The mapping is total and lossless for the live handler's purposes:
    ///   • `inference_error` present → `InferenceFailed` (a failed model is never a
    ///     chosen silence — [[fallbacks-are-illegal-fail-loud]]);
    ///   • `Speak`/`RaiseUnprompted` → `Spoke` (the prose reaches the room);
    ///   • `Act` → `Acted` — the drive spent its whole act budget without settling on
    ///     speech; the results are already in memory, so the live handler `continue`s
    ///     and the metronome re-perceives next tick (the honest long-tail degrade, and
    ///     the ONLY case a directed turn still leans on the tick loop);
    ///   • `Pass` → `Passed` (she declined in her own words).
    /// `drive_to_settle` collapses `WouldAct`/`ActUnfulfilled` into `Decision::Act`
    /// before returning, so those never surface here — an over-budget or un-carried
    /// act both land on `Acted`, which the live handler treats as "re-perceive next
    /// tick", the correct move either way.
    pub fn from_settled(outcome: SettleOutcome) -> (SettleStep, Option<TurnMetrics>) {
        let metrics = Some(outcome.metrics);
        if let Some(error) = outcome.inference_error {
            return (SettleStep::InferenceFailed { error }, metrics);
        }
        let step = match outcome.decision {
            Decision::Speak { text } | Decision::RaiseUnprompted { text } => {
                SettleStep::Spoke(text)
            }
            Decision::Act { calls, intent } => SettleStep::Acted { calls, intent },
            Decision::Pass => SettleStep::Passed,
        };
        (step, metrics)
    }
}

/// ONE step of settlement — the single place a `Decision` becomes speech-or-action,
/// shared by the live heartbeat (`persona::service_loop`, called ONCE per metronome
/// tick) and the eval driver ([`drive_to_settle`], which loops steps because the
/// grader replaces the metronome). Live and eval therefore make a turn the
/// IDENTICAL way — run the cycle over `world`, read the `Decision`, and on `Act`
/// run it once + admit the result as memory. The ONLY difference between the two is
/// pacing (metronome vs synchronous loop), never the per-step motion.
///
/// `may_act` lets the caller pace ACTING without changing the motion: `true` (live,
/// always) runs the act; `false` (eval, past its act budget) returns [`SettleStep::
/// WouldAct`] without executing, so the budget gates acting while still letting a
/// later step settle into a Speak.
///
/// [`TurnFraming`] says how this turn is framed: whether it is addressed TO the
/// persona (a question put to her — the eval exam, a direct @mention, a DM) and
/// whether it is her own self-initiated heartbeat. It only reshapes the system
/// prompt (the silence affordance — see [`Workspace::directed_at_self`] — and the
/// "your own time" framing); the per-step motion is otherwise identical. The
/// burst itself is `impl Into<Burst>`: an attributed `Burst` (live/eval, carries
/// authorship) or a raw `String`/`&str` (collapses to one opaque turn).
pub async fn settle_step(
    cycle: &WorkspaceCycle,
    burst: impl Into<Burst>,
    room_id: Uuid,
    may_act: bool,
    framing: TurnFraming,
) -> (SettleStep, Option<TurnMetrics>) {
    let ws = cycle.run_framed(burst, room_id, framing).await;
    // The cost of THIS tick's deliberation generation — latency + tokens of the
    // model call behind the verdict. Carried out alongside the step so the caller
    // (the eval driver, or the live heartbeat) can accumulate per-turn speed and
    // latency without re-timing the brain. `None` when no verdict carried metrics.
    let metrics = ws.metrics();
    // A FAILED model call is not a verdict and not a silence — surface it LOUD so no
    // failure ever masquerades as a chosen `Pass` ([[fallbacks-are-illegal-fail-loud]]).
    // Checked BEFORE the decision so a fault can never collapse into `Passed` (the
    // swept-model bug: reassign changed the served model, the faculty still requested
    // the old one, the lane refused, and the refusal read as serene silence).
    if let Some(error) = ws.deliberation_fault() {
        return (
            SettleStep::InferenceFailed {
                error: error.to_string(),
            },
            metrics,
        );
    }
    let step = match ws.decision().cloned() {
        Some(Decision::Act { calls, intent }) => {
            if !may_act {
                SettleStep::WouldAct { calls, intent }
            } else {
                match apply_act(cycle, &calls, &intent, room_id).await {
                    Some(_observation) => SettleStep::Acted { calls, intent },
                    None => SettleStep::ActUnfulfilled { calls, intent },
                }
            }
        }
        Some(Decision::Speak { text }) | Some(Decision::RaiseUnprompted { text }) => {
            // Mark the settlement in the volatile buffer: she produced an utterance, so
            // the current concern is answered. This boundary is what lets the next
            // concern legitimately re-issue a tool call identical to one used here
            // without the repeat-perception guard mistaking it for a spin (and it reads
            // as honest proprioception — "I already answered this" — next tick). Only
            // when she has hands; a handless persona never spins on a tool.
            if let Some(body) = cycle.acting() {
                let head: String = text.chars().take(WM_ACTION_HEAD_CHARS).collect();
                body.working_memory.record_settlement(&head);
            }
            SettleStep::Spoke(text)
        }
        Some(Decision::Pass) | None => SettleStep::Passed,
    };
    (step, metrics)
}

/// Epoch-ms wall clock for stamping a self-observation. A real timestamp (not a
/// monotonic tick) so the engram orders correctly against chat messages in recall.
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
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
        assert!(!is_long_running("code/read"), "a file read stays synchronous");
        assert!(!is_long_running("chat/send"));
        assert!(!is_long_running("cargo/test"), "the wrong short name must NOT match");
    }
    use crate::cognition::tool_executor::{
        NativeBatchOutcome, ParsedToolBatch, ToolError, ToolExecutionContext, ToolExecutor,
        ToolOutcome,
    };
    use crate::cognition::workspace::{
        ActingBody, Contribution, Faculty, FacultyId, SalienceArbiter, Workspace,
    };
    use crate::persona::admission_state::AdmissionState;
    use crate::persona::recall_metadata::RecallMetadataRegistry;
    use async_trait::async_trait;
    use std::sync::{Arc, Mutex};

    /// A `ToolExecutor` that records the `context_id` it was handed and returns a
    /// canned per-call result — so a test can assert BOTH that the act was scoped
    /// to the right room and that the observation correlates call→result.
    struct RecordingExecutor {
        seen_context: Mutex<Option<Uuid>>,
        result_content: String,
    }

    #[async_trait]
    impl ToolExecutor for RecordingExecutor {
        async fn execute_native_batch(
            &self,
            calls: &[ToolCall],
            context: &ToolExecutionContext,
            _max_result_chars: usize,
        ) -> Result<NativeBatchOutcome, ToolError> {
            *self.seen_context.lock().unwrap() = Some(context.context_id);
            let results = calls
                .iter()
                .map(|c| crate::ai::types::ToolResult {
                    tool_use_id: c.id.clone(),
                    content: self.result_content.clone(),
                    is_error: None,
                })
                .collect();
            Ok(NativeBatchOutcome {
                results,
                media: Vec::new(),
                stored_ids: Vec::new(),
            })
        }

        async fn parse_response(
            &self,
            _response_text: &str,
            _model_family: Option<&str>,
        ) -> Result<ParsedToolBatch, ToolError> {
            Ok(ParsedToolBatch {
                tool_calls: Vec::new(),
                cleaned_text: String::new(),
                parse_time_us: 0,
            })
        }

        async fn store_outcome(
            &self,
            _outcome: &ToolOutcome,
            _context: &ToolExecutionContext,
        ) -> Result<Uuid, ToolError> {
            Ok(Uuid::nil())
        }
    }

    /// An executor whose batch always fails at the batch level (channel down).
    struct FailingExecutor;
    #[async_trait]
    impl ToolExecutor for FailingExecutor {
        async fn execute_native_batch(
            &self,
            _calls: &[ToolCall],
            _context: &ToolExecutionContext,
            _max_result_chars: usize,
        ) -> Result<NativeBatchOutcome, ToolError> {
            Err(ToolError::ExecutionFailed {
                tool: "code/run".into(),
                underlying: "ipc channel down".into(),
            })
        }
        async fn parse_response(
            &self,
            _t: &str,
            _f: Option<&str>,
        ) -> Result<ParsedToolBatch, ToolError> {
            Ok(ParsedToolBatch {
                tool_calls: Vec::new(),
                cleaned_text: String::new(),
                parse_time_us: 0,
            })
        }
        async fn store_outcome(
            &self,
            _o: &ToolOutcome,
            _c: &ToolExecutionContext,
        ) -> Result<Uuid, ToolError> {
            Ok(Uuid::nil())
        }
    }

    /// Deliberation faculty: reaches for its hands once, then SETTLES into a Speak
    /// the moment it perceives a NEW act-observation it has not yet answered — the
    /// canonical act→observe arc the driver exists to run.
    ///
    /// It perceives its own hands through the working-memory proprioception channel
    /// (the `WorkingMemoryFaculty` stamps each act `[action #n]`), NOT the deleted
    /// world-state fold. It remembers the highest action stamp it has already spoken
    /// about (`responded_through`) so that across SEPARATE concerns — where the
    /// volatile buffer still carries the prior concern's action — it re-awakens and
    /// acts again instead of mistaking old proprioception for "already answered."
    /// That is the faculty remembering its own last conclusion (legitimate, content-
    /// driven), not an iteration counter in the agentic sense.
    struct ActThenSpeak {
        responded_through: std::sync::atomic::AtomicU64,
    }
    impl ActThenSpeak {
        fn new() -> Self {
            Self {
                responded_through: std::sync::atomic::AtomicU64::new(0),
            }
        }
    }
    /// Highest `[action #N]` stamp present in the assembled perception, 0 if none.
    fn latest_action_seq(perceived: &str) -> u64 {
        perceived
            .split("[action #")
            .skip(1)
            .filter_map(|s| {
                let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
                digits.parse::<u64>().ok()
            })
            .max()
            .unwrap_or(0)
    }
    #[async_trait]
    impl Faculty for ActThenSpeak {
        fn id(&self) -> FacultyId {
            FacultyId::Deliberation
        }
        fn reacts_to_broadcast(&self) -> bool {
            true
        }
        async fn contribute(&self, ws: &Workspace) -> Option<Contribution> {
            use std::sync::atomic::Ordering;
            let latest = latest_action_seq(&ws.perceived());
            if latest > self.responded_through.load(Ordering::Relaxed) {
                self.responded_through.store(latest, Ordering::Relaxed);
                Some(Contribution::verdict(
                    Decision::Speak {
                        text: "the answer is 4".into(),
                    },
                    0.9,
                    "settled after observing a fresh result",
                ))
            } else {
                Some(Contribution::verdict(
                    Decision::Act {
                        calls: vec![tool_call()],
                        intent: "run the code".into(),
                    },
                    0.9,
                    "reaching for hands",
                ))
            }
        }
    }

    /// Deliberation faculty that NEVER settles — always wants to act again. Models
    /// the "acts forever" fitness gap the external grader bounds with `max_acts`.
    struct AlwaysAct;
    #[async_trait]
    impl Faculty for AlwaysAct {
        fn id(&self) -> FacultyId {
            FacultyId::Deliberation
        }
        fn reacts_to_broadcast(&self) -> bool {
            true
        }
        async fn contribute(&self, _ws: &Workspace) -> Option<Contribution> {
            Some(Contribution::verdict(
                Decision::Act {
                    calls: vec![tool_call()],
                    intent: "act again".into(),
                },
                0.9,
                "never settles",
            ))
        }
    }

    fn tool_call() -> ToolCall {
        ToolCall {
            id: "call-1".into(),
            name: "code/run".into(),
            input: serde_json::json!({ "lang": "rust", "code": "fn main() { println!(\"{}\", 2 + 2); }" }),
        }
    }

    fn admission() -> Arc<AdmissionState> {
        Arc::new(AdmissionState::new(Arc::new(RecallMetadataRegistry::new())))
    }

    use crate::cognition::working_memory::{WorkingMemory, WorkingMemoryFaculty};

    fn body(executor: Arc<dyn ToolExecutor>, admission: Arc<AdmissionState>) -> Arc<ActingBody> {
        body_with_wm(executor, admission, Arc::new(WorkingMemory::new(3)))
    }

    /// Body that shares a specific working-memory buffer — so a test can wire the
    /// SAME buffer into a `WorkingMemoryFaculty` on the cycle and watch the
    /// perception of the persona's own hands flow act→memory→next-tick perception
    /// (the proprioception channel that replaced the deleted world-state fold).
    fn body_with_wm(
        executor: Arc<dyn ToolExecutor>,
        admission: Arc<AdmissionState>,
        working_memory: Arc<WorkingMemory>,
    ) -> Arc<ActingBody> {
        Arc::new(ActingBody {
            persona_id: Uuid::new_v4(),
            persona_name: "Asha".into(),
            executor,
            admission,
            working_memory,
        })
    }

    // what this catches: an act is scoped to the room it is FOR (one mind is in
    // many rooms — the body is room-agnostic, `room_id` flows per-call), the
    // observation correlates each call to its result in first person, and the
    // outcome becomes a recallable engram. Regresses the multi-room steer + the
    // result-as-memory choice (ACTING-ORGANISM §3.3).
    #[tokio::test]
    async fn apply_act_scopes_to_the_room_and_observes_the_result() {
        let exec = Arc::new(RecordingExecutor {
            seen_context: Mutex::new(None),
            result_content: "4\n".into(),
        });
        let adm = admission();
        let cycle = WorkspaceCycle::new(Vec::new(), Arc::new(SalienceArbiter), 8)
            .with_acting(body(exec.clone(), adm.clone()));

        let room = Uuid::new_v4();
        let observation = apply_act(&cycle, &[tool_call()], "check the math", room)
            .await
            .expect("acted");

        assert_eq!(
            *exec.seen_context.lock().unwrap(),
            Some(room),
            "the act must be scoped to the room it is for, not a phantom nil room"
        );
        assert!(observation.contains("code/run"), "names the tool it ran");
        assert!(observation.contains("check the math"), "carries the intent");
        assert!(
            observation.contains('4'),
            "folds in the result the hand returned"
        );
        assert_eq!(
            adm.engram_count(),
            1,
            "the outcome became a recallable memory (result-as-engram)"
        );
        assert!(adm
            .engram_at(0)
            .expect("engram present")
            .content
            .contains('4'));
    }

    // what this catches: with no hands (no ActingBody on the cycle), the driver
    // ABSTAINS rather than fabricating a result — the no-hands path that used to
    // live in the faculty now lives here (the faculty only emits the Act verdict).
    #[tokio::test]
    async fn apply_act_without_hands_abstains() {
        let cycle = WorkspaceCycle::new(Vec::new(), Arc::new(SalienceArbiter), 8);
        assert!(
            apply_act(&cycle, &[tool_call()], "try", Uuid::new_v4())
                .await
                .is_none(),
            "no hands → None, never a fabricated success"
        );
    }

    // what this catches: a batch-level executor failure (channel down) abstains
    // rather than admitting a fabricated outcome the mind would then "remember" as
    // fact ([[fallbacks-are-illegal-fail-loud]]).
    #[tokio::test]
    async fn apply_act_abstains_when_the_hand_fails() {
        let adm = admission();
        let cycle = WorkspaceCycle::new(Vec::new(), Arc::new(SalienceArbiter), 8)
            .with_acting(body(Arc::new(FailingExecutor), adm.clone()));
        assert!(apply_act(&cycle, &[tool_call()], "run", Uuid::new_v4())
            .await
            .is_none());
        assert_eq!(adm.engram_count(), 0, "a failed act admits no memory");
    }

    // what this catches: the act→observe MOTION — the driver runs the act, folds
    // the observation into the next perception, and the mind settles into a Speak
    // that the external observer reads. acts==1, spoken is the settled answer.
    #[tokio::test]
    async fn drive_to_settle_acts_then_settles_on_speak() {
        let exec = Arc::new(RecordingExecutor {
            seen_context: Mutex::new(None),
            result_content: "4".into(),
        });
        let adm = admission();
        // Same buffer in the body (writer) and the perception-tier faculty
        // (reader): act → working memory → next-tick perception.
        let wm = Arc::new(WorkingMemory::new(3));
        let cycle = WorkspaceCycle::new(
            vec![
                Arc::new(WorkingMemoryFaculty::new(Arc::clone(&wm))) as Arc<dyn Faculty>,
                Arc::new(ActThenSpeak::new()),
            ],
            Arc::new(SalienceArbiter),
            8,
        )
        .with_acting(body_with_wm(exec.clone(), adm.clone(), Arc::clone(&wm)));

        let outcome =
            drive_to_settle(&cycle, "[eval]\npeer: what is 2+2?", Uuid::new_v4(), 8, TurnFraming::ambient()).await;

        assert_eq!(outcome.acts, 1, "acted exactly once before settling");
        assert_eq!(outcome.spoken.as_deref(), Some("the answer is 4"));
        assert!(matches!(outcome.decision, Decision::Speak { .. }));
    }

    // what this catches: the grader's stopwatch. A mind that never settles is
    // bounded by the EXTERNAL `max_acts` budget and the final un-driven Act is
    // returned as unfinished — never a fabricated answer, and the budget is the
    // observer's, not a cap in the persona's head (ACTING-ORGANISM §4).
    #[tokio::test]
    async fn drive_to_settle_returns_unsettled_act_when_budget_exhausted() {
        let exec = Arc::new(RecordingExecutor {
            seen_context: Mutex::new(None),
            result_content: "...".into(),
        });
        let adm = admission();
        let cycle = WorkspaceCycle::new(vec![Arc::new(AlwaysAct)], Arc::new(SalienceArbiter), 8)
            .with_acting(body(exec.clone(), adm.clone()));

        let outcome = drive_to_settle(&cycle, "go", Uuid::new_v4(), 2, TurnFraming::ambient()).await;

        assert_eq!(outcome.acts, 2, "spent exactly the observer's budget");
        assert!(
            outcome.spoken.is_none(),
            "did not settle → no spoken answer"
        );
        assert!(
            matches!(outcome.decision, Decision::Act { .. }),
            "returns the un-driven Act as honest 'did not finish'"
        );
    }

    // what this catches: the shared step's acting gate. `may_act = false` (how the
    // eval driver paces ACTING past its budget) must return the decided Act WITHOUT
    // executing it — the executor is never touched — so a deferred act can't run a
    // tool the budget already forbade. `may_act = true` (the live path, always) runs
    // it. This is the single seam that keeps live (one permitted act per tick) and
    // eval (budget-gated acting) on the IDENTICAL per-step motion.
    #[tokio::test]
    async fn settle_step_defers_the_act_without_executing_when_may_act_is_false() {
        let exec = Arc::new(RecordingExecutor {
            seen_context: Mutex::new(None),
            result_content: "...".into(),
        });
        let adm = admission();
        let cycle = WorkspaceCycle::new(vec![Arc::new(AlwaysAct)], Arc::new(SalienceArbiter), 8)
            .with_acting(body(exec.clone(), adm.clone()));

        let (deferred, _) = settle_step(&cycle, "go", Uuid::new_v4(), false, TurnFraming::ambient()).await;
        assert!(
            matches!(deferred, SettleStep::WouldAct { .. }),
            "may_act=false defers the act"
        );
        assert!(
            exec.seen_context.lock().unwrap().is_none(),
            "a deferred act NEVER touches the executor"
        );

        let (ran, _) = settle_step(&cycle, "go", Uuid::new_v4(), true, TurnFraming::ambient()).await;
        assert!(matches!(ran, SettleStep::Acted { .. }), "may_act=true runs it");
        assert!(
            exec.seen_context.lock().unwrap().is_some(),
            "a permitted act DOES reach the executor"
        );
    }

    /// A `ToolExecutor` that returns a DIFFERENT canned result for each
    /// successive call — so a multi-act investigation accumulates DISTINCT
    /// observations in memory (act 1 brings back one fact, act 2 another).
    /// Models hands that probe the world and learn something new each reach.
    struct ScriptedExecutor {
        results: Mutex<std::collections::VecDeque<String>>,
    }
    impl ScriptedExecutor {
        fn new(results: impl IntoIterator<Item = &'static str>) -> Self {
            Self {
                results: Mutex::new(results.into_iter().map(String::from).collect()),
            }
        }
    }
    #[async_trait]
    impl ToolExecutor for ScriptedExecutor {
        async fn execute_native_batch(
            &self,
            calls: &[ToolCall],
            _context: &ToolExecutionContext,
            _max_result_chars: usize,
        ) -> Result<NativeBatchOutcome, ToolError> {
            let content = self
                .results
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| "no more results".into());
            let results = calls
                .iter()
                .map(|c| crate::ai::types::ToolResult {
                    tool_use_id: c.id.clone(),
                    content: content.clone(),
                    is_error: None,
                })
                .collect();
            Ok(NativeBatchOutcome {
                results,
                media: Vec::new(),
                stored_ids: Vec::new(),
            })
        }
        async fn parse_response(
            &self,
            _t: &str,
            _f: Option<&str>,
        ) -> Result<ParsedToolBatch, ToolError> {
            Ok(ParsedToolBatch {
                tool_calls: Vec::new(),
                cleaned_text: String::new(),
                parse_time_us: 0,
            })
        }
        async fn store_outcome(
            &self,
            _o: &ToolOutcome,
            _c: &ToolExecutionContext,
        ) -> Result<Uuid, ToolError> {
            Ok(Uuid::nil())
        }
    }

    /// A deliberation faculty that is a small *investigator*: its next move is a
    /// pure function of what it has DISCOVERED so far (the observations folded
    /// into perception), never a counter and never a magic flag. It needs two
    /// facts to answer — where the program starts and what that entry calls —
    /// so it acts to learn the first, acts again to learn the second, then
    /// (seeing both in memory) synthesizes and speaks. The branch reads
    /// accumulated MEMORY CONTENT, which is the whole point: the hands change
    /// the mind. (A test stand-in for the model, exactly like `ActThenSpeak`;
    /// it proves the cycle's cross-tick plumbing, not production cognition.)
    struct Investigator;
    #[async_trait]
    impl Faculty for Investigator {
        fn id(&self) -> FacultyId {
            FacultyId::Deliberation
        }
        fn reacts_to_broadcast(&self) -> bool {
            true
        }
        async fn contribute(&self, ws: &Workspace) -> Option<Contribution> {
            // Reads what it has DISCOVERED from assembled perception — the
            // working-memory render of its own recent acts (proprioception),
            // where each act-observation head carries the executor's result tokens
            // (ENTRY=…/CALLS=…). Not the raw burst, not the deleted fold.
            let perceived = ws.perceived();
            let after = |key: &str| -> Option<String> {
                perceived
                    .split(key)
                    .nth(1)
                    .and_then(|s| s.split_whitespace().next())
                    .map(String::from)
            };
            match (after("ENTRY="), after("CALLS=")) {
                (None, _) => Some(Contribution::verdict(
                    Decision::Act {
                        calls: vec![probe("find where the program starts")],
                        intent: "find where the program starts".into(),
                    },
                    0.9,
                    "I don't know the entry point yet — reach for it",
                )),
                (Some(_), None) => Some(Contribution::verdict(
                    Decision::Act {
                        calls: vec![probe("see what the entry calls")],
                        intent: "find what the entry calls".into(),
                    },
                    0.9,
                    "I know the entry; now I need what it calls",
                )),
                (Some(entry), Some(calls)) => Some(Contribution::verdict(
                    Decision::Speak {
                        text: format!("the program starts in {entry} and calls {calls}"),
                    },
                    0.95,
                    "synthesized both discoveries from memory — settling",
                )),
            }
        }
    }

    /// A probe tool call whose INPUT carries no `ENTRY=`/`CALLS=` token (only the
    /// executor's *result* does), so the investigator never false-triggers on
    /// its own intent.
    fn probe(what: &'static str) -> ToolCall {
        ToolCall {
            id: "probe-1".into(),
            name: "code/search".into(),
            input: serde_json::json!({ "query": what }),
        }
    }

    // what this catches: THE HANDS CHANGE THE MIND. A multi-act investigation
    // converges through MEMORY CONTENT — each act discovers a distinct fact, the
    // result re-enters as an Episodic engram, and the NEXT decision is a function
    // of what the persona now KNOWS (not an act counter, not a `[you just acted]`
    // flag). She acts to find the entry point, observes it, acts to find what it
    // calls, observes that, then — seeing BOTH discoveries in her assembled
    // perception — synthesizes and speaks. This is the organic loop
    // cognition→action→perception→cognition converging by judgment, the novel
    // architecture that distinguishes the organism from a textbook agentic
    // counter loop ([[persona-codes-blind-no-hands-no-organic-loop]]).
    #[tokio::test]
    async fn the_hands_change_the_mind_across_a_multi_act_investigation() {
        let exec = Arc::new(ScriptedExecutor::new(["ENTRY=main", "CALLS=boot"]));
        let adm = admission();
        let wm = Arc::new(WorkingMemory::new(3));
        let cycle = WorkspaceCycle::new(
            vec![
                Arc::new(WorkingMemoryFaculty::new(Arc::clone(&wm))) as Arc<dyn Faculty>,
                Arc::new(Investigator),
            ],
            Arc::new(SalienceArbiter),
            8,
        )
        .with_acting(body_with_wm(exec.clone(), adm.clone(), Arc::clone(&wm)));

        let outcome = drive_to_settle(
            &cycle,
            "[eval]\npeer: where does the program start and what does it call?",
            Uuid::new_v4(),
            8,
            TurnFraming::ambient(),
        )
        .await;

        assert_eq!(
            outcome.acts, 2,
            "two DISCOVERIES were needed to converge — multi-act, not one-shot"
        );
        let spoken = outcome
            .spoken
            .expect("the mind settled into a spoken synthesis, not an un-driven act");
        assert!(
            spoken.contains("main"),
            "the answer carries the FIRST discovery (entry point) — proof act 1 entered memory"
        );
        assert!(
            spoken.contains("boot"),
            "the answer carries the SECOND discovery (what it calls) — proof act 2 entered memory"
        );
        assert_eq!(
            adm.engram_count(),
            2,
            "each discovery became a durable memory the mind perceived next tick"
        );
    }

    // what this catches: the repeat-perception short-circuit. An IDENTICAL, already-
    // satisfied call this turn must NOT re-execute — the greedy re-emission that spun
    // `commands/list` forever in the nil-room eval (proven live 2026-07-02): working
    // memory already carried the result, yet the model re-issued the byte-identical
    // call every act and never answered. `apply_act` now detects the satisfied
    // `(name, args)` in working memory, skips the hand, and records an explicit
    // "already ran it; answer now" proprioception so the redundancy is PERCEIVED rather
    // than merely present via a stamp shift the greedy decode ignores. A MIXED batch (a
    // genuinely new call) still runs — proven by
    // `the_hands_change_the_mind_across_a_multi_act_investigation` (two DISTINCT calls
    // both execute). Content-driven, not an iteration counter
    // ([[persona-tool-loop-act-then-report]], [[no-hardcoded-heuristics-to-steer-cognition]]).
    #[tokio::test]
    async fn identical_already_satisfied_act_does_not_re_execute() {
        // Two queued results: only the FIRST may ever be popped. If the identical
        // second act reached the hand, the queue would drain by one more — the length
        // assertion below catches exactly that.
        let exec = Arc::new(ScriptedExecutor::new(["4\n", "SECOND-MUST-NOT-POP"]));
        let adm = admission();
        let wm = Arc::new(WorkingMemory::new(4));
        let cycle = WorkspaceCycle::new(Vec::new(), Arc::new(SalienceArbiter), 8)
            .with_acting(body_with_wm(exec.clone(), adm.clone(), Arc::clone(&wm)));
        let room = Uuid::new_v4();

        // First act genuinely runs; its result lands in working memory.
        let first = apply_act(&cycle, &[tool_call()], "check the math", room)
            .await
            .expect("first act runs");
        assert!(first.contains("code/run"), "first act names the tool it ran");
        assert_eq!(
            exec.results.lock().unwrap().len(),
            1,
            "first act popped exactly one result off the hand"
        );

        // Second, byte-identical act: already satisfied → short-circuit, no re-run.
        let second = apply_act(&cycle, &[tool_call()], "check the math", room)
            .await
            .expect("short-circuit still returns Some — it counts as an act, honestly");
        assert!(
            second.contains("already ran"),
            "records explicit answer-now proprioception instead of another result: {second}"
        );
        assert_eq!(
            exec.results.lock().unwrap().len(),
            1,
            "the identical call NEVER reached the hand a second time (queue undrained)"
        );
    }

    // what this catches: SETTLE IS A REST, NOT A HALT — the metronome does not
    // crank to a halt after one answer. The SAME mind (same cycle, same body,
    // same accumulating memory) settles concern A, then RE-AWAKENS on a fresh
    // concern B and runs the act→observe→speak arc again. Her concern-A memory
    // persists across the two drives (continuity of self), and she still engages
    // B. Proves the organism keeps breathing across concerns: a settle is the
    // judgment "the work is done for now," never a terminus.
    #[tokio::test]
    async fn it_settles_then_re_awakens_without_cranking_to_a_halt() {
        // Distinct per-concern observations: identical content would be a
        // content-addressed dedup no-op in memory (correct substrate behavior,
        // [[embeddings-are-per-content-computed-once-shared]]), which would mask
        // the continuity-of-self assertion below.
        let exec = Arc::new(ScriptedExecutor::new(["learned about A", "learned about B"]));
        let adm = admission();
        // One living mind: the working-memory buffer accumulates ACROSS both
        // concern-drives (volatile continuity), so `ActThenSpeak` must re-awaken on
        // concern B by perceiving a NEW act stamp rather than mistaking concern A's
        // still-buffered proprioception for "already answered".
        let wm = Arc::new(WorkingMemory::new(3));
        let cycle = WorkspaceCycle::new(
            vec![
                Arc::new(WorkingMemoryFaculty::new(Arc::clone(&wm))) as Arc<dyn Faculty>,
                Arc::new(ActThenSpeak::new()),
            ],
            Arc::new(SalienceArbiter),
            8,
        )
        .with_acting(body_with_wm(exec.clone(), adm.clone(), Arc::clone(&wm)));
        let room = Uuid::new_v4();

        // Concern A: act → observe → settle on a Speak.
        let a = drive_to_settle(&cycle, "[eval]\npeer: concern A?", room, 8, TurnFraming::ambient()).await;
        assert_eq!(a.acts, 1, "settled concern A after one act→observe");
        assert!(a.spoken.is_some(), "concern A got a spoken answer");
        assert_eq!(adm.engram_count(), 1, "concern A left exactly one memory");

        // Concern B on the SAME living mind — it must wake again, not stay halted.
        let b = drive_to_settle(&cycle, "[eval]\npeer: a totally different concern B?", room, 8, TurnFraming::ambient()).await;
        assert_eq!(
            b.acts, 1,
            "the mind RE-AWAKENED and acted on the new concern — not stuck post-settle"
        );
        assert!(b.spoken.is_some(), "and settled concern B too");
        assert_eq!(
            adm.engram_count(),
            2,
            "continuity of self: concern-A memory persisted, concern B added its own"
        );
    }

    /// what this catches: `SettleStep::from_settled` mis-projecting a driven
    /// `SettleOutcome` onto the live turn handler — the seam that lets a DIRECTED
    /// live turn `drive_to_settle` and feed the ONE existing turn match (no parallel
    /// handler). A regression here would route a settled Speak to silence, hide an
    /// inference failure behind a serene Pass ([[fallbacks-are-illegal-fail-loud]]),
    /// or drop an over-budget Act instead of re-perceiving next tick.
    fn outcome_with(decision: Decision, inference_error: Option<String>) -> SettleOutcome {
        SettleOutcome {
            spoken: match &decision {
                Decision::Speak { text } | Decision::RaiseUnprompted { text } => {
                    Some(text.clone())
                }
                _ => None,
            },
            decision,
            acts: 0,
            world_state: String::new(),
            metrics: TurnMetrics::default(),
            inference_error,
        }
    }

    #[test]
    fn from_settled_projects_every_terminal_outcome_onto_the_live_handler() {
        // Speak → Spoke (the prose reaches the room).
        let (step, m) = SettleStep::from_settled(outcome_with(
            Decision::Speak {
                text: "hello".into(),
            },
            None,
        ));
        assert!(matches!(step, SettleStep::Spoke(t) if t == "hello"));
        assert!(m.is_some(), "metrics always carry through");

        // RaiseUnprompted also speaks — initiative is still an utterance.
        let (step, _) = SettleStep::from_settled(outcome_with(
            Decision::RaiseUnprompted {
                text: "idea".into(),
            },
            None,
        ));
        assert!(matches!(step, SettleStep::Spoke(t) if t == "idea"));

        // Budget-spent Act → Acted (results already in memory; live handler
        // re-perceives next tick — the honest long-tail degrade).
        let (step, _) = SettleStep::from_settled(outcome_with(
            Decision::Act {
                calls: vec![],
                intent: "kept gathering".into(),
            },
            None,
        ));
        assert!(matches!(step, SettleStep::Acted { .. }));

        // Pass → Passed (chosen silence, honored).
        let (step, _) = SettleStep::from_settled(outcome_with(Decision::Pass, None));
        assert!(matches!(step, SettleStep::Passed));

        // inference_error present → InferenceFailed, REGARDLESS of decision — a
        // failed model is never a chosen silence.
        let (step, _) = SettleStep::from_settled(outcome_with(
            Decision::Pass,
            Some("lane refused model".into()),
        ));
        assert!(
            matches!(step, SettleStep::InferenceFailed { error } if error == "lane refused model"),
            "an inference failure must surface LOUD, never collapse to Passed"
        );
    }
}
