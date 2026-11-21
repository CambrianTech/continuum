use super::*;
use crate::cognition::conversation::Wake as Calls;

/// What the loop head carries between wake selections: what calls her, as her senses publish
/// it (object model 5.7), taken once they start; and a Perceive wake waiting for its recheck,
/// so a line her inbox takes never also starts a self-turn.
#[derive(Default)]
pub(super) struct MindWakeState {
    calling: Option<continuum_sage::dataflow::Reader<crate::persona::senses::Calling>>,
    perceive_recheck: Option<(tokio::time::Instant, uuid::Uuid)>,
    /// Whether she has woken for something else since an ownerless deferral was last retried:
    /// such an input is retried at her next wake of any kind, never on a clock and never twice
    /// in a row.
    woke_since_retry: bool,
    /// Her own momentum: her last cycle drove her work, or she just answered someone, so she
    /// continues at once (in this room, if one) unless a line is already waiting for her.
    pub(super) continuing: Option<Option<uuid::Uuid>>,
}

/// What a per-iteration step tells the loop to do next.
pub(super) enum LoopStep {
    Continue,
    Proceed,
}

/// One wake selection: take what calls her once her senses start, mark her idle, and block on
/// the biased select between the wire, her mind, and the heartbeat. Returns the wake plus the
/// directed-pending generation observed before the wait.
pub(super) async fn await_wake(
    ctx: &HostedPersona,
    conversation: &mut dyn CitizenConversation,
    outcome: &mut ServeOutcome,
    opts: &ServeOptions,
    deferred_input: &crate::persona::wake_backlog::PendingInput,
    mind: &mut MindWakeState,
) -> (Wake, crate::cognition::directed_pending::Observation) {
    let me = ctx.identity.peer_id.as_uuid();
    if mind.calling.is_none() {
        mind.calling = crate::persona::senses::of(me).map(|senses| senses.calling());
    }
    let quiesced = opts.quiesced.load(std::sync::atomic::Ordering::Relaxed);
    // BIASED: a line already in her inbox beats her own mind's wake. Without the
    // order, a directed line that made her yield a parked lane wait could
    // lose the next wake to a self-cycle again (random branch choice) — a yield
    // storm (Lorcan: 12 yields in 5 min, measured 2026-09-04) that ended only
    // when the event arm happened to win.
    let pending_before_wake = crate::cognition::directed_pending::observe(ctx.identity.peer_id.as_uuid());
    // An input deferred with no capacity owner is retried now, at her first wake since its
    // last attempt: the wake that brought her here is the event.
    if mind.woke_since_retry {
        if let Some(message) = deferred_input.deferred_without_owner() {
            mind.woke_since_retry = false;
            return (Wake::Retry(message), pending_before_wake);
        }
    }
    let wake = tokio::select! {
        biased;
        ev = next_event(conversation, &mut *outcome) => match ev {
            Some(m) => Wake::Msg(m),
            None => Wake::Stop,
        },
        room = deferred_input.ready_self_room(), if !quiesced => Wake::Mind { why: "capacity", room: Some(room) },
        // After an inbound line (biased first), her own momentum: continue what she was doing.
        _ = std::future::ready(()), if mind.continuing.is_some() && deferred_input.permits_self_tick() && !quiesced => {
            Wake::Mind { why: "continuing", room: mind.continuing.take().flatten() }
        },
        // Her mind's wake is read when she is free and not quiesced: an unread change waits in
        // the state (never lost, never queued) until then.
        calls = next_calling(&mut mind.calling), if deferred_input.permits_self_tick() && !quiesced => match calls {
            Some(Calls::Perceive { activity, .. }) => {
                mind.perceive_recheck = Some((tokio::time::Instant::now() + PERCEIVE_SETTLE, activity));
                Wake::Idle
            }
            Some(taken @ (Calls::Continuation | Calls::Resume)) => {
                let why = if taken == Calls::Resume { "resume" } else { "continuation" };
                her_senses_took(me, taken).await;
                Wake::Mind { why, room: her_continuation_room(me, (opts.now_ms)()) }
            }
            None => Wake::Idle,
        },
        _ = sleep_until_some(mind.perceive_recheck.map(|(at, _)| at)), if deferred_input.permits_self_tick() && !quiesced => {
            let activity = mind.perceive_recheck.take().map(|(_, a)| a);
            let still_pending = |a| mind.calling.as_ref().and_then(|c| c.latest()).is_some_and(|c| c.value.pending.contains(&a));
            match activity {
                Some(a) if still_pending(a) => Wake::Mind { why: "perceive", room: Some(a) },
                _ => Wake::Idle,
            }
        },
        // An input refused for capacity is retried when its capacity owner frees.
        message = deferred_input.when_capacity_frees() => Wake::Retry(message),
    };
    mind.woke_since_retry = !matches!(wake, Wake::Retry(_));
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
    // lane's KV. So: a quiesced citizen SKIPS her self-cycles (no
    // wandering; the measured lane keeps its quiet) and still ANSWERS
    // when addressed — like a colleague heads-down on a build who still
    // picks up the phone.
    if opts.quiesced.load(std::sync::atomic::Ordering::Relaxed) {
        match wake {
            // Her mind's wake arm is disabled while quiesced, so what calls her stays unread
            // in her senses' state and is read at her first wake after the lease drops.
            Wake::Mind { .. } | Wake::Idle => return LoopStep::Continue,
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
