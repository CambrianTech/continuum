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
    any_real_receipt, claimed_file_without_act, collect_touched_paths, mutated_workspace,
    wrote_without_observation,
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
///
/// ## Why the lived-experience write lives HERE and not at the call sites
///
/// This function is the ONE place a `SettleOutcome` is produced, so it is the one
/// place "a turn was lived" can be recorded without the fact being re-derived per
/// caller. It was not always: the #319 producer was first wired into a SINGLE
/// service_loop call site (the directed-message path), which left the self-tick
/// path and the held-work path settling turns that no experience record ever
/// described. Three callers, one of them remembering — the missing-constraint
/// shape ([[the-same-bug-at-two-sites-is-a-missing-constraint]]), and the reason
/// zero `LivedTurn` records existed on disk while citizens were demonstrably
/// deliberating.
///
/// Recording once around the driver — rather than at each of its four return
/// paths — is deliberate for the same reason: a fifth return path added later
/// inherits the record instead of silently opting out of learning.
///
/// The write is gated on [`WorkspaceCycle::acting`] because that is where a
/// citizen's identity lives. A cycle with no `ActingBody` is pure cognition (a
/// faculty test, a replay) — it is nobody's lived experience, so there is no
/// stream it belongs in. That is a structural absence, not a skipped write.
/// Consecutive spoken plans a workspace-deliverable turn tolerates before it
/// settles without a deliverable (each is re-perceived, not ended).
pub(super) const NARRATION_BUDGET: usize = 3;

pub async fn drive_to_settle(
    cycle: &WorkspaceCycle,
    burst: impl Into<Burst>,
    max_acts: usize,
    framing: TurnFraming,
) -> SettleOutcome {
    let settled = settle_to_outcome(cycle, burst.into(), max_acts, framing).await;
    if let Some(body) = cycle.acting() {
        crate::cognition::experience::record_lived_turn(
            &crate::modules::persona_instance_manager::resolve_continuum_root(),
            crate::identity::PeerId::from_uuid(body.persona_id),
            &settled,
        );
    }
    settled
}

