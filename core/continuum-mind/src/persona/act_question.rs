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
//! - With nothing held it offers her INITIATIVE turn (`initiative_turn.rs`) when the
//!   caller's cadence offers one: her hands, nothing assigned, the choice hers.
//! - It reports on EVERY path, including every decline — `persona.work.gate` carries
//!   the decision and its inputs whether or not a work turn follows. A gate whose
//!   refusal is invisible is a gate nobody can debug.

use crate::persona::service_loop::{CitizenConversation, LIVE_MAX_ACTS};
use crate::persona::supervisor::HostedPersona;

/// How many run-room rows the write-or-release gate reads to count a holder's
/// acts since her last write. 80 rows was ~15 minutes of a five-coder room
/// (⚙ receipts + 💭 thoughts interleaved), so a holder's six write-less acts
/// scrolled off the page before the gate could see them: measured 2026-09-07
/// 07:40–08:40Z, 51 acts / 4 writes across five holders and the gate fired
/// twice, both for the one holder whose acts happened to cluster. The wall is
/// a cached projection (airc#1389/#1390), so a deeper page costs nothing.
const WORK_GATE_PAGE_ROWS: usize = 400;

/// Ask the act-question for a citizen who may be holding work.
///
/// Called from BOTH turn outcomes — after she speaks, and after she passes — because
/// holding work is what makes the question relevant, not the speak decision.
/// Reports whether a held-work turn was driven and any typed capacity refusal —
/// so a caller (the self-tick) can skip a redundant musing cycle when the
/// heartbeat already advanced her claimed card. Undriven when there was no cycle,
/// no citizen, or no held work.
/// A card is in her hands when it is Claimed or InProgress — the ONE predicate the work
/// gate, and the service loop's ambient-permit bypass, both key on (a card she has
/// claimed but not yet started is held: beginning must not be the precondition for
/// beginning).
pub fn card_is_held(c: &airc_lib::WorkCard) -> bool {
    crate::persona::work_focus::actionable(c)
}

#[derive(Default)]
pub(crate) struct WorkQuestionOutcome {
    pub drove: bool,
    pub room: Option<uuid::Uuid>,
    pub capacity: Option<crate::cognition::workspace::CapacityRefusal>,
    /// The driven turn was engagement, so her beat stays engaged: always for held work
    /// (a card in her hands is progress-driven), and for an initiative turn only when she
    /// acted or spoke (a pass rests her beat).
    pub engaged: bool,
}

