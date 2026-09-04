//! The ACT-QUESTION — "you hold work; do you want to work it?"
//!
//! ONE question, asked at ONE seam, by the citizen's own turn.
//!
//! ## Why this is its own file
//!
//! It used to be 273 lines nested SIX levels deep inside a match arm inside the turn
//! loop of a 4,900-line `service_loop.rs`. That depth is what hid three separate
//! defects from a full session of live bisection (2026-08-18), because a condition
//! buried that far down declines invisibly and every hypothesis looks the same from
//! outside. Depth was the bug's habitat, so the fix includes taking the habitat away.
//!
//! ## The three defects it was hiding
//!
//! 1. **`!directed`** — the question was asked only on an UNDIRECTED turn, while
//!    `benchmark/dispatch` actuates with an ADDRESSED imperative. The actuation path
//!    and the work gate were mutually exclusive by construction.
//! 2. **`InProgress`-only** — claiming a card leaves it `Claimed`; `InProgress` needs
//!    an explicit `work/state`. The gate demanded the state that starting work is what
//!    produces. Circular: she could never begin, because beginning was the precondition.
//! 3. **Nested under the speak-PASS arm** — reachable ONLY through a silence, so a
//!    citizen who ANSWERED someone and also held a claimed card was never asked whether
//!    to work it. Talking and working were alternatives. For a colleague they are not.
//!
//! ## The contract
//!
//! - It is a QUESTION, never an instruction. She may pass, exactly as before
//!   ([[no-hardcoded-heuristics-to-steer-cognition]]).
//! - It NEVER decides when to ask. The caller owns that; this module only asks.
//! - It reports on EVERY path, including every decline — `persona.work.gate` carries
//!   the decision and its inputs whether or not a work turn follows. A gate whose
//!   refusal is invisible is a gate nobody can debug.

use crate::persona::service_loop::{PersonaConversation, LIVE_MAX_ACTS};
use crate::persona::work_burst::{held_work_burst, own_recent_thoughts};
use crate::persona::supervisor::HostedPersona;

