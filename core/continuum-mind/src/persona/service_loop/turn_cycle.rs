use super::*;

/// What a turn's cognition cycle produced for the loop.
pub(super) enum CycleVerdict {
    /// She spoke: the reply to publish, with the L2 producer's provenance inputs.
    Spoke(SpokenTurn),
    /// No reply to publish this turn (deferred, acted, silent, failed, or yielded).
    Skip,
    /// Custody failed after generation. The loop must leave its durable
    /// obligation open and return to lifecycle supervision, not close a skip.
    CheckpointFailed(std::sync::Arc<crate::cognition::persona_workspace::TaskSinkError>),
}

/// The L2 producer's two provenance inputs are BORN inside the cycle — credit on her hands
/// at the focus site, receipts on the settled outcome — and CONSUMED after the reply is
/// published. Card-linked work is staged during the shared drive and again at settlement,
/// including turns that finish without speech; these receipts remain available for the
/// separate, genuinely unlinked speech producer.
pub(super) struct SpokenTurn {
    pub(super) response_text: String,
    pub(super) turn_credit: Option<crate::persona::training_producer::CapturedCredit>,
    pub(super) turn_generation_receipts: Vec<crate::ai::provenance::GenerationReceipt>,
}

/// What the cycle needs beyond the trigger: the burst it reasons over and the engram of
/// the message that woke the turn.
pub(super) struct CycleInput {
    pub(super) workspace_burst: crate::cognition::workspace::Burst,
    pub(super) wake_engram: Option<Uuid>,
}

