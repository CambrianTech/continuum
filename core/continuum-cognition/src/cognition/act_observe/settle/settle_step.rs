use super::*;

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
    chain: &super::super::apply::ActChain,
) -> (
    SettleStep,
    Option<TurnMetrics>,
    Vec<crate::ai::provenance::GenerationReceipt>,
) {
    let burst: Burst = burst.into();
    // The step's room comes FROM the burst (witnessed non-nil, #425). Raw-string
    // conversion is #[cfg(test)]-only, so production can only arrive here with a
    // real attributed Burst.
    let room_id: Uuid = burst.room.as_uuid();
    // The turn's hands, bound at its admission: every act of this step runs in them.
    let hands = burst.hands;
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
    // THE SECOND LEDGER SPLIT. `persona.act.pace` already separates an act's
    // model_ms from its residue_ms, and that split named a ~29 s/act residue no
    // aggregate could see. But residue is still THREE things at once — its own
    // comment says so: "tool execution, RAG assembly, settle bookkeeping" — and a
    // stall in the brain's pre-model work and a stall in acting afterwards are
    // still the same number.
    //
    // `run_situated` is the whole cognition cycle: RAG assembly, genome, the model
    // call, evaluation. Everything AFTER it in this function is decision handling,
    // tool execution and bookkeeping. Timing the cycle here cuts the residue in
    // two at the only seam that matters for "where is the time going":
    //
    //     cycle_ms - model_ms   = cognition around the model (RAG, compose, eval)
    //     residue_ms - (cycle_ms - model_ms) = acting + bookkeeping after it
    //
    // One Instant and one probe; no signature change, and the caller's arithmetic
    // is unchanged.
    let cycle_started = std::time::Instant::now();
    let mut ws = cycle.run_situated(burst, framing, situation).await;
    let cycle_ms = cycle_started.elapsed().as_millis() as u64;
    // The cost of THIS tick's deliberation generation — latency + tokens of the
    // model call behind the verdict. Carried out alongside the step so the caller
    // (the eval driver, or the live heartbeat) can accumulate per-turn speed and
    // latency without re-timing the brain. `None` when no verdict carried metrics.
    let metrics = ws.metrics();
    // Beside the metrics, from the SAME Workspace and for the same reason: the
    // caller cannot re-derive these after `ws` is dropped. A turn is N
    // deliberations; each tick contributes its dispatched generations, faults
    // included, and the loop folds them together.
    let step_receipts: Vec<crate::ai::provenance::GenerationReceipt> =
        ws.generation_receipts().cloned().collect();
    continuum_base::probe!(
        class = "cognition.cycle.cost",
        room = %room_id,
        cycle_ms,
        model_ms = metrics.as_ref().map(|m| m.latency_ms).unwrap_or(0), // unwrap_or: no metrics = no model call THIS cycle, so 0ms of model time is the true reading, not a guess — and `had_metrics` below carries the distinction so a reader never mistakes it for a fast call
        // cycle MINUS model: RAG assembly, genome activation, compose, evaluate —
        // everything the brain does around the generation. Subtract this from the
        // act's residue_ms and what remains is tool execution plus bookkeeping.
        around_model_ms = cycle_ms
            .saturating_sub(metrics.as_ref().map(|m| m.latency_ms).unwrap_or(0)), // unwrap_or: same 0 as above; with no model call the WHOLE cycle is around-model, which is exactly what this field should then report
        had_metrics = metrics.is_some(),
        "cognition cycle cost, split from the model call it contains — the second half of the act's residue ledger"
    );
    // #266 Fold this generation's prefill accounting into the persona's lifetime KV
    // totals BEFORE any early return below — an inference FAULT still consumed real
    // prefill on the lane, and a measurement that only counts successful turns would
    // flatter exactly the failure mode we are hunting. One writer, one place.
    if let Some(m) = metrics.as_ref() {
        cycle.note_generation(m);
    }
    if let Err(error) = cycle.persist_render_receipts(std::mem::take(&mut ws.render_receipts)).await {
        continuum_base::probe!(class = "settle.receipt_persistence_failed", room = %room_id,
            error = %error, "served generation retained; checkpoint failed before speech or action");
        return (SettleStep::ReceiptPersistenceFailed { error: Arc::new(error) }, metrics, step_receipts);
    }
    // A FAILED model call is not a verdict and not a silence — surface it LOUD so no
    // failure ever masquerades as a chosen `Pass` ([[fallbacks-are-illegal-fail-loud]]).
    // Checked BEFORE the decision so a fault can never collapse into `Passed` (the
    // swept-model bug: reassign changed the served model, the faculty still requested
    // the old one, the lane refused, and the refusal read as serene silence).
    if let Some(error) = ws.prompt_capacity_refusal() {
        let replay_input = step_receipts.is_empty() && metrics.is_none();
        return (
            SettleStep::CapacityRefused {
                error,
                replay_input,
            },
            metrics,
            step_receipts,
        );
    }
    if let Some(error) = ws.deliberation_fault() {
        // 2026-08-29: 18 solve attempts died on this path in six hours with ZERO
        // probe trail — the lane OOM'd every deep prefill and the harness read
        // silence. A failed generation is a loud event or it is an invisible one.
        continuum_base::probe!(
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
            // Carried on the FAILURE path too, and deliberately: this is the case
            // the receipts exist for. A turn whose generation faulted still
            // dispatched a generation, and dropping the receipt here would make a
            // failed call indistinguishable from one that never happened.
            step_receipts,
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
                if apply_act(cycle, &calls, &intent, ws.activity_room_id, chain, hands, ws.decision_generation())
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
            let mut echoed_a_peer = false;
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
                // Whole: what of it her trail shows is the trail's levels in her view's one
                // allocation, never a head cut at a window share here (FLOW §2d).
                body.working_memory.record_settlement(&text, Some(ws.activity_room_id));
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
                    continuum_base::probe!(
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
                    continuum_base::probe!(
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
                    continuum_base::probe!(
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
                    continuum_base::probe!(
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
                    continuum_base::probe!(
                        class = "persona.act.draft_peer_echo",
                        persona = %body.persona_name,
                        room_id = %room_id,
                        "settled utterance near-duplicates a peer turn from this burst — recorded echo proprioception, NOT posted"
                    );
                    // The copy does not leave her mouth. Until 2026-09-17 this arm only
                    // RECORDED the fact and the line still posted (SettleStep::Spoke):
                    // 0d5c1ffa re-spoke cf6b4df6's whole "I'm not sure what you're
                    // trying to do here…" verbatim minutes apart in #academy, and Cormac
                    // counted 64 of these "refusals" in an hour on the Intel tier — none
                    // of them refused anything. A near-identical copy of a peer's line is
                    // the contagion carrier ([[a-mindless-seat-is-accounted-for]]); it is
                    // a gate pass like a framing echo, counted by the mindless receipt,
                    // and her memory keeps the fact so the next turn is not the same one.
                    echoed_a_peer = true;
                }
                if let Some(file) = claimed_file_without_act(&text, &pre_settle) {
                    body.working_memory.record_fact(&format!(
                        "[unacted] I spoke of having created or implemented `{file}`, \
                         but my working memory holds no tool act of mine touching it. \
                         If that work happened in a past session it is unverified now \
                         — only reading or running the file can show its real state."
                    ));
                    continuum_base::probe!(
                        class = "persona.act.unacted_claim",
                        persona = %body.persona_name,
                        room_id = %room_id,
                        file = %file,
                        "completion claim named a file with no mutation act in working memory — recorded unacted-claim proprioception"
                    );
                }
            }
            if echoed_a_peer {
                SettleStep::Passed {
                    reason: Some(format!(
                        "{}peer_echo:near_identical: the draft near-duplicated a peer's line in this burst",
                        crate::cognition::workspace::GATE_REFUSAL_PREFIX
                    )),
                }
            } else {
                SettleStep::Spoke(text)
            }
        }
        Some(Decision::Pass { reason }) => SettleStep::Passed { reason },
        None => SettleStep::Passed { reason: None },
    };
    (step, metrics, step_receipts)
}