pub(crate) async fn ask_the_act_question(
    ctx: &HostedPersona,
    conversation: &mut dyn CitizenConversation,
    lamport: u64,
    turn_room: uuid::Uuid,
    directed: bool,
    initiative: crate::persona::initiative_turn::Initiative,
) -> WorkQuestionOutcome {
    // Her cognition cycle, fetched the same way the turn loop fetches it — passing it
    // in would thread a borrow through the whole turn for one optional branch.
    let Some(cycle) =
        crate::cognition::persona_workspace::global().get(&ctx.identity.peer_id.as_uuid())
    else {
        continuum_base::probe!(
            class = "persona.work.gate",
            persona = %ctx.identity.agent_name,
            decision = "no_cycle",
            "act-question skipped: no WorkspaceCycle registered for this citizen"
        );
        return WorkQuestionOutcome::default();
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
            let claims_err = claims_result.as_ref().err().map(|e| e.to_string());
            let claims = claims_result.unwrap_or_default();
            // ONE card per work turn — her freshest live claim (the FOCUS rule,
            // `bench_round::room_for_card`): with two held cards the staging
            // resolution was ambiguous, her hands stayed at home, and every act
            // landed in her own repo copy (Lorcan, 2026-09-04). The other card
            // stays held; its turn comes when it is the freshest.
            let held: Vec<&airc_lib::WorkCard> = crate::persona::work_focus::focus_actionable_card(claims.iter())
                .into_iter()
                .collect();
            // HER INITIATIVE (Joel, 2026-10-23: "They're supposed to be as capable as you so
            // don't constrain them"). Holding nothing used to end her moment here, so she
            // could act only once something had handed her a card. Offered when the board
            // answered (an unreadable board may hide a hold; the next beat asks again), the
            // caller's cadence offers it, and she has hands to act with.
            let hands = cycle.acting().is_some();
            let initiative_due = held.is_empty()
                && claims_err.is_none()
                && hands
                && initiative == crate::persona::initiative_turn::Initiative::Offered;
            continuum_base::probe!(
                class = "persona.work.gate",
                persona = %ctx.identity.agent_name,
                directed = directed,
                initiative = ?initiative,
                hands = hands,
                active_claims = claims.len(),
                held = held.len(),
                claims_error = claims_err.as_deref().unwrap_or(""),
                states = claims
                    .iter()
                    .map(|c| format!("{:?}", c.state))
                    .collect::<Vec<_>>()
                    .join(","),
                decision = if !held.is_empty() {
                    "work_turn"
                } else if initiative_due {
                    "initiative_turn"
                } else {
                    "no_held_work"
                },
                "held-work gate evaluated after a speak-pass — this row is \
                 the ONLY place the act-question's inputs are visible"
            );
            {
                if !held.is_empty() {
                    // Her own newest thoughts on this work lead the turn (see
                    // `held_work_burst`); paged from the durable store, the
                    // same page the catch-up reads. A failed page is a missing
                    // block, never a failed turn.
                    let page =
                        crate::persona::durable_history::room_rows(turn_room, crate::persona::durable_history::RoomPage::Newest, WORK_GATE_PAGE_ROWS)
                            .await;
                    let last_state = match &page {
                        Ok(rows) => {
                            let about: Vec<String> = held
                                .iter()
                                .flat_map(|c| {
                                    let id8: String =
                                        c.card_id.as_uuid().to_string().chars().take(8).collect();
                                    let inst =
                                        crate::recipe::bench_title::parse_card_title(&c.title)
                                            .map(|(_, i)| i)
                                            .unwrap_or_default(); // unwrap_or: a non-bench title has no instance to scope on
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
                    continuum_base::probe!(
                        class = "persona.work.last_state",
                        persona = %ctx.identity.agent_name,
                        thoughts = last_state.len() as u64,
                        "her own newest thoughts lead the work turn"
                    );
                    // No page = no note (a fresh card carries none); the absence is not a quantity.
                    let progress = match &page {
                        Ok(rows) => crate::persona::work_burst::card_progress(
                            rows,
                            ctx.identity.peer_id.as_uuid(),
                        ),
                        Err(_) => crate::persona::work_burst::CardProgress::default(),
                    };
                    let acts_without_write = page
                        .as_ref()
                        .map(|rows| {
                            crate::persona::work_burst::acts_since_last_write_since(
                                rows,
                                ctx.identity.peer_id.as_uuid(),
                                crate::persona::work_burst::last_hold_boundary_ms(
                                    ctx.identity.peer_id.as_uuid(),
                                ),
                            )
                        })
                        .unwrap_or(0); // unwrap_or: no page = no acts counted; the gate stays open, never closes on absence
                    if acts_without_write >= crate::persona::work_burst::WRITE_OR_RELEASE_AFTER_ACTS
                    {
                        continuum_base::probe!(
                            class = "persona.work.write_or_release_gate",
                            persona = %ctx.identity.agent_name,
                            acts_without_write,
                            "a write-less stretch on a held card: the advisory checkpoint is shown (never a release)"
                        );
                    }
                    // NO GOVERNOR (card 3bd860ba, Joel 2026-09-28: responsibility is durable until
                    // an explicit handoff). A write-less stretch is an OBSERVATION, never authority
                    // over whose work a card is: planning, reading and review are real work with
                    // no file change, and the act count released Kimi's multi-day project card with
                    // her edits uncommitted. The gate probe above records the stretch; a card leaves
                    // her only by her own release, an explicit reassignment, or an activity's
                    // declared policy (the benchmark round's idle reconciler is one).
                    let burst_text = crate::persona::work_burst::held_work_burst_gated(
                        &held,
                        &last_state,
                        acts_without_write,
                        &progress,
                    );
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
                    let work_framing = crate::cognition::workspace::TurnFraming::self_thread(false).on_workspace();
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
                    // `held` was reduced through work_focus::focus_card above; the same
                    // selection decides hands, facts, credit and genes.
                    // Bound through THE one binding every turn uses (card fc9e9fd7): the hands
                    // of her card's claim, standing in its checkout, never her default hands.
                    let card_turn = match held.first() {
                        Some(card) => Some(
                            crate::persona::card_hands::bind_card(
                                &cycle,
                                ctx.identity.peer_id.as_uuid(),
                                card,
                            )
                            .await,
                        ),
                        None => None,
                    };
                    let work_hands = card_turn.as_ref().and_then(|t| t.hands.as_ref());
                    let card_root = work_hands.and_then(|h| h.root());
                    let burst = burst.in_hands(card_turn.as_ref().and_then(|t| t.turn_hands()));
                    if let (Some(ws), Some(body)) = (card_root.as_ref(), cycle.acting()) {
                        // PINNED (not recorded): the FIFO window is three
                        // deep and by her third act this fact was gone —
                        // four consecutive work turns without [hands]/[env]
                        // on 2026-09-07. Replaced by key each work turn,
                        // dropped when the turn ends below.
                        body.working_memory.pin_fact(
                            "hands",
                            &format!(
                                "[hands] For this turn my files and shell are \
                                 rooted AT the repo root `{}` — paths are \
                                 repo-relative; `ls` lists the repo itself.",
                                ws.display()
                            ),
                        );
                        // Prepared environments and hidden grading belong only
                        // to benchmark recipes, never to an ordinary project.
                        if held.first().is_some_and(|card| {
                            crate::recipe::bench_title::parse_card_title(&card.title).is_some()
                                || crate::recipe::bench_title::parse_review_title(&card.title).is_some()
                        }) {
                            // THE ENVIRONMENT, as a fact. Live 2026-09-07: a
                            // holder ran `pip install --no-build-isolation -e .`
                            // twelve times in one checkout (21 acts, 0 edits) —
                            // the grader's prepared env for her instance sat
                            // beside it, unnamed. Absence is named too, so
                            // she never guesses an interpreter.
                            body.working_memory.pin_fact(
                                "env",
                                &crate::persona::instance_env_fact::instance_env_fact(&ws),
                            );
                            // THE GRADING CONTRACT, as a fact (card 2bb8ae13): the
                            // tests that grade her are not in the checkout.
                            if let Some(instance) =
                                ws.file_name().and_then(|n| n.to_str())
                            {
                                body.working_memory.pin_fact(
                                    "grading",
                                    &crate::persona::instance_env_fact::grading_fact(instance),
                                );
                            }
                        } else {
                            body.working_memory.unpin_fact("env");
                            body.working_memory.unpin_fact("grading");
                        }
                        // THE LEDGER, as the fact her turn opens with: the
                        // saved state of the thought — hers from the last
                        // turn, the previous holder's, or the owner's for a
                        // review card. Absence is named with the instruction
                        // (2026-09-12: 78 acts of re-orientation on worked cards).
                        let (reference, detail) = crate::persona::card_ledger_fact::ledger_fact_for(
                            ctx.identity.peer_id.as_uuid(), &held,
                        ).await;
                        body.working_memory.pin_expandable_fact("ledger", &reference, detail);
                    }
                    if let Some(ws) = &card_root {
                        continuum_base::probe!(
                            class = "persona.work.hands_rooted",
                            persona = %ctx.identity.agent_name,
                            workspace = %ws.display(),
                            cards = held.len(),
                            "this work turn acts in the hands of her card's claim, at its checkout"
                        );
                    } else if held.first().is_some() {
                        // Fail LOUD, work anyway with her default hands: a citizen who cannot
                        // reach the repo still gets her turn, and an empty patch is then
                        // explained on the probe stream instead of being a mystery zero.
                        tracing::error!(
                            persona = %ctx.identity.agent_name,
                            "could NOT bind the hands of her claimed card — she works in her own \
                             dir this turn and any graded diff will read EMPTY"
                        );
                    }
                    let selected_credit = card_turn.as_ref().and_then(|t| t.credit.clone());
                    // HER GENES FOLLOW HER MIND, never her card (GENE-REUSE-FORK-MINT §3d):
                    // the genes whose regions this turn's focused line is inside, weighted by
                    // depth (`genome::composer`), and every receipt names the genes that ran
                    // (`GenerationReceipt::genes`). Unreadable: the genome she has.
                    let persona_uuid = ctx.identity.peer_id.as_uuid();
                    let need = crate::cognition::recall_faculty::focused_query(&burst.rendered);
                    if let Some(phenotype) = crate::genome::composer::compose(persona_uuid, need).await {
                continuum_base::probe!(
                    class = "persona.genome.turn_genes",
                    persona = %ctx.identity.agent_name,
                    genes = phenotype.genes.len() as u64,
                    phenotype = %crate::genome::composer::render(&phenotype),
                    "the phenotype this turn runs with: each gene and its scale"
                );
                cycle.page_in(phenotype.genes);
            }
                    // Her credit follows the card she holds through the turn (card 12ddd78f).
                    let mut credit_capture = crate::persona::training_producer::ClaimFollowingCapture::new(
                        selected_credit.as_ref(),
                        conversation.stream_citizen(),
                        crate::persona::training_producer::TurnCreditCapture::opener(
                            ctx.identity.peer_id.as_uuid(),
                            &ctx.identity.agent_name,
                            &work_context,
                        ),
                    );
                    let work = crate::cognition::act_observe::drive_to_settle_with_credit(
                        &cycle,
                        burst,
                        LIVE_MAX_ACTS,
                        work_framing,
                        conversation,
                        Some(&mut credit_capture as &mut dyn crate::cognition::act_observe::TurnCredit),
                    )
                    .await;
                    // The turn's work is synced to its card's branch; her hands stay where
                    // they are, because nothing moved them (card fc9e9fd7).
                    if let Some(hands) = work_hands {
                        crate::persona::card_hands::end_card_turn(hands, work.acts).await;
                    }
                    if let Some(body) = cycle.acting() {
                        // One turn ended: 1-turn pins ([hands], [env]) leave now; the
                        // governor's 2-turn [released] notice survives to the next work
                        // turn's start — the pull decision — and leaves at its end.
                        body.working_memory.end_of_work_turn();
                    }
                    let turn_act_count = work.acts;
                    let (work_step, _, _) =
                        crate::cognition::act_observe::SettleStep::from_settled(work);
                    continuum_base::probe!(
                        class = "training.hook.work_turn",
                        persona = %ctx.identity.agent_name,
                        acts = turn_act_count as u64,
                        held = held.len() as u64,
                        "held-work turn settled after its shared credit capture"
                    );
                    let capacity = match &work_step {
                        crate::cognition::act_observe::SettleStep::CapacityRefused {
                            error,
                            ..
                        } => Some(error.clone()),
                        _ => None,
                    };
                    match work_step {
                        crate::cognition::act_observe::SettleStep::Spoke(text) => {
                            // She worked and has something to report —
                            // that report earned its send.
                            continuum_base::probe!(
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
                            if let Err(e) = crate::persona::service_loop::publish_own_speech(
                                conversation,
                                ctx.identity.peer_id,
                                turn_room,
                                &text,
                            )
                            .await
                            {
                                tracing::warn!(
                                    error = %e,
                                    "work-turn report failed to send"
                                );
                            }
                            // The shared driver already staged this report with the act
                            // chain under the same turn ID and exact generation receipts.
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
                                        Ok(()) => continuum_base::probe!(
                                            class = "work.settle.deterministic",
                                            persona = %ctx.identity.agent_name,
                                            lamport = lamport,
                                            card_id = %id,
                                            reason = reason.as_deref().unwrap_or(""),  // unwrap_or: no reason given = empty field on the probe
                                            "held card concluded at the turn boundary on a \
                                             reasoned 'done' — the owning recipe's outcome \
                                             fires now, no sweep, no tool-call dependency"
                                        ),
                                        Err(e) => continuum_base::probe!(
                                            class = "work.settle.error",
                                            persona = %ctx.identity.agent_name,
                                            card_id = %id,
                                            error = %e,
                                            "reasoned 'done' but the card transition failed \
                                             — the crash-backstop sweep still covers it"
                                        ),
                                    }
                                }
                                ConcludeAction::Ambiguous => continuum_base::probe!(
                                    class = "work.settle.ambiguous",
                                    persona = %ctx.identity.agent_name,
                                    lamport = lamport,
                                    held = held.len() as u64,
                                    "reasoned 'done' with multiple held cards — cannot \
                                     tell which; leaving all held for an explicit close"
                                ),
                                ConcludeAction::Blocked => continuum_base::probe!(
                                    class = "work.blocker",
                                    persona = %ctx.identity.agent_name,
                                    lamport = lamport,
                                    reason = reason.as_deref().unwrap_or(""),  // unwrap_or: no reason given = empty field on the probe
                                    "held-work turn passed BLOCKED — card stays hers; a \
                                     candidate substrate-gap report, not a completion"
                                ),
                                ConcludeAction::Nothing => continuum_base::probe!(
                                    class = "work.pass.nothing",
                                    persona = %ctx.identity.agent_name,
                                    lamport = lamport,
                                    "held-work turn passed with nothing to contribute — a \
                                     substrate-gap signal ([[failures-are-substrate]])"
                                ),
                                ConcludeAction::Hold => continuum_base::probe!(
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
                            continuum_base::probe!(
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
                    return WorkQuestionOutcome {
                        drove: true,
                        room: Some(turn_room),
                        capacity,
                        engaged: true,
                    };
                }
            }
            if initiative_due {
                // Boxed: the settle drive is the largest future here, and nesting it by
                // value inside this one is the layout-depth overflow CI has hit before.
                return Box::pin(crate::persona::initiative_turn::drive_initiative_turn(
                    ctx,
                    &cycle,
                    conversation,
                    lamport,
                    turn_room,
                ))
                .await;
            }
        }
    }
    // Reached only when she held no work and no initiative was offered (or no citizen
    // was present) — nothing was driven this call.
    WorkQuestionOutcome::default()
}