/// Run the WorkspaceCycle over the burst (the brain decides; the loop is the wire driver)
/// and report the verdict. Every non-speaking end of the turn is `Skip`.
pub(super) async fn run_turn_cycle(
    ctx: &HostedPersona,
    conversation: &mut dyn CitizenConversation,
    outcome: &mut ServeOutcome,
    deferred_input: &mut crate::persona::wake_backlog::PendingInput,
    phase_timings: &mut PhaseTimings,
    trigger: TurnTrigger<'_>,
    input: CycleInput,
) -> CycleVerdict {
    let TurnTrigger {
        msg,
        turn_room,
        admitted_retry,
    } = trigger;
    let CycleInput {
        workspace_burst,
        wake_engram,
    } = input;
    let respond_started = std::time::Instant::now();
    // A turn in her mind room runs on her MIND cycle (PRIVACY-OF-THOUGHT.md §4).
    match crate::cognition::persona_workspace::global()
        .cycle_for_room(&ctx.identity.peer_id.as_uuid(), turn_room)
    {
        Some(cycle) => {
            // HER GENES ARE IN HER HEAD ON EVERY TURN, chosen by what her mind is on
            // (GENE-REUSE-FORK-MINT §3d): every gene whose region the turn's focused line is
            // inside, weighted by depth (`genome::composer`). They stay on her cycle until
            // the next turn selects, so nothing between turns flips her slot's adapters (each
            // flip clears her KV). Unreadable: the genome she has. The receipt names the
            // genes that ran (GenerationReceipt::genes).
            let need = crate::cognition::recall_faculty::focused_query(&workspace_burst.rendered);
            if let Some(phenotype) = crate::genome::composer::compose(ctx.identity.peer_id.as_uuid(), need).await {
                continuum_base::probe!(
                    class = "persona.genome.turn_genes",
                    persona = %ctx.identity.agent_name,
                    genes = phenotype.genes.len() as u64,
                    phenotype = %crate::genome::composer::render(&phenotype),
                    "the phenotype this turn runs with: each gene and its scale"
                );
                cycle.page_in(phenotype.genes);
            }
            // Run the mind over the metadata-rich burst built above
            // (WHO/WHEN/WHAT per inbox item, own posts attributed, room as the
            // WHERE). Recall (the persona's own memories) is injected by the
            // RecallFaculty inside the cycle — so "given this RAG, do I have
            // what I need?" is: this burst + recalled memory.
            // Scope the cognition tick to the room this turn is FOR (the
            // contextId), so the deliberation faculty stamps tool calls with
            // it and the persona's hands act in the real room, not a phantom
            // nil one. The serviced room is `turn_room` — the room the
            // trigger message ARRIVED in (A.6 shipped: IncomingMessage now
            // carries the transport's room; `identity.default_room` is only
            // the fallback for room-less scripted sources). Same room the
            // burst header above declares.
            // See IDENTITY-SCOPE-PEER-LIVENESS-MODEL.md A.6 step 3.
            // ONE settlement step through the SHARED primitive the eval driver
            // also uses (`act_observe::settle_step`). The live path takes exactly
            // ONE step per metronome tick — `may_act = true` (it always permits
            // its one act), and on `Act` it STOPS (no synchronous loop, no act
            // counter): the metronome re-perceives next tick with the result in
            // memory, and she settles into a Speak then (or acts again — acting-
            // forever is a fitness gap to train, never a substrate cap, §4). The
            // eval driver wraps this SAME step in a grader-paced loop; live and
            // eval thus make a turn identically (ACTING-ORGANISM.md §3.3).
            // Directedness (closes TODO #9 — the live ghost-a-direct-question gap).
            // Per Joel 2026-06-29 ("shouldn't need to be directly addressed — it's
            // a chat system"): ambient participation is now the DEFAULT posture,
            // carried by the rebalanced [Conversational Presence] framing, NOT by
            // priority. Literal addressing is reserved for its one job — withholding
            // the silent-PASS hatch when a message actually NAMES her, so she cannot
            // ghost a question put to her. Glass-box proved the gap: a cleanly-woken
            // MSG-turn PASSed a direct question while eval turns SPOKE 36/38 (the
            // model is capable; the live framing was the lever). We DERIVE it from
            // the trigger text via the SAME word-boundary, identity-aware `mentions`
            // primitive the self-tick uses (line ~1000) — a structural addressing
            // FACT fed to the mind, never a filter reading her output
            // ([[no-hardcoded-heuristics-to-steer-cognition]]). The wider
            // per-channel focus/priority that will ALSO modulate this (a learned or
            // self-set attention weight — never a hard mute except self-chosen or
            // flooding) is substrate-blocked on the airc per-(persona,room) state
            // store (#89); this is the addressing half, unblocked today.
            // Human opportunity retains priority, but sender kind can be stale
            // or shared across clients: it cannot assert that she was named.
            let attention = admitted_retry
                .then(|| deferred_input.attention_for(msg))
                .flatten()
                .unwrap_or_else(|| new_message_attention(ctx, msg)); // A new message has no retained attention; derive it from its actual sender and mentions.
            if attention.requires_priority() {
                // Focus = she is TAKING the turn on it; the heard receipt
                // already fired at the drain (`wake_backlog::publish_heard`).
                crate::cognition::vitals_pulse::record_focus(ctx.identity.peer_id.as_uuid());
            }
            let framing = crate::cognition::workspace::TurnFraming::message(attention);

            // ── Ambient-yield under lane saturation (#171 / #139) ───────────────
            // The fan-out killer: a room burst wakes N peers, and each spends a full
            // ~54s deliberation on the shared 2 lanes just to decide "not for me" —
            // so the ONE directed question everyone's waiting on sits behind minutes
            // of ambient chatter (glass-boxed: first-token up to 28 min). Ambient
            // participation is the DEFAULT and stays that way — but it is the
            // LOWEST-priority room work. So when every shared decode slot is busy,
            // an ambient turn YIELDS: she participates ambiently when there's
            // capacity (a later beat, lanes free — a self-tick re-perceives the room
            // incl. this line), never at the cost of the addressed question. A
            // priority turn (named or human opportunity) keeps its lane.
            //
            // This is resource PRIORITY, the same admission doctrine as the idle
            // self-tick, NOT a `will_respond` gate: the substrate never decides her
            // output; it only declines to spend a saturated lane on unaddressed
            // chatter. `high_water` is already advanced above, so a yielded message
            // can't re-trigger. [[idle-is-self-directed-free-time]]
            // [[never-thrash-sticky-hysteresis-on-every-lane]]
            // [[conversational-latency-is-a-misdirection-budget]]
            // #171: an ambient (non-directed) turn must claim one of a small number
            // of ambient slots or YIELD — a PERMIT, not a gauge. The gauge-based
            // first cut didn't fire: a simultaneous room-burst wakes N peers who all
            // read inflight=0 (none had generated yet) and stampede. The permit is
            // held across the whole turn, so ambient concurrency is bounded no matter
            // when everyone woke. Priority turns bypass entirely, including a human
            // speaking without a mention. Ambient turns defer to a later beat with free
            // capacity (a self-tick re-perceives the room); high_water is pre-advanced
            // so it can't re-trigger, and the durable transcript loses nothing.
            // A CARD-HOLDER IS NOT A STAMPEDE RESPONSE (2026-09-20, the M5's first
            // bounded hour: 4 minds on 2 lanes, ambient budget 1, 11 lane grants,
            // writes 0). The #171 permit bounds the fan-out of minds deciding "not for
            // me"; a mind holding a card is not deciding that — she is the mind the
            // lane exists for. She goes straight to the lane gate, where the Work class
            // (`LanePriority::Work`) still bounds her and ambient turns yield to her.
            // Read off the board she already keeps; an unreadable board = no proof of
            // held work = the ambient rule, as before.
            let holds_work = match conversation.stream_citizen() {
                Some(citizen) => citizen
                    .active_claims()
                    .await
                    .map(|cards| cards.iter().any(crate::persona::act_question::card_is_held))
                    .unwrap_or(false), // unwrap_or: an unreadable board proves nothing — the ambient rule stands
                None => false,
            };
            if holds_work && !attention.requires_priority() {
                continuum_base::probe!(
                    class = "persona.turn.work_holder_bypasses_ambient_permit",
                    persona = %ctx.identity.agent_name,
                    "a mind holding a card goes straight to the lane gate — the ambient anti-stampede permit is for minds deciding whether a room line is theirs"
                );
            }
            let _ambient_permit = if attention.requires_priority() || holds_work {
                None
            } else {
                match crate::inference::resource_admission::try_hold_ambient_turn() {
                    Some(permit) => Some(permit),
                    None => {
                        continuum_base::probe!(
                            class = "persona.turn.ambient_yielded",
                            persona = %ctx.identity.agent_name,
                            lamport = msg.lamport,
                            "ambient turn yielded — ambient slots busy; the addressed \
                             question is served first (#171)"
                        );
                        outcome.turns_skipped += 1;
                        return CycleVerdict::Skip;
                    }
                }
            };
            // Directed vs ambient part ways HERE — the fix for the live/eval
            // convergence divergence the ACTING-ORGANISM comments flagged (eval
            // SPOKE 36/38; the live path single-stepped a DIRECT question, `Acted`
            // once, `continue`d — and the burst-fingerprint dedup then suppressed
            // re-perception, so she went idle with the question unanswered).
            //
            // DIRECTED (she was actually named/asked) → `drive_to_settle`, the SAME
            // primitive the eval path validates (eval.rs run_pass): act→observe→act
            // until she SPEAKS/PASSES, so a direct question converges to an answer
            // WITHIN the turn instead of leaking onto the slow ambient tick loop.
            // `from_settled` projects the driven outcome back onto the one existing
            // turn handler below, so there is no parallel match. There is NO act
            // budget on live turns (`LIVE_MAX_ACTS = usize::MAX`) — she settles when
            // SHE settles; an "acts-forever" persona is a fitness gap to TRAIN,
            // never a substrate ceiling (ACTING-ORGANISM §4, Joel 2026-07-11).
            //
            // AMBIENT turns drive to settlement too (2026-07-11, Joel: "gating
            // acts is not autonomy"). The first cut kept ambient turns to a calm
            // one-step motion on the theory that she'd continue next tick — but
            // the live glass-box disproved it: Casper's first genuine room act
            // (read_file on his claimed wordstats card) executed cleanly, the
            // result entered memory, and the chain DIED on the next tick when
            // three chatting peers recaptured the workspace. One-act-then-yield
            // is a substrate hand-brake on a mind that already CHOSE to act.
            // drive_to_settle only loops while she KEEPS choosing Act — she can
            // Speak or Pass at any step, so driving never forces engagement with
            // noise; it just stops interrupting her own momentum. The budget is
            // the same heartbeat safety valve as the directed path (see the
            // directed comment above), and an over-budget turn degrades to the
            // metronome tail exactly as before.
            // #169 STREAMING: hand this turn a token sink so the deliberation
            // faculty forwards each decoded chunk here AS it generates (instead
            // of only the accumulated final text). A background task drains it.
            // Slice 1 proves the rail by stamping time-to-first-token on the probe
            // stream — the perceived-latency floor for video/voice/avatar; slice 2
            // forwards Token chunks to the room/TTS/avatar as `persona.turn.delta`.
            // Cleared right after the turn, so a non-streamed path is byte-identical.
            let (tok_tx, tok_rx) =
                tokio::sync::mpsc::unbounded_channel::<crate::ai::adapter::GenerationChunk>();
            cycle.set_token_sink(Some(tok_tx));
            // #169/#170: drain + publish this turn's streamed answer, coalesced.
            let forwarder = spawn_token_forwarder(
                tok_rx,
                conversation.stream_citizen(),
                ctx.identity.agent_name.clone(),
                // #170: a room turn — tee tokens to the browser rail so she types live.
                Some(turn_room.to_string()),
                Some(ctx.identity.peer_id.to_string()),
            );
            let turn_act_count;
            let (step, turn_metrics, settled_receipts, turn_credit) = {
                // A citizen holding a card works at that repo, in a ROOM turn too: the turn
                // is bound to her card claim's hands (card fc9e9fd7), its credit captured on
                // her act, and its work synced to the card's branch, by the one owner her
                // own-time turn uses as well (card 959f3d5c).
                let (outcome, credit) = crate::persona::card_hands::drive_in_held_card(
                    &cycle,
                    ctx.identity.peer_id.as_uuid(),
                    &ctx.identity.agent_name,
                    &msg.render_content(),
                    workspace_burst,
                    framing,
                    LIVE_MAX_ACTS,
                    conversation,
                )
                .await;
                // The lived-turn experience write (#319) is NOT here: it lives inside
                // `drive_to_settle`, which is the one place a `SettleOutcome` is born.
                // It WAS here, at this single call site, and that was the bug — the
                // self-tick and held-work paths settle turns through the same driver
                // and got no record, so nothing on disk ever described a lived turn.
                // The driver stays a driver: no learning policy at any call site.
                turn_act_count = outcome.acts;
                let (step, metrics, receipts) = crate::cognition::act_observe::SettleStep::from_settled(outcome);
                (step, metrics, receipts, credit)
            };
            // The credit is the turn's held CARD, captured with it, NOT read back out of
            // `acting_card_of`, a mutable persona-global that a later focus rebind would
            // re-attribute to the wrong card (card 0d51573a); and not off her hands, which a
            // failed rooting leaves None while the card stays known (Astra/S6 on 799b8fe9).
            let turn_generation_receipts = settled_receipts;
            continuum_base::probe!(
                class = "training.hook.directed_turn",
                persona = %ctx.identity.agent_name,
                acts = turn_act_count as u64,
                card_linked = turn_credit.as_ref().map(|c| c.is_card_linked()).unwrap_or(false), // JUSTIFIED unwrap_or: no credit = not card-linked, a legible false
                "directed turn settled — what the learning hook sees"
            );
            // Turn done: drop the cycle's sink so the forwarder's channel closes,
            // then join it (all `tok_tx` clones are gone once the turn's Workspaces
            // dropped inside `drive_to_settle`).
            cycle.set_token_sink(None);
            let _ = forwarder.await;
            phase_timings.respond_ms = respond_started.elapsed().as_millis() as u64;
            // Live speed/latency on the probe stream — the model's own measured
            // generation cost for THIS turn (decode tok/s + latency), the same
            // numbers the eval gym accumulates per task. Observability only; the
            // turn proceeds identically whether or not anyone is watching.
            if let Some(m) = turn_metrics {
                continuum_base::probe!(
                    class = "persona.turn.metrics",
                    persona = %ctx.identity.agent_name,
                    lamport = msg.lamport,
                    input_tokens = m.input_tokens,
                    output_tokens = m.output_tokens,
                    latency_ms = m.latency_ms,
                    tokens_per_second = m.tokens_per_second(),
                    "deliberation generation cost"
                );
            }
            deferred_input.observe_settlement(msg, wake_engram, attention, &step, &turn_generation_receipts);
            match step {
                crate::cognition::act_observe::SettleStep::CapacityRefused { .. }
                | crate::cognition::act_observe::SettleStep::DeferredBeforeDispatch => {
                    return CycleVerdict::Skip;
                }
                crate::cognition::act_observe::SettleStep::Spoke(text) => {
                    CycleVerdict::Spoke(SpokenTurn {
                    response_text: text,
                    turn_credit,
                    turn_generation_receipts,
                    })
                }
                crate::cognition::act_observe::SettleStep::Acted { calls, intent } => {
                    continuum_base::probe!(
                        class = "persona.turn.acted",
                        persona = %ctx.identity.agent_name,
                        lamport = msg.lamport,
                        calls = calls.len(),
                        intent = %intent,
                        "acted; result admitted as memory, re-perceives next tick"
                    );
                    outcome.turns_acted += 1;
                    return CycleVerdict::Skip;
                }
                crate::cognition::act_observe::SettleStep::ActUnfulfilled { calls, intent } => {
                    // No hands or the executor errored. Abstain — never a
                    // fabricated result, never a raw call envelope to the room.
                    // Every turn ends on a probe (Kimi, 5090, 2026-10-04: a turn on an
                    // addressed line ended with no outcome on the probe stream, so a
                    // chosen silence and a failed act read the same).
                    continuum_base::probe!(
                        class = "persona.turn.act_unfulfilled",
                        persona = %ctx.identity.agent_name,
                        lamport = msg.lamport,
                        calls = calls.len(),
                        intent = %intent,
                        "persona chose to act but the act could not be carried out — skipping turn"
                    );
                    outcome.turns_skipped += 1;
                    return CycleVerdict::Skip;
                }
                crate::cognition::act_observe::SettleStep::WouldAct { .. } => {
                    // Unreachable on the live path: `may_act = true` always, so the
                    // act is executed (→ Acted/ActUnfulfilled), never deferred.
                    unreachable!("live settle_step always permits its one act");
                }
                crate::cognition::act_observe::SettleStep::Passed {
                    reason: pass_reason,
                } => {
                    tracing::info!(
                        lamport = msg.lamport,
                        "persona chose silence (workspace) — substrate honors decision"
                    );
                    // SAY WHY (Cormac, 2026-09-28: every turn his 8 citizens finished
                    // ended "workspace-pass", 0 spoke, and the probe could not tell her
                    // CHOICE of silence from a GATE that converted her draft into a pass,
                    // or from no decision at all). The reason is the brain's; this line
                    // only reports it.
                    let (gated, pass_detail) = match pass_reason.as_deref() {
                        Some(r) => (
                            r.starts_with(crate::cognition::workspace::GATE_REFUSAL_PREFIX),
                            r,
                        ),
                        None => (false, "no-decision"),
                    };
                    continuum_base::probe!(
                        class = "persona.turn.silent",
                        persona = %ctx.identity.agent_name,
                        lamport = msg.lamport,
                        reason = "workspace-pass",
                        gated = gated,
                        pass_reason = %pass_detail,
                        "persona chose silence, or a gate converted her draft into a pass (gated) — pass_reason says which"
                    );
                    // THE ACT-QUESTION. Asked here on the PASS path and again after a spoken reply,
                    // so holding work is what makes it fire — not declining to speak.
                    // `directed` is recomputed here rather than threaded: it is a pure function of
                    // (identity, msg.text) and the binding above lives inside the cycle branch.
                    let spoke_directed = ctx.identity.persona_identity().mentions(&msg.text);
                    let work = crate::persona::act_question::ask_the_act_question(
                        ctx,
                        conversation,
                        msg.lamport,
                        turn_room,
                        spoke_directed,
                        // A message turn never offers her initiative: the message snapped her beat to
                        // engaged, and that beat (the self-tick) offers it.
                        crate::persona::initiative_turn::Initiative::NotOffered,
                    )
                    .await;
                    deferred_input.settle_work(&work);
                    outcome.turns_skipped += 1;
                    return CycleVerdict::Skip;
                }
                crate::cognition::act_observe::SettleStep::ReceiptPersistenceFailed { error } => {
                    tracing::error!(persona = %ctx.identity.agent_name, lamport = msg.lamport,
                        error = %error, "render receipt checkpoint failed; generation served, action not executed");
                    return CycleVerdict::CheckpointFailed(error);
                }
                crate::cognition::act_observe::SettleStep::InferenceFailed { error } => {
                    // The model call FAILED (timeout, 5xx, the serving lane
                    // refusing a model it isn't hosting) — NOT a chosen silence.
                    // Surface it LOUD and skip the turn; the next tick retries
                    // against whatever the serving daemon has resident. Never
                    // fabricate a Pass over a broken lane
                    // ([[fallbacks-are-illegal-fail-loud]]).
                    tracing::warn!(
                        lamport = msg.lamport,
                        error = %error,
                        "deliberation inference FAILED — skipping turn (not a chosen silence)"
                    );
                    continuum_base::probe!(
                        class = "persona.turn.inference_failed",
                        persona = %ctx.identity.agent_name,
                        lamport = msg.lamport,
                        error = %error,
                        "deliberation model call failed; turn skipped, retries next tick"
                    );
                    outcome.turns_skipped += 1;
                    return CycleVerdict::Skip;
                }
            }
        }
        None => {
            // No WorkspaceCycle for a persona actively servicing turns means the
            // supervisor's spawn-time `register_from_cfg` never ran or was evicted
            // — a wiring bug, not a transient. Fail loud and drop the turn; the
            // legacy `respond()` fallback is deleted
            // ([[fallbacks-are-illegal-fail-loud]]: a fallback fires 100% on the
            // failure it hides — here it would silently mask a dead brain by
            // routing cognition down a parallel path that no longer exists for the
            // living persona).
            phase_timings.respond_ms = respond_started.elapsed().as_millis() as u64;
            tracing::error!(
                persona = %ctx.identity.agent_name,
                persona_id = %ctx.identity.peer_id.as_uuid(),
                lamport = msg.lamport,
                "no WorkspaceCycle registered — spawn wiring bug; dropping turn (no respond() fallback)"
            );
            continuum_base::probe!(
                class = "persona.turn.error",
                persona = %ctx.identity.agent_name,
                lamport = msg.lamport,
                stage = "no-cycle",
                "no WorkspaceCycle registered; failing loud (no respond() fallback)"
            );
            outcome.turns_errored += 1;
            return CycleVerdict::Skip;
        }
    }
}

/// The attention a NEW message earns from its actual sender and mentions: the one derivation,
/// shared by the turn and by the loop when it must retain an input it cannot yet run.
pub(super) fn new_message_attention(
    ctx: &HostedPersona,
    msg: &crate::cognition::conversation::IncomingMessage,
) -> crate::cognition::workspace::TurnAttention {
    crate::cognition::workspace::TurnAttention::for_message(
        ctx.identity.persona_identity().mentions(&msg.text),
        crate::cognition::room_directory::is_human_peer(msg.peer_id),
    )
}