/// The settle loop itself. Private so that [`drive_to_settle`] is the only way to
/// reach it — every produced outcome therefore passes the lived-experience seam.
async fn settle_to_outcome(
    cycle: &WorkspaceCycle,
    burst: Burst,
    max_acts: usize,
    framing: TurnFraming,
) -> SettleOutcome {
    // The turn's room comes FROM the burst — witnessed non-nil at construction,
    // so the drive can no longer disagree with the rendered header (#425).
    let room_id: Uuid = burst.room.as_uuid();
    let mut acts = 0usize;
    // Rolling act-duration sum for the inline pace verdict below.
    let mut pace_sum_secs: f64 = 0.0;
    // This turn's causal thread: each admitted act observation becomes the
    // CausedBy target of the next act in the SAME chain — the driver owns the
    // chain, so an edge can never cross turns or rooms (CAUSAL-MEMORY-GRAPH.md).
    // ROOTED in what caused this turn, so the first act chains to its trigger rather
    // than starting mid-air — the link that makes "which acts were done for this card"
    // a graph query instead of an inference.
    let chain = super::apply::ActChain::rooted_in(&burst.cause);
    // How many turns actually run with a head on their thread — the measurement that
    // tells us whether the causal graph is CONNECTED in the live system, rather than
    // connected in the one path I happened to wire by hand
    // ([[an-absence-is-an-unfinished-measurement]]). An `ambient` row is not a fault;
    // it is an idle tick whose stimulus the projection layer discarded.
    crate::probe!(
        class = "engram.chain.rooted",
        cause = burst.cause.as_str(),
        room = %room_id,
        rooted = burst.cause.root().is_some(),
        "turn's causal thread begins here"
    );
    // The turn's investigation trail (see `SettleOutcome::touched_paths`).
    let mut touched: Vec<String> = Vec::new();
    // Fold each tick's deliberation cost in, so the settled outcome reports the
    // task's TOTAL speed/latency (a multi-act task pays for every generation).
    let mut metrics = TurnMetrics::default();

    // Signature of a tick's tool batch for loop-detection: `name|args` per call, the random
    // per-call `id` excluded, sorted so batch order doesn't matter. Two ticks with the same
    // signature emitted the byte-identical action.
    fn calls_signature(calls: &[ToolCall]) -> String {
        let mut parts: Vec<String> = calls.iter().map(|c| c.loop_fingerprint()).collect();
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
    let mut narrations_since_act: usize = 0;
    let mut acts_at_last_nudge: Option<usize> = None;
    // Which `acts` value the last budget fact fired at — a Speak's one-shot
    // re-perception re-enters the loop with `acts` unchanged, and the same
    // milestone must not stamp twice.
    let mut budget_fact_at: Option<usize> = None;

    // DISCOVERY SATURATION GATE (#390) — the STATE escalation of the [no-deliverable]
    // fact above, built on its own measured failure: the fact fires on EVERY act with
    // no mutation receipt (see the Acted arm), and a live run still spent 15+ varied
    // reads on the right file with a zero-byte diff — the fact was perceived and
    // changed nothing ([[perception-facts-fire-correctly-and-change-nothing-only-
    // state-changes-behaviour]]). Varied reads never trip the #206 identical-act
    // backstop (each has a fresh signature), so the only bound they ever hit is
    // `max_acts` — at which point the budget is gone and the empty-diff re-drive in
    // `agent/solve` structurally cannot fire. The state fix: on a turn whose
    // DELIVERABLE IS THE WORKSPACE, discovery may spend at most HALF the act budget
    // before the first workspace mutation; past that, acts are withheld (same
    // `may_act` lever as #206) so the drive ends with REAL remaining budget — which
    // is exactly what the empty-diff re-drive needs to hand back with the structural
    // fact. The first mutation lifts the gate for the rest of the turn: iteration
    // (edit → run tests → edit) is never bounded, only pre-write wandering. Not a
    // steer — it never says what to write, exactly like `max_acts`; it converts an
    // unbounded read loop into a decision point while the budget can still buy the
    // decision. Non-workspace turns (chat, research) are untouched.
    // 3/4 of the budget, not 1/2 (2026-08-24): at /2 the gate fired at act 16
    // of 32 on tasks whose honest STUDY phase needs more (126-case reverse
    // engineering) — she "quit" at half budget because the gate quit for her,
    // and the red-build re-drive's fresh turn hit its own gate identically.
    // The gate still bounds a pure-read runaway (a full-budget read turn ends
    // withheld, and the pre-gate warning now lands 4 acts before THIS bound —
    // genuinely before, not at, the cliff). Plumbing must never out-stubborn
    // the engineer it serves.
    // Divide BEFORE multiply so this can never overflow: a self-tick (non-benchmark)
    // turn passes `max_acts = usize::MAX` (the "unlimited acts" sentinel), and the old
    // `max_acts * 3` overflowed usize → a debug-build panic that aborted EVERY persona
    // service loop the instant it self-ticked, so residency could never hold
    // (resident_count flapped to 0, #412 root). `(max_acts / 4)` is always ≤ max_acts, so
    // `* 3` stays in range; `saturating_mul` is belt-and-suspenders. 3/4 of the budget,
    // overflow-safe.
    let discovery_budget = (max_acts / 4).saturating_mul(3).max(1);
    let mut mutated_yet = false;
    let mut saturation_probed = false;

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
        // ACT-BUDGET PROPRIOCEPTION (2026-08-23). She could never see her own
        // stopwatch: the budget gated acts SILENTLY, so an engineer's rational
        // analysis-first plan burned the whole default budget and was graded
        // with an empty src/ (MirrorCode baseline — the cap measured our
        // patience, not her skill; an engineer who can see "2 acts left"
        // triages, one who can't cannot). Three structural facts — the
        // contract at turn start, the midpoint, and two-remaining — state the
        // TRUE shape of the turn and nothing else: no file, no fix, no next
        // tool. Pacing stays entirely her decision.
        // [[no-hardcoded-heuristics-to-steer-cognition]]
        if framing.workspace_deliverable && budget_fact_at != Some(acts) {
            if let Some(body) = cycle.acting() {
                // The probe is the RECEIPT that the fact reached her working
                // memory — record_fact leaves no structured trace, and "the
                // stopwatch is visible to her" was unprovable from the live
                // stream the day it shipped ([[observability-as-substrate]]).
                let probe_budget_fact = |milestone: &str| {
                    crate::probe!(
                        class = "persona.act_budget.fact",
                        persona = body.persona_name.as_str(),
                        milestone = milestone,
                        acts = acts,
                        max_acts = max_acts,
                        "act-budget proprioception fact recorded in working memory"
                    );
                };
                // PRE-GATE WARNING (2026-08-23, measured twice on bib2json):
                // at a 32-act budget the midpoint fact and the discovery-
                // saturation gate BOTH land at act 16 — her first pacing
                // signal arrived in the same instant her acts were withheld,
                // so a spec-study opening could never convert in time. The
                // gate's contract becomes perceptible BEFORE it binds: a
                // structural fact at 4 acts out (workspace turns with no
                // mutation yet). States the turn's real rule; names no file,
                // no fix, no tool.
                let gate_approaching = framing.workspace_deliverable
                    && !mutated_yet
                    && discovery_budget >= 4
                    && acts == discovery_budget - 4;
                if gate_approaching {
                    budget_fact_at = Some(acts);
                    probe_budget_fact("pre_gate");
                    body.working_memory.record_fact(&format!(
                        "[act-budget] {acts} acts spent, none has changed a file yet.                          This turn's contract withholds further acts after                          {discovery_budget} total unless the workspace has been                          written to — the task is graded on files, and unwritten                          work will not exist for the grader."
                    ));
                } else if acts == 0 {
                    budget_fact_at = Some(acts);
                    probe_budget_fact("turn_start");
                    body.working_memory.record_fact(&if max_acts == usize::MAX {
                        "[act-budget] This turn has no fixed act budget: I act until I settle. \
                         The task is graded on the state of the workspace when I settle — \
                         work not yet written when I settle does not exist for the grader."
                            .to_string()
                    } else {
                        format!(
                            "[act-budget] This turn grants me {max_acts} act→observe \
                             cycles before I must settle. The task is graded on the state of \
                             the workspace when I settle — work not yet written when the \
                             budget ends does not exist for the grader."
                        )
                    });
                } else if acts == max_acts / 2 {
                    budget_fact_at = Some(acts);
                    probe_budget_fact("midpoint");
                    body.working_memory.record_fact(&format!(
                        "[act-budget] I have spent {acts} of my {max_acts} acts this turn."
                    ));
                } else if max_acts.saturating_sub(acts) == 2 {
                    budget_fact_at = Some(acts);
                    probe_budget_fact("two_remaining");
                    body.working_memory.record_fact(&format!(
                        "[act-budget] {acts} of {max_acts} acts spent — 2 remain before I \
                         must settle."
                    ));
                }
            }
        }
        // may_act gates ACTING (not speaking): past the act budget, once she is provably
        // stuck re-emitting the identical act, OR once a workspace-deliverable turn has
        // saturated its discovery budget without a single mutation (#390 gate above), a
        // fresh Act is returned un-driven and she must settle into a Speak/Pass.
        // Speaking is never gated.
        let discovery_open =
            !framing.workspace_deliverable || mutated_yet || acts < discovery_budget;
        if !discovery_open && acts < max_acts && stuck < STUCK_LIMIT && !saturation_probed {
            saturation_probed = true;
            crate::probe!(
                class = "persona.settle.discovery_saturated",
                room_id = %room_id,
                acts = acts,
                discovery_budget = discovery_budget,
                max_acts = max_acts,
                "workspace-deliverable turn spent its discovery budget with zero \
                 mutations — withholding further acts so the remaining budget reaches \
                 the empty-diff re-drive instead of being read away (#390 state gate)"
            );
        }
        let may_act = acts < max_acts && stuck < STUCK_LIMIT && discovery_open;
        let act_started = std::time::Instant::now();
        crate::probe!(
            class = "settle.tick.start",
            room = %room_id,
            acts_so_far = acts as u64,
            "settle loop iterating — next receipt is this tick's workspace run"
        );
        // PER-TICK DEADLINE (2026-08-29) — the reaper the becalmed week demanded.
        // Five distinct wedge sites in three days, each parking ONE await inside a
        // tick with no bound: memory-era lease, faculty barrier, permit convoy,
        // dream inversion, and finally a park past drive.start that mooted the
        // per-seam chase. A held solve that wedges is a SYSTEM-WIDE MUTE (its
        // measured hold defers all ambient cognition), so a tick is bounded the
        // way every RTOS task is: generously above the slowest honest tick
        // (Flash-Next deep tick ≈ 10-15 min incl. tools), fatally below forever.
        // Elapse → loud infra outcome; the drive ends; the hold RELEASES; resume
        // retries; nothing stays silently becalmed again.
        const TICK_DEADLINE: std::time::Duration = std::time::Duration::from_secs(25 * 60);
        let (step, step_metrics) = match tokio::time::timeout(
            TICK_DEADLINE,
            settle_step(cycle, burst.clone(), may_act, framing, situation, &chain),
        )
        .await
        {
            Ok(r) => r,
            Err(_) => {
                crate::probe!(
                    class = "settle.tick.deadline",
                    room = %room_id,
                    acts_so_far = acts as u64,
                    deadline_s = TICK_DEADLINE.as_secs(),
                    "tick exceeded its deadline — ending the turn LOUDLY as infra (never a capability verdict); the hold releases with the drive"
                );
                (
                    SettleStep::InferenceFailed {
                        error: format!(
                            "tick exceeded {}s deadline at act {} — an in-tick await parked                              (infra), turn ended loudly so the measured hold releases",
                            TICK_DEADLINE.as_secs(),
                            acts
                        ),
                    },
                    None,
                )
            }
        };
        // This act's model wall-time, captured before the accumulate consumes
        // the metrics — the pace row below splits act time into model vs
        // residue with it.
        let act_model_ms = step_metrics.as_ref().map(|m| m.latency_ms).unwrap_or(0);
        if let Some(m) = step_metrics {
            metrics.accumulate(m);
        }
        // INLINE PACE VERDICT (Joel 2026-08-23: "know immediately if a model is
        // being slow as molasses, looping, thrashing, not even starting" — the
        // states were entering WITHOUT visibility and a human had to pester).
        // Every act stamps its wall-clock against this turn's own rolling mean;
        // the verdict is computed WHERE THE WORK HAPPENS, event-based, so
        // slow/stalled is a probe row the moment it occurs, never a discovery.
        // No constants deciding cognition: "slow" is relative to THIS turn's
        // own pace (2x rolling mean, min 3 samples), and the row always carries
        // the raw numbers so a dashboard can re-judge.
        {
            let act_secs = act_started.elapsed().as_secs_f64();
            pace_sum_secs += act_secs;
            let pace_n = acts as f64 + 1.0;
            let mean = pace_sum_secs / pace_n;
            let slow = acts >= 3 && act_secs > mean * 2.0;
            crate::probe!(
                class = "persona.act.pace",
                room_id = %room_id,
                act = acts,
                act_secs = act_secs as u64,
                rolling_mean_secs = mean as u64,
                slow = slow,
                stuck_streak = stuck,
                // THE LEDGER SPLIT (restore-economy VDD): model_ms is this act's
                // generation wall-time (the adapter's own measurement, riding up
                // through StepMetrics); residue_ms is everything else the act
                // spent — tool execution, RAG assembly, settle bookkeeping.
                // Before this split the ~29s/act residue was only derivable by
                // subtracting log aggregates; a stall hiding in tools vs a stall
                // hiding in the model were the same number. Now each act names
                // where its time went, per-act, at the moment it happens.
                model_ms = act_model_ms,
                residue_ms = ((act_secs * 1000.0) as u64).saturating_sub(act_model_ms),
                "act pace vs this turn's own rolling mean — slow/looping visible the moment it happens"
            );
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
                // NARRATION IS NOT A SETTLEMENT (2026-09-05): a workspace-deliverable
                // turn that speaks without having changed a file is re-perceived up to
                // NARRATION_BUDGET consecutive times, not once. Measured on the team
                // round: each work turn was one act and a spoken plan, the second plan
                // ended the turn, the next turn re-oriented — 29 acts, 0 writes in 70
                // minutes on 12 held cards. The fact names the count so pacing stays
                // hers; the stuck detector still bounds the turn.
                if framing.workspace_deliverable && narrations_since_act < NARRATION_BUDGET {
                    if let Some(body) = cycle.acting() {
                        if !mutated_workspace(&body.working_memory.recent_entries()) {
                            narrations_since_act += 1;
                            acts_at_last_nudge = Some(acts);
                            // Sense, not steer (2026-09-01): the receipt-absence is
                            // the fact; the "an explanation of a fix is not the fix"
                            // sermon accumulated dozens of copies in looping minds
                            // and became the content of their turns.
                            body.working_memory.record_fact(&format!(
                                "[no-deliverable] I settled by speaking ({narrations_since_act} of \
                                 {NARRATION_BUDGET} plans in a row); no act of mine has changed a \
                                 file in this workspace yet."
                            ));
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
                    room: room_id,
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
                narrations_since_act = 0;
                collect_touched_paths(&mut touched, &calls);
                // Latch the #390 discovery gate OPEN on the first workspace mutation:
                // once she has written anything, iteration is hers for the whole
                // remaining budget. Same receipt predicate as the [no-deliverable]
                // fact — one truth for "did an act change a file".
                if framing.workspace_deliverable && !mutated_yet {
                    if let Some(body) = cycle.acting() {
                        mutated_yet = mutated_workspace(&body.working_memory.recent_entries());
                    }
                }
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
                            // Sense, not steer — same contract as the settle-path fact.
                            body.working_memory.record_fact(&format!(
                                "[no-deliverable] {acts} actions taken on this task; no act \
                                 of mine has changed a file in this workspace yet."
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
            SettleStep::WouldAct { calls, intent }
            | SettleStep::ActUnfulfilled { calls, intent } => {
                return SettleOutcome {
                    room: room_id,
                    decision: Decision::Act { calls, intent },
                    spoken: None,
                    acts,
                    world_state: burst.rendered.clone(),
                    metrics,
                    inference_error: None,
                    touched_paths: touched,
                };
            }
            SettleStep::Passed { reason } => {
                return SettleOutcome {
                    room: room_id,
                    decision: Decision::Pass { reason },
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
                    room: room_id,
                    decision: Decision::pass(),
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
/// burst is an attributed [`Burst`] carrying its activity room — the step's room
/// is read from it, never passed beside it (#425).
pub async fn settle_step(
    cycle: &WorkspaceCycle,
    burst: impl Into<Burst>,
    may_act: bool,
    framing: TurnFraming,
    situation: Situation,
    chain: &super::apply::ActChain,
) -> (SettleStep, Option<TurnMetrics>) {
    let burst: Burst = burst.into();
    // The step's room comes FROM the burst (witnessed non-nil, #425). Raw-string
    // conversion is #[cfg(test)]-only, so production can only arrive here with a
    // real attributed Burst.
    let room_id: Uuid = burst.room.as_uuid();
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
    let ws = cycle.run_situated(burst, framing, situation).await;
    // The cost of THIS tick's deliberation generation — latency + tokens of the
    // model call behind the verdict. Carried out alongside the step so the caller
    // (the eval driver, or the live heartbeat) can accumulate per-turn speed and
    // latency without re-timing the brain. `None` when no verdict carried metrics.
    let metrics = ws.metrics();
    // #266 Fold this generation's prefill accounting into the persona's lifetime KV
    // totals BEFORE any early return below — an inference FAULT still consumed real
    // prefill on the lane, and a measurement that only counts successful turns would
    // flatter exactly the failure mode we are hunting. One writer, one place.
    if let Some(m) = metrics.as_ref() {
        cycle.note_generation(m);
    }
    // A FAILED model call is not a verdict and not a silence — surface it LOUD so no
    // failure ever masquerades as a chosen `Pass` ([[fallbacks-are-illegal-fail-loud]]).
    // Checked BEFORE the decision so a fault can never collapse into `Passed` (the
    // swept-model bug: reassign changed the served model, the faculty still requested
    // the old one, the lane refused, and the refusal read as serene silence).
    if let Some(error) = ws.deliberation_fault() {
        // 2026-08-29: 18 solve attempts died on this path in six hours with ZERO
        // probe trail — the lane OOM'd every deep prefill and the harness read
        // silence. A failed generation is a loud event or it is an invisible one.
        crate::probe!(
            class = "settle.inference_failed",
            room = %room_id,
            error = &error.to_string()[..error.to_string().len().min(160)],
            "deliberation generation FAILED — surfacing before the step returns"
        );
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
                if apply_act(cycle, &calls, &intent, room_id, chain)
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
                let claimed_past = crate::ai::json_in_prompt_tools::claims_past_tool_run(&text);
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
        Some(Decision::Pass { reason }) => SettleStep::Passed { reason },
        None => SettleStep::Passed { reason: None },
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