/// Ask the act-question for a citizen who may be holding work.
///
/// Called from BOTH turn outcomes — after she speaks, and after she passes — because
/// holding work is what makes the question relevant, not the speak decision.
/// Returns `true` if she held work and a held-work turn was driven this call —
/// so a caller (the self-tick) can skip a redundant musing cycle when the
/// heartbeat already advanced her claimed card. `false` when there was no cycle,
/// no citizen, or no held work.
pub(crate) async fn ask_the_act_question(
    ctx: &HostedPersona,
    conversation: &mut dyn PersonaConversation,
    lamport: u64,
    turn_room: uuid::Uuid,
    directed: bool,
) -> bool {
    // Her cognition cycle, fetched the same way the turn loop fetches it — passing it
    // in would thread a borrow through the whole turn for one optional branch.
    let Some(cycle) =
        crate::cognition::persona_workspace::global().get(&ctx.identity.peer_id.as_uuid())
    else {
        crate::probe!(
            class = "persona.work.gate",
            persona = %ctx.identity.agent_name,
            decision = "no_cycle",
            "act-question skipped: no WorkspaceCycle registered for this citizen"
        );
        return false;
    };
    // THE SECOND QUESTION (BigMama's gate-conflation diagnosis,
    // verified in-file 2026-08-08; the root under Joel's "missing
    // something"): speak and act shared ONE terminal gate, so
    // "nothing to say" — the CORRECT answer on a quiet room —
    // also silently answered "nothing to do" for a citizen
    // holding claimed work. The ledger's falsifiable signature:
    // every completion followed a direct address; zero happened
    // ambiently. Working is not speaking. A Pass settles the
    // speak-question; when she holds an in-progress claim, the
    // ACT-question is asked as its OWN turn — a separate
    // drive_to_settle whose burst IS her card, under the
    // workspace-deliverable contract. Her answer stays hers:
    // Pass here too and the turn simply ends. This adds a
    // question, never an instruction — the card is not made
    // louder and nothing nags inside the speak turn
    // ([[no-hardcoded-heuristics-to-steer-cognition]]).
    //
    // GLASS BOX (2026-08-18). This gate has FIVE conditions and used to
    // emit NOTHING when any of them declined, so "she holds a card and
    // never worked it" looked identical whether the citizen was absent,
    // the claims call failed, the states didn't match, or the set was
    // empty. One evening of live bisection produced five hypotheses that
    // the probe stream could not tell apart — because the branch was
    // silent on every path but the taken one. It now reports the DECISION
    // and every input to it, always. A gate whose refusal is invisible is
    // a gate nobody can debug ([[a-perception-fact-is-honesty]]).
    //
    // WHY `directed` NO LONGER BLOCKS. The act-question used to be asked
    // only on an UNDIRECTED turn — which made it unreachable on the one
    // path benchmarks actually use: `benchmark/dispatch` actuates with an
    // ADDRESSED imperative ("an addressed imperative in its OWN message
    // block actuates; a card sitting silently on the board does not"), so
    // every kickoff drives a DIRECTED turn and every directed turn skipped
    // the work question. The actuation path and the work gate were
    // mutually exclusive by construction. The `directed` flag was never
    // load-bearing for correctness here: this whole branch already sits
    // behind her PASS on the speak-question, so she has declined to talk
    // either way, and the act-question stays hers to pass again.
    //
    // WHY `Claimed` COUNTS AS HELD. The filter took `InProgress` only,
    // but claiming a card — `work/claim`, or dispatch's pre-claim — leaves
    // it `Claimed`; `InProgress` requires an explicit `work/state` call.
    // So the gate demanded a state that starting work is what produces:
    // she could never begin, because beginning was the precondition. Both
    // states mean "this card is in her hands", which is the only question
    // this gate is asking.
    {
        if let Some(citizen) = conversation.stream_citizen() {
            let claims_result = citizen.active_claims().await;
            let claims_err =
                claims_result.as_ref().err().map(|e| e.to_string());
            let claims = claims_result.unwrap_or_default();
            let held: Vec<&airc_lib::WorkCard> = claims
                .iter()
                .filter(|c| {
                    matches!(
                        c.state,
                        airc_work::CardState::InProgress
                            | airc_work::CardState::Claimed
                    )
                })
                .collect();
            // ONE card per work turn — her freshest live claim (the FOCUS rule,
            // `bench_round::room_for_card`): with two held cards the staging
            // resolution was ambiguous, her hands stayed at home, and every act
            // landed in her own repo copy (Lorcan, 2026-09-04). The other card
            // stays held; its turn comes when it is the freshest.
            let held: Vec<&airc_lib::WorkCard> =
                crate::persona::work_focus::focus_card(held).into_iter().collect();
            crate::probe!(
                class = "persona.work.gate",
                persona = %ctx.identity.agent_name,
                directed = directed,
                active_claims = claims.len(),
                held = held.len(),
                claims_error = claims_err.as_deref().unwrap_or(""),
                states = claims
                    .iter()
                    .map(|c| format!("{:?}", c.state))
                    .collect::<Vec<_>>()
                    .join(","),
                decision = if held.is_empty() { "no_held_work" } else { "work_turn" },
                "held-work gate evaluated after a speak-pass — this row is \
                 the ONLY place the act-question's inputs are visible"
            );
            {
                if !held.is_empty() {
                    // Stamp held-work freshness for EACH card she is about to work,
                    // so the round verdict + the standing autopilot can see Citizen
                    // progress (which leaves no detached-solve ledger entry). Without
                    // this the round reads "unstarted" while she works and the
                    // autopilot piles up duplicate rounds (A / #the-9-1-night-audit).
                    let worked_at = crate::persona::trace::now_ms();
                    for c in &held {
                        crate::cognition::bench_round::record_card_worked(
                            c.card_id.as_uuid(),
                            worked_at,
                        );
                    }
                    // Her own newest thoughts on this work lead the turn (see
                    // `held_work_burst`); paged from the durable store, the
                    // same page the catch-up reads. A failed page is a missing
                    // block, never a failed turn.
                    let last_state = match crate::persona::durable_history::room_rows(turn_room, 80).await {
                        Ok(rows) => {
                            let about: Vec<String> = held
                                .iter()
                                .flat_map(|c| {
                                    let id8: String = c.card_id.as_uuid().to_string().chars().take(8).collect();
                                    let inst = crate::commands::benchmark::parse_card_title(&c.title)
                                        .map(|(_, i)| i)
                                        .unwrap_or_default();  // unwrap_or: a non-bench title has no instance to scope on
                                    [id8, inst]
                                })
                                .collect();
                            crate::persona::work_burst::own_recent_thoughts_about(
                                &rows,
                                ctx.identity.peer_id.as_uuid(),
                                4,
                                400,
                                &about,
                            )
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "work turn: last-state page failed; opening without it");
                            Vec::new()
                        }
                    };
                    crate::probe!(
                        class = "persona.work.last_state",
                        persona = %ctx.identity.agent_name,
                        thoughts = last_state.len() as u64,
                        "her own newest thoughts lead the work turn"
                    );
                    let burst_text = held_work_burst(&held, &last_state);
                    // The producer's CONTEXT half, kept before the burst is
                    // moved into the driver — one construction, so the
                    // training example records the prompt she was actually
                    // handed rather than a re-derived approximation of it.
                    let work_context = burst_text.clone();
                    // Her held-work turn happens IN the room she was asked in —
                    // non-nil by the service loop's A.6 fallback (#425).
                    let burst = crate::cognition::workspace::Burst::raw_in(
                        crate::identity::ActivityRoom::from_uuid(turn_room)
                            .expect("turn_room is the service loop's non-nil turn room"), // service loop guarantees a non-nil turn room; witness re-checks
                        burst_text,
                    );
                    let work_framing =
                        crate::cognition::workspace::TurnFraming::self_thread(
                            false,
                        )
                        .on_workspace();
                    // HANDS FOLLOW THE CARD (#456). Her held card may be a
                    // staged benchmark checkout — a real git repo under
                    // `workspace/swe/<instance>`. Without rooting her hands
                    // there she works the card by writing into her OWN
                    // workspace, and the grader's `git diff` on the sandbox
                    // scores a false ZERO: the same defect glass-boxed on
                    // agent/solve 2026-07-22 (2 real acts, correct file
                    // written, empty patch).
                    //
                    // This is the live sibling of agent/solve's re-root, and
                    // it is what lets a citizen work a bench card IN HER OWN
                    // LOOP — which is the only path where the L2 training
                    // producer fires, so it is also what puts benchmark
                    // experience into her genome instead of only her memory.
                    //
                    // The re-root is PROCESS-GLOBAL (the file engine keys on
                    // caller identity), so the restore below is mandatory on
                    // EVERY exit — #312: after a flask solve, Anwen's live
                    // self was still reading the exam repo hours later.
                    // Non-bench cards resolve to None and nothing moves.
                    // A REVIEW card roots her hands in the OWNER's checkout (the fix
                    // under review lives there); any other held card resolves to her
                    // own staged instance as before.
                    let review_workspace = held
                        .iter()
                        .filter_map(|c| crate::commands::benchmark::parse_review_title(&c.title))
                        .find_map(|instance| {
                            let copies = crate::persona::staged_workspace::owners_of(&instance);
                            copies
                                .iter()
                                .find(|c| c.has_work)
                                .or_else(|| copies.first())
                                .map(|c| c.path.clone())
                        });
                    let card_workspace = review_workspace.or_else(|| {
                        crate::persona::staged_workspace::workspace_for_held_cards(
                            &ctx.identity.peer_id.as_uuid(),
                            held.iter().map(|c| c.title.as_str()),
                        )
                    });
                    let work_hands = match &card_workspace {
                        Some(ws) => {
                            let hands =
                                crate::cognition::persona_workspace::ActingHands::of(
                                    &cycle,
                                );
                            match crate::cognition::persona_workspace::root_acting_workspace(
                                &cycle,
                                &ws.to_string_lossy(),
                                &[],
                                false,
                            )
                            .await
                            {
                                Ok(()) => {
                                    // WHERE SHE STANDS, as a FACT in working memory (the
                                    // same channel as the act-budget facts). Live
                                    // 2026-09-04: the card said `swe/<instance>/`
                                    // (workspace-relative) while her tools stood INSIDE
                                    // that repo — every citizen burned acts on `ls swe/`
                                    // → "No such file" and reported the checkout missing.
                                    if let Some(body) = cycle.acting() {
                                        body.working_memory.record_fact(&format!(
                                            "[hands] For this turn my files and shell are \
                                             rooted AT the repo root `{}` — paths are \
                                             repo-relative; `ls` lists the repo itself \
                                             (there is no `swe/` directory from here).",
                                            ws.display()
                                        ));
                                    }
                                    crate::probe!(
                                        class = "persona.work.hands_rooted",
                                        persona = %ctx.identity.agent_name,
                                        workspace = %ws.display(),
                                        cards = held.len(),
                                        "hands rooted at her claimed card's \
                                         staged workspace for this work turn"
                                    );
                                    hands
                                }
                                Err(e) => {
                                    // Fail LOUD, work anyway in her own
                                    // workspace: a citizen who cannot reach
                                    // the repo still gets her turn, and the
                                    // empty patch is then explained on the
                                    // probe stream instead of being a mystery
                                    // zero. No silent re-root.
                                    tracing::error!(
                                        persona = %ctx.identity.agent_name,
                                        workspace = %ws.display(),
                                        error = %e,
                                        "could NOT root hands at the claimed \
                                         card's workspace — she will work in \
                                         her own dir and any graded diff will \
                                         read EMPTY"
                                    );
                                    None
                                }
                            }
                        }
                        None => None,
                    };
                    let work = crate::cognition::act_observe::drive_to_settle(
                        &cycle,
                        burst,
                        LIVE_MAX_ACTS,
                        work_framing,
                    )
                    .await;
                    // Give her back her own hands BEFORE anything else can
                    // observe them — every exit path from here (Spoke, Passed,
                    // Acted) must leave her rooted at home (#312).
                    if let Some(hands) = &work_hands {
                        if let Err(e) =
                            crate::cognition::persona_workspace::restore_acting_workspace(
                                hands,
                            )
                            .await
                        {
                            tracing::error!(
                                persona = %ctx.identity.agent_name,
                                error = %e,
                                "work turn could NOT return her hands to her \
                                 own workspace — she is still rooted at the \
                                 card's repo and her live turns will act there"
                            );
                        }
                    }
                    let (work_step, _) =
                        crate::cognition::act_observe::SettleStep::from_settled(
                            work,
                        );
                    match work_step {
                        crate::cognition::act_observe::SettleStep::Spoke(
                            text,
                        ) => {
                            // She worked and has something to report —
                            // that report earned its send.
                            crate::probe!(
                                class = "persona.turn.work",
                                persona = %ctx.identity.agent_name,
                                lamport = lamport,
                                decision = "spoke",
                                "work-turn settled with a report"
                            );
                            // Answer where she was asked — `turn_room`
                            // is the A.6 arrival room already resolved
                            // for this turn, so the report lands in the
                            // room whose work it reports on.
                            if let Err(e) =
                                conversation.say_in(turn_room, &text).await
                            {
                                tracing::warn!(
                                    error = %e,
                                    "work-turn report failed to send"
                                );
                            }
                            // L2 producer on the WORK turn (#456). This was
                            // missing, and it is the highest-value training
                            // signal the substrate produces: the reply turn
                            // below already feeds the producer, but the turn
                            // where she actually WORKS HER CLAIMED CARD did
                            // not — so every act of real work was invisible
                            // to the genome while chat was not.
                            //
                            // The (context, completion) pair here is honest:
                            // context = the card burst she was handed,
                            // completion = the report she wrote after doing
                            // the work. Same shape as the reply path, same
                            // best-effort spawn, same quality bar applied
                            // inside the producer.
                            //
                            // Still the LIVE path — an eval fork never
                            // reaches here (`drive_to_settle` is called from
                            // the fork, this call site is not), so the
                            // measurement-contamination guard the reply path
                            // relies on is unchanged.
                            crate::persona::training_producer::produce(
                                ctx.identity.peer_id.as_uuid(),
                                ctx.identity.agent_name.clone(),
                                ctx.profile.model_id.clone(),
                                work_context.clone(),
                                text.clone(),
                            );
                        }
                        crate::cognition::act_observe::SettleStep::Passed { reason } => {
                            // THE DETERMINISTIC COMPLETION EDGE. A pass now carries a
                            // REASON (her own words), and the substrate ACTS on it at
                            // the turn boundary instead of waiting on the LLM to call a
                            // `work/state` tool (unreliable) or a 3-min sweep to notice
                            // (Rube-Goldberg polling). Recipe-general: the held card may
                            // be a benchmark solve, a code task, a Slack/AWS/finance
                            // pipeline item — the edge only transitions the card; the
                            // recipe that OWNS it reacts to WORK_CARD_STATE_CHANGED
                            // (bench grade-on-done is one such reactor, next_unworked
                            // another). [[activity-outcome-score-is-recipe-owned-gates-multiply-objectives-weigh]]
                            use crate::cognition::pass_intent::{
                                classify_pass, conclude_from_pass, ConcludeAction,
                            };
                            let held_ids: Vec<uuid::Uuid> =
                                held.iter().map(|c| c.card_id.as_uuid()).collect();
                            let intent = classify_pass(reason.as_deref());
                            // The DECISION is a pure, exhaustively-tested function
                            // (`conclude_from_pass`); this arm only EXECUTES it — so the
                            // logic under test is the logic that runs (compression).
                            match conclude_from_pass(&held_ids, intent) {
                                ConcludeAction::Close(id) => {
                                    // One held card + a reasoned 'done' → conclude it
                                    // through the ONE shared card-lifecycle path (same
                                    // change+emit the `work/state` verb uses), so the
                                    // owning recipe's outcome fires NOW. Quality is the
                                    // recipe's to score — an empty deliverable grades an
                                    // honest fail, never a substrate second-guess.
                                    let card_id = airc_work::WorkCardId::from_uuid(id);
                                    match citizen
                                        .advance_card_to(card_id, airc_work::CardState::Closed)
                                        .await
                                    {
                                        Ok(()) => crate::probe!(
                                            class = "work.settle.deterministic",
                                            persona = %ctx.identity.agent_name,
                                            lamport = lamport,
                                            card_id = %id,
                                            reason = reason.as_deref().unwrap_or(""),  // unwrap_or: no reason given = empty field on the probe
                                            "held card concluded at the turn boundary on a \
                                             reasoned 'done' — the owning recipe's outcome \
                                             fires now, no sweep, no tool-call dependency"
                                        ),
                                        Err(e) => crate::probe!(
                                            class = "work.settle.error",
                                            persona = %ctx.identity.agent_name,
                                            card_id = %id,
                                            error = %e,
                                            "reasoned 'done' but the card transition failed \
                                             — the crash-backstop sweep still covers it"
                                        ),
                                    }
                                }
                                ConcludeAction::Ambiguous => crate::probe!(
                                    class = "work.settle.ambiguous",
                                    persona = %ctx.identity.agent_name,
                                    lamport = lamport,
                                    held = held.len() as u64,
                                    "reasoned 'done' with multiple held cards — cannot \
                                     tell which; leaving all held for an explicit close"
                                ),
                                ConcludeAction::Blocked => crate::probe!(
                                    class = "work.blocker",
                                    persona = %ctx.identity.agent_name,
                                    lamport = lamport,
                                    reason = reason.as_deref().unwrap_or(""),  // unwrap_or: no reason given = empty field on the probe
                                    "held-work turn passed BLOCKED — card stays hers; a \
                                     candidate substrate-gap report, not a completion"
                                ),
                                ConcludeAction::Nothing => crate::probe!(
                                    class = "work.pass.nothing",
                                    persona = %ctx.identity.agent_name,
                                    lamport = lamport,
                                    "held-work turn passed with nothing to contribute — a \
                                     substrate-gap signal ([[failures-are-substrate]])"
                                ),
                                ConcludeAction::Hold => crate::probe!(
                                    class = "persona.turn.work",
                                    persona = %ctx.identity.agent_name,
                                    lamport = lamport,
                                    decision = "passed",
                                    "work-turn passed with no legible completion — her \
                                     choice, honored; nothing concluded"
                                ),
                            }
                        }
                        other => {
                            // Acted (results already in her working
                            // memory) or an inference failure — either
                            // way the receipt says which.
                            crate::probe!(
                                class = "persona.turn.work",
                                persona = %ctx.identity.agent_name,
                                lamport = lamport,
                                decision = ?std::mem::discriminant(&other),
                                "work-turn settled without a spoken report"
                            );
                        }
                    }
                    // She held work and worked it this turn — tell the caller so
                    // a self-tick can skip the redundant musing cycle.
                    return true;
                }
            }
        }
    }
    // Reached only when she held no work (or no citizen was present) — nothing
    // was driven this call.
    false
}
