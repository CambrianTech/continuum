use super::*;

/// What the loop head carries between wake selections: her mind's wakes (EVENT-MIND
/// §1b, phase 2) taken once her region boots; a timer at her continuation's deadline or
/// the floor of her dial; Perceive wakes wait for their recheck so a line her inbox
/// takes never also starts a self-turn.
#[derive(Default)]
pub(super) struct MindWakeState {
    wakes: Option<tokio::sync::mpsc::Receiver<crate::persona::perception_region::Wake>>,
    next_mind_check: Option<tokio::time::Instant>,
    perceive_recheck: Option<(tokio::time::Instant, uuid::Uuid)>,
}

/// What a per-iteration step tells the loop to do next.
pub(super) enum LoopStep {
    Continue,
    Proceed,
}

/// One wake selection: take her mind's wakes if they have booted, mark her idle, and
/// block on the biased select between the wire, her mind, and the heartbeat. Returns the
/// wake plus the directed-pending generation observed before the wait.
pub(super) async fn await_wake(
    ctx: &HostedPersona,
    conversation: &mut dyn PersonaConversation,
    outcome: &mut ServeOutcome,
    opts: &ServeOptions,
    deferred_input: &crate::persona::wake_backlog::PendingInput,
    mind: &mut MindWakeState,
    next_beat: std::time::Duration,
) -> (Wake, crate::cognition::directed_pending::Observation) {
    let me = ctx.identity.peer_id.as_uuid();
    if mind.wakes.is_none() {
        mind.wakes = conversation.take_wakes();
        if mind.wakes.is_some() {
            // First look right away: a saved continuation resumes now (rule 8).
            mind.next_mind_check = Some(tokio::time::Instant::now());
        }
    }
    // HER turn boundary: idle whenever the loop is back at its wake select. The
    // matching `engaged` stamp is set where a serving lane is actually acquired
    // (the deliberation faculty) — a room wake alone is not wakefulness, or a
    // busy room would cancel every dream (measured: 290 paged out in 20 min with
    // the stamp on the wake itself).
    crate::cognition::activity_gate::persona_idle(ctx.identity.peer_id.as_uuid());
    // BIASED: a line already in her inbox beats a due self-tick. Without the
    // order, a directed line that made her yield a parked lane wait could
    // lose the next wake to the tick again (random branch choice) — a yield
    // storm (Lorcan: 12 yields in 5 min, measured 2026-09-04) that ended only
    // when the event arm happened to win.
    let pending_before_wake = crate::cognition::directed_pending::observe(ctx.identity.peer_id.as_uuid());
    let wake = tokio::select! {
        biased;
        ev = next_event(conversation, &mut *outcome), if deferred_input.message().is_none() => match ev {
            Some(m) => Wake::Msg(m),
            None => Wake::Stop,
        },
        w = next_mind_wake(&mut mind.wakes), if deferred_input.message().is_none() => match w {
            crate::persona::perception_region::Wake::Perceive { activity, .. } => {
                mind.perceive_recheck = Some((tokio::time::Instant::now() + PERCEIVE_SETTLE, activity));
                Wake::Idle
            }
            crate::persona::perception_region::Wake::Continuation => Wake::Mind { why: "continuation", room: her_continuation_room(me) },
            crate::persona::perception_region::Wake::Resume => Wake::Mind { why: "resume", room: her_continuation_room(me) },
        },
        _ = sleep_until_some(mind.perceive_recheck.map(|(at, _)| at)), if deferred_input.message().is_none() => {
            let activity = mind.perceive_recheck.take().map(|(_, a)| a);
            match activity {
                Some(a) if crate::persona::perception_feed::with_region(me, |r| r.is_pending(a)).unwrap_or(false) => {
                    Wake::Mind { why: "perceive", room: Some(a) }
                }
                _ => Wake::Idle,
            }
        },
        _ = sleep_until_some(mind.next_mind_check), if deferred_input.message().is_none() => {
            let now = (opts.now_ms)();
            let looked = crate::persona::perception_feed::with_region(me, |r| {
                let wake = r.wake_for(now);
                let floor = tokio::time::Instant::now() + mind_check_floor(r.dial());
                let due = r.continuation_due_ms().filter(|due| *due > now).map(|due| {
                    tokio::time::Instant::now() + std::time::Duration::from_millis(due - now)
                });
                (wake, due.map_or(floor, |d| d.min(floor)))
            });
            match looked {
                Some((wake, next)) => {
                    mind.next_mind_check = Some(next);
                    match wake {
                        Some(crate::persona::perception_region::Wake::Perceive { activity, .. }) => {
                            mind.perceive_recheck = Some((tokio::time::Instant::now() + PERCEIVE_SETTLE, activity));
                            Wake::Idle
                        }
                        Some(crate::persona::perception_region::Wake::Continuation) => Wake::Mind { why: "continuation", room: her_continuation_room(me) },
                        Some(crate::persona::perception_region::Wake::Resume) => Wake::Mind { why: "resume", room: her_continuation_room(me) },
                        None => Wake::Idle,
                    }
                }
                None => {
                    mind.next_mind_check = None;
                    Wake::Idle
                }
            }
        },
        retry = deferred_input.at_next_beat(next_beat) => match retry {
            Some(message) => Wake::Retry(message),
            None => Wake::Tick,
        },
    };
    (wake, pending_before_wake)
}

/// Quiescence under an eval-preemption lease: suspends WANDERING, never reachability.
/// `Continue` = the loop skips this wake; `Proceed` = serve it.
pub(super) fn quiesce_gate(opts: &ServeOptions, wake: &Wake) -> LoopStep {
    // QUIESCE HONORS ITS OWN CONTRACT (Joel, 2026-08-30: "I could call
    // them up or dm — just want to make sure we're not into singular
    // activity mode again"). The lease's documented promise is "skips
    // INITIATING self-directed turns — she stays online and still answers
    // explicit work" — but this arm was implemented as a full coma
    // (neither ticks NOR inbound; DMs buffered until the lease dropped).
    // The coma's original justification — an in-flight conversation would
    // keep the GPU busy through the measurement — predates the slot
    // economy: a directed reply now decodes on its own slot under a
    // permit sized to the served slots, without touching the measured
    // lane's KV. So: a quiesced citizen SKIPS her self-tick (no
    // wandering; the measured lane keeps its quiet) and still ANSWERS
    // when addressed — like a colleague heads-down on a build who still
    // picks up the phone.
    if opts.quiesced.load(std::sync::atomic::Ordering::Relaxed) {
        match wake {
            // A suppressed beat leaves the cadence untouched — when the
            // lease drops she resumes at whatever rhythm she had earned.
            // Her own wake under the lease is skipped like a beat; it is not lost:
            // a due continuation or a loud room re-yields on her next mind check.
            Wake::Tick | Wake::Mind { .. } | Wake::Idle => return LoopStep::Continue,
            Wake::Msg(_) | Wake::Retry(_) => continuum_base::probe!(
                class = "persona.quiesced.directed_served",
                "quiesced citizen serving an inbound turn — leases suspend wandering, never reachability"
            ),
            // Stop falls through to the arm below.
            Wake::Stop => {}
        }
    }
    LoopStep::Proceed
}
