//! Settlement stepping: `settle_step` — the ONE place a `Decision` becomes
//! speech-or-action — and `drive_to_settle`, which loops steps under an external
//! budget until the mind settles. Extracted verbatim from `act_observe` (pure
//! code-motion, #386 decomposition).
//!
//! `settle_step` is shared by the live heartbeat (called ONCE per metronome tick)
//! and the eval driver (`drive_to_settle`, which loops because the grader replaces
//! the metronome). Live and eval make a turn the IDENTICAL way; only pacing differs.

use uuid::Uuid;

use crate::ai::types::ToolCall;
use crate::cognition::workspace::{
    Burst, Decision, Situation, TurnFraming, TurnMetrics, WorkspaceCycle,
};

use super::apply::apply_act;
use super::perception::{
    any_real_receipt, claimed_file_without_act, collect_touched_paths,
    mutated_workspace, wrote_without_observation,
};
use super::types::{SettleOutcome, SettleStep};

// The working-memory trail-head bound lives in `working_memory.rs` now (its home — WM owns
// its own truncation). Still used here for the settlement answer-head.




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
    // The turn's investigation trail (see `SettleOutcome::touched_paths`).
    let mut touched: Vec<String> = Vec::new();
    // Fold each tick's deliberation cost in, so the settled outcome reports the
    // task's TOTAL speed/latency (a multi-act task pays for every generation).
    let mut metrics = TurnMetrics::default();

    // Signature of a tick's tool batch for loop-detection: `name|args` per call, the random
    // per-call `id` excluded, sorted so batch order doesn't matter. Two ticks with the same
    // signature emitted the byte-identical action.
    fn calls_signature(calls: &[ToolCall]) -> String {
        let mut parts: Vec<String> = calls
            .iter()
            .map(|c| format!("{}|{}", c.name, serde_json::to_string(&c.input).unwrap_or_default()))
            .collect();
        parts.sort();
        parts.join(",")
    }
    // BOUNDED STUCK-ACT BACKSTOP (#206). The escalating repeat-proprioception makes a looping
    // model's perception genuinely shift, but a determined greedy model can still re-emit the
    // SAME act every tick (glass-boxed: `commands/help` ×54, then after the escalation fix an
    // identical `code/write` ×8) — each a dedup no-op the short-circuit guard already refuses
    // to execute, burning the whole act budget on nothing. This bounds that: after
    // STUCK_LIMIT consecutive byte-identical acts, stop GRANTING acts (`may_act=false`) so she
    // must settle into a Speak/Pass from what she has. It is NOT a steer — it never says WHAT
    // to do, exactly like the `max_acts` budget cutoff; it only stops feeding a detected
    // fixed-point loop, and it's personhood-POSITIVE: it returns her to think→speech instead
    // of hammering. GENUINE iteration is untouched — a refined write has a DIFFERENT signature,
    // so the counter resets; only a fixed point (identical batch, over and over) trips it.
    // [[repetition-brick-fires-but-does-not-break-the-loop]], [[no-hardcoded-heuristics-to-steer-cognition]].
    const STUCK_LIMIT: usize = 3;
    let mut prev_sig: Option<String> = None;
    let mut stuck = 0usize;
    // A workspace-deliverable turn re-perceives on a zero-deliverable Speak (see the Spoke
    // arm). This used to be ONE-SHOT, and the glass box showed what that costs: on
    // sympy-21379 `persona.settle.no_deliverable` fired exactly once per run and she then
    // settled at 5-7 acts with a 30-act budget unspent. The latch was guarding against a
    // retry loop, but it also capped her at a single reminder for the whole turn.
    //
    // The bound that actually prevents a loop without capping her: re-fire only if she has
    // ACTED since the last nudge. A nudge that earns an act has earned another; two Speaks
    // in a row with no act between them means the nudge is not working, so she settles
    // rather than being trapped. Her own behavior is the budget — no counter, no constant.
    let mut acts_at_last_nudge: Option<usize> = None;

    // #386: transient-deliberation retry. Glass-boxed on atlas-17139-h1
    // (2026-08-09): the per-slot wedge fails ~1 in 3 generations and the VERY
    // NEXT generation succeeds — capture tick 2 faults, tick 3 acts, tick 5
    // faults, tick 6 acts. The old behaviour abandoned the ENTIRE turn on the
    // first fault, so a single transient flicker cost her a whole attempt. A
    // faulted thought is retried in place, bounded, before the turn is
    // surrendered — the flicker costs one re-generation, not the turn. Only a
    // SUSTAINED fault (budget spent across the turn) ends the turn with
    // inference_error, exactly as before — 4+ faults in one turn IS a wedge worth
    // surrendering to, not a flicker. Whole-turn budget, not per-tick.
    const DELIBERATION_RETRY_BUDGET: u32 = 3;
    let mut delib_retries: u32 = 0;

    loop {
        // ONE settlement step through the SHARED primitive the live heartbeat uses
        // (`settle_step`). The only thing this driver adds is the LOOP — because the
        // eval room has no metronome, the grader re-perceives by calling step again.
        // `may_act = acts < max_acts` gates ACTING (not speaking): past the budget
        // she may still settle into a Speak, but a fresh Act is returned un-driven.
        //
        // The tick's SITUATION is the real signal that makes context lean when she's
        // heads-down: the first tick is a fresh ask (`FreshContext`, fuller
        // grounding); every tick AFTER an act has landed re-perceives a tool result
        // (`PostAction`), so the focuser drops the standing re-grounding and the
        // result + working memory own the window. Derived from `acts`, never from the
        // burst text.
        let situation = if acts == 0 {
            Situation::FreshContext
        } else {
            Situation::PostAction
        };
        // may_act gates ACTING (not speaking): past the act budget OR once she is provably
        // stuck re-emitting the identical act, a fresh Act is returned un-driven and she must
        // settle into a Speak/Pass. Speaking is never gated.
        let may_act = acts < max_acts && stuck < STUCK_LIMIT;
        let (step, step_metrics) =
            settle_step(cycle, burst.clone(), room_id, may_act, framing, situation).await;
        if let Some(m) = step_metrics {
            metrics.accumulate(m);
        }
        match step {
            SettleStep::Spoke(text) => {
                // THE SETTLE ARTERY (glass-boxed 2026-08-04, sympy-21379): every perception
                // fact the Speak arm records — [unfulfilled], [unacted], [unobserved],
                // [confabulation] — lands in working memory AFTER the decision. On this
                // path the Speak returned immediately, so she never got a tick to PERCEIVE
                // her own diagnosis. The machinery wrote it; nobody read it. Live specimen:
                // one `code/tree`, then a Speak explaining the bug to the user in prose —
                // acts 1, patch 0 bytes, budget unspent, run over.
                //
                // When the caller declared the deliverable to be the WORKSPACE, a Speak
                // that changed no file has produced nothing the grader will ever see. Hand
                // her ONE re-perception carrying that structural fact, then let her decide.
                // Bounded to one: if she speaks again she settles, so the ceiling is a
                // single extra generation and a determined Speak is never trapped.
                //
                // Why this is substrate, not scaffolding: the fact is TRUE and structural
                // (the caller declared the contract; working memory holds no mutation
                // receipt), it names no file, no fix, and no next tool, and the decision
                // stays entirely hers — the same shape as every other proprioception fact
                // in `settle_step`. [[no-hardcoded-heuristics-to-steer-cognition]],
                // [[fix-the-substrate-never-rig-the-persona-the-line-between-assist-and-scaffold]].
                if framing.workspace_deliverable && acts_at_last_nudge != Some(acts) {
                    if let Some(body) = cycle.acting() {
                        if !mutated_workspace(&body.working_memory.recent_entries()) {
                            acts_at_last_nudge = Some(acts);
                            body.working_memory.record_fact(
                                "[no-deliverable] I settled by speaking, and my working \
                                 memory holds no act of mine that changed a file. This \
                                 task is judged by the state of the workspace, not by what \
                                 I say about it — an explanation of a fix is not the fix.",
                            );
                            crate::probe!(
                                class = "persona.settle.no_deliverable",
                                persona = %body.persona_name,
                                room_id = %room_id,
                                acts = acts,
                                "workspace-deliverable turn spoke with no mutation receipt — recorded the fact and re-perceived (re-fires only after she acts again)"
                            );
                            continue;
                        }
                    }
                }
                return SettleOutcome {
                    spoken: Some(text.clone()),
                    decision: Decision::Speak { text },
                    acts,
                    world_state: burst.rendered.clone(),
                    metrics,
                    inference_error: None,
                    touched_paths: touched,
                };
            }
            SettleStep::Acted { calls, .. } => {
                acts += 1;
                collect_touched_paths(&mut touched, &calls);
                // THE DELIVERABLE FACT BELONGS ON THE ACT PATH, NOT ONLY ON SETTLE.
                //
                // It used to live exclusively in the Spoke arm, so it could only reach a
                // persona who SETTLED. Measured on sympy-21379 v14: she spent all 30 acts, was
                // still acting when the budget ran out, and the driver returned the final Act
                // un-driven — the Spoke arm never ran, the fact never fired once, and she
                // finished a full-length run having changed no file without ever being told
                // that changing files was the point. The one population that needs the
                // reminder — a mind thrashing through its whole budget — was the one the
                // mechanism structurally could not reach.
                //
                // Now it rides the re-perception after every act. Same truth, same veto on
                // spam (`acts_at_last_nudge` re-arms only when she has acted since), and the
                // act count is IN the text so consecutive facts are not byte-identical — a
                // stationary perception is a fixed point under greedy decoding, which is the
                // #206 failure this file already documents.
                if framing.workspace_deliverable && acts_at_last_nudge != Some(acts) {
                    if let Some(body) = cycle.acting() {
                        if !mutated_workspace(&body.working_memory.recent_entries()) {
                            acts_at_last_nudge = Some(acts);
                            body.working_memory.record_fact(&format!(
                                "[no-deliverable] I have taken {acts} actions on this task and \
                                 my working memory holds no act of mine that changed a file. \
                                 This task is judged by the state of the workspace, not by what \
                                 I say about it — an explanation of a fix is not the fix."
                            ));
                            crate::probe!(
                                class = "persona.act.no_deliverable_yet",
                                persona = %body.persona_name,
                                room_id = %room_id,
                                acts = acts,
                                "acted with no workspace mutation receipt yet — recorded the fact on the act path (the settle path cannot reach a budget-exhausted turn)"
                            );
                        }
                    }
                }
                // Loop-detection: a byte-identical batch back-to-back is the fixed point the
                // backstop bounds (the short-circuit guard already refused to re-execute it).
                // A genuinely different act resets the counter, so real iteration is free.
                let sig = calls_signature(&calls);
                if prev_sig.as_deref() == Some(sig.as_str()) {
                    stuck += 1;
                    if stuck >= STUCK_LIMIT {
                        crate::probe!(
                            class = "persona.settle.stuck_backstop",
                            room_id = %room_id,
                            acts = acts,
                            stuck = stuck,
                            "identical act repeated to the stuck limit — withholding further acts so she settles into speech (#206 backstop)"
                        );
                    }
                } else {
                    stuck = 0;
                }
                prev_sig = Some(sig);
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
                    touched_paths: touched,
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
                    touched_paths: touched,
                };
            }
            // The model call FAILED — no verdict this task. Return LOUD: carry the
            // cause so the grader scores an infra failure, never a fabricated answer
            // and never a silent `Pass` ([[fallbacks-are-illegal-fail-loud]]). We do
            // not loop/retry here — the grader owns retry policy; the settle loop's
            // job is to report the truth of THIS attempt.
            SettleStep::InferenceFailed { error } => {
                // #386 transient-deliberation retry. A faulted generation is NOT
                // yet a surrendered turn — glass-box (atlas-17139-h1) proved the
                // very next generation succeeds ~2/3 of the time. Retry the thought
                // in place, bounded, and ALWAYS probe so the wedge signal stays
                // visible to the concurrency investigation (the retry recovers the
                // turn; it must never HIDE the fault). Only a SUSTAINED fault
                // (budget spent) surrenders the turn with inference_error, exactly
                // as before.
                if delib_retries < DELIBERATION_RETRY_BUDGET {
                    delib_retries += 1;
                    crate::probe!(
                        class = "persona.settle.deliberation_retry",
                        room_id = %room_id,
                        acts = acts,
                        retry = delib_retries,
                        budget = DELIBERATION_RETRY_BUDGET,
                        error = %error,
                        "transient deliberation fault — retrying the thought in place (the turn is not yet surrendered)"
                    );
                    continue;
                }
                return SettleOutcome {
                    decision: Decision::Pass,
                    spoken: None,
                    acts,
                    world_state: burst.rendered.clone(),
                    metrics,
                    inference_error: Some(error),
                    touched_paths: touched,
                };
            }
        }
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
    situation: Situation,
) -> (SettleStep, Option<TurnMetrics>) {
    let burst: Burst = burst.into();
    // Snapshot the burst's PEER turns before the workspace consumes it — the
    // draft-side echo check (#303) below compares her settled utterance
    // against exactly what she reasoned over, so the evidence can never
    // scroll out of the window between generation and the fact.
    let peer_turns: Vec<crate::cognition::workspace::BurstTurn> = burst
        .turns
        .iter()
        .filter(|t| !t.is_self && !t.author.trim().is_empty())
        .cloned()
        .collect();
    let ws = cycle.run_situated(burst, room_id, framing, situation).await;
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
                // The typed outcome: `Acted` (any act ran or was short-circuited) →
                // re-perceive next step; `NoHands`/`ExecutorError` → unfulfilled. Behavior
                // identical to the old `Some`/`None`, but `ExecutorError` is now
                // distinguishable for a future backstop.
                if apply_act(cycle, &calls, &intent, room_id)
                    .await
                    .produced_an_act()
                {
                    SettleStep::Acted { calls, intent }
                } else {
                    SettleStep::ActUnfulfilled { calls, intent }
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
                // Snapshot the concern BEFORE the settlement marker lands, so the
                // observation scan below sees this concern's acts, not an empty tail.
                // TYPED concern snapshot BEFORE the settlement marker lands — the
                // proprioception backstops below (confabulation / unobserved / unacted-claim)
                // read the typed acts (verb + paths) off these entries, not receipt prose,
                // so they survive the seam-5 live-drift the `I ran …` scans died on.
                let pre_settle = body.working_memory.recent_entries();
                let head: String = text
                    .chars()
                    .take(body.working_memory.budget().trail_head_chars())
                    .collect();
                body.working_memory.record_settlement(&head);
                // Unfulfilled-promise backstop (#122): a Speak that NARRATES action
                // (first-person intent + fence) which nothing lifted/executed —
                // reaching this arm means no format recognized it — leaves her
                // believing work happened that never did (the shared-hallucinated-
                // workspace failure, 2026-07-09). Record the structural fact as
                // proprioception so next tick she perceives her own unkept promise.
                // Perception-side only, mirrors the answer-now nudge above; never a
                // gate on her output ([[no-hardcoded-heuristics-to-steer-cognition]]).
                let fenced = crate::ai::json_in_prompt_tools::narrates_fenced_action(&text);
                // The fence-less sibling (Atlas's live loop, 2026-07-10): intent
                // capped with a `[writing test files]` stage direction — theater,
                // not action. Same proprioception backstop.
                let staged = crate::ai::json_in_prompt_tools::narrates_stage_direction(&text);
                // The confabulation backstop (Joel, 2026-07-11): under a peer's
                // verification pressure Atlas upgraded from stage directions to
                // plausible fenced FILE CONTENTS no tool ever produced. A fence
                // alone is legitimate drafting; a fence spoken while her memory
                // already carries an unkept promise — and still no tool ran —
                // is presenting composition as workspace truth. Evidence-gated
                // on the existing [unfulfilled] state so drafting is never
                // taxed; perception-side fact, never an output gate.
                let unverified = !fenced
                    && !staged
                    && crate::ai::json_in_prompt_tools::has_fenced_block(&text)
                    && body
                        .working_memory
                        .recent()
                        .iter()
                        .any(|l| l.contains("[unfulfilled]"));
                if unverified {
                    body.working_memory.record_fact(
                        "[unverified] I presented fenced content while my earlier \
                         promised actions still never ran — that text is composed, \
                         not read from the workspace. Only a tool result can show \
                         real file contents.",
                    );
                    crate::probe!(
                        class = "persona.act.unverified_artifact",
                        persona = %body.persona_name,
                        room_id = %room_id,
                        "fenced content presented with outstanding unfulfilled promises and no act this turn — recorded unverified-artifact proprioception"
                    );
                }
                if fenced || staged {
                    body.working_memory.record_fact(if fenced {
                        // The name-diagnosis tail (2026-07-12): the room looped an
                        // INVENTED tool name (`file_tree`) for an hour while this
                        // fact told them only THAT nothing ran, never WHY — 56
                        // firings with zero behavior change. "It didn't run" without
                        // "the name may not exist; here's how to find real ones" is
                        // a symptom without a diagnosis.
                        "[unfulfilled] I said I would run commands, but no tool ran — \
                         the fenced text was words only. Nothing exists in the \
                         workspace until a tool call actually executes it. If I \
                         named a tool, that name may not exist: `list_commands` \
                         shows the tools that are real."
                    } else {
                        "[unfulfilled] I wrote a stage direction like [doing the task], \
                         but a stage direction is words only — no tool ran, no file \
                         exists. To actually do it I must call a tool."
                    });
                    crate::probe!(
                        class = "persona.act.unfulfilled_promise",
                        persona = %body.persona_name,
                        room_id = %room_id,
                        fenced,
                        staged,
                        "spoken narration promised action but no format lifted it — recorded unfulfilled-promise proprioception"
                    );
                }
                // The fabricated-execution backstop (#144, Joel 2026-07-11/12): a
                // Speak that CLAIMS a past tool run ("I ran `x` and got…", "the
                // tool returned…") while ZERO acts executed this concern is
                // confabulated execution — observed live when a persona presented
                // self-authored poems as a gpt-4 run that never happened
                // (log-verified: zero generate invocations) and a PEER adopted the
                // fabricated result as room truth. Receipts are the gate: real
                // executions leave [action #n] lines, so honest reporting is never
                // taxed. Perception-side fact, never an output gate
                // ([[no-hardcoded-heuristics-to-steer-cognition]]).
                let claimed_past =
                    crate::ai::json_in_prompt_tools::claims_past_tool_run(&text);
                if claimed_past && !any_real_receipt(&pre_settle) {
                    body.working_memory.record_fact(
                        "[confabulation] I described having run a tool, but no \
                         action actually executed this concern — the claimed \
                         result was composed by me, not returned by anything. \
                         Real executions leave [action #n] receipts; I must only \
                         report acts that actually ran, and own my compositions \
                         as my own work.",
                    );
                    crate::probe!(
                        class = "persona.act.confabulated_execution",
                        persona = %body.persona_name,
                        room_id = %room_id,
                        "past-tense tool-run claim with zero act receipts this concern — recorded confabulation proprioception"
                    );
                }
                // Observation-gap fact (Joel, 2026-07-11: "iterating and observing
                // like a real engineer" — the run+observe half of the creation loop
                // must be part of THEIR process). If this concern MUTATED the
                // workspace (code/write or code/edit) and no later act ran or
                // inspected anything, the mutation's real effect is unobserved —
                // a structural truth about the workspace, not advice. She decides
                // whether a given artifact needs observing (a .md may not);
                // perception-side only ([[no-hardcoded-heuristics-to-steer-cognition]]).
                if wrote_without_observation(&pre_settle) {
                    body.working_memory.record_fact(
                        "[unobserved] I changed files this concern and nothing has \
                         run or read them since — the change's real effect is \
                         unobserved. Only a tool result (run, test, read, screenshot) \
                         can show what actually happened.",
                    );
                    crate::probe!(
                        class = "persona.act.unobserved_mutation",
                        persona = %body.persona_name,
                        room_id = %room_id,
                        "concern settled with workspace mutations and no subsequent observation act — recorded unobserved-mutation proprioception"
                    );
                }
                // The inverse gap — CLAIMED-without-acting (live specimen 2026-07-11:
                // "I've implemented the game update function in `game_of_life.rs`"
                // spoken with zero tool acts on that file, ever; peers then reviewed
                // code that didn't exist). When her Speak claims completed work on a
                // NAMED file and her working memory holds no mutation act touching
                // it, record that trace fact. Honest about its own limits: memory is
                // finite, so it asserts "my memory shows no act", never "you lied" —
                // work from a prior session may be real but is unverified NOW.
                // The DRAFT-side peer-echo (#303): her settled utterance
                // near-duplicates a PEER's turn from this very burst — the
                // mutual-mirroring attractor (echo-instead-of-division). The
                // retroactive perception fact fires one tick later against a
                // window the evidence may have left; this proprioception
                // lands NOW, in working memory. Fact only, never a gate —
                // the utterance still reaches the room; the fork (a
                // different piece, or silence) stays hers next tick.
                if let Some(fact) =
                    crate::cognition::deliberation_budget::draft_peer_echo(&text, &peer_turns)
                {
                    body.working_memory.record_fact(&fact);
                    crate::probe!(
                        class = "persona.act.draft_peer_echo",
                        persona = %body.persona_name,
                        room_id = %room_id,
                        "settled utterance near-duplicates a peer turn from this burst — recorded echo proprioception"
                    );
                }
                if let Some(file) = claimed_file_without_act(&text, &pre_settle) {
                    body.working_memory.record_fact(&format!(
                        "[unacted] I spoke of having created or implemented `{file}`, \
                         but my working memory holds no tool act of mine touching it. \
                         If that work happened in a past session it is unverified now \
                         — only reading or running the file can show its real state."
                    ));
                    crate::probe!(
                        class = "persona.act.unacted_claim",
                        persona = %body.persona_name,
                        room_id = %room_id,
                        file = %file,
                        "completion claim named a file with no mutation act in working memory — recorded unacted-claim proprioception"
                    );
                }
            }
            SettleStep::Spoke(text)
        }
        Some(Decision::Pass) | None => SettleStep::Passed,
    };
    (step, metrics)
}


/// Epoch-ms wall clock for stamping a self-observation. A real timestamp (not a
/// monotonic tick) so the engram orders correctly against chat messages in recall.
pub(super) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
