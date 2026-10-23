use super::*;

/// This persona's own heartbeat rhythm: the quick `engaged` beat, the `rest_cap` it drifts
/// toward when alone, and the `next` beat currently in force.
pub(super) struct BeatClock {
    pub(super) engaged: std::time::Duration,
    pub(super) rest_cap: std::time::Duration,
    pub(super) next: std::time::Duration,
}

/// One never-stop heartbeat cycle (a Tick, or her own mind's wake): runs the self-cycle
/// over the CURRENT world-state and sets the next beat from what it found.
pub(super) async fn serve_self_tick(
    ctx: &HostedPersona,
    conversation: &mut dyn PersonaConversation,
    opts: &ServeOptions,
    last_burst_fp: &mut u64,
    wake_room: Option<uuid::Uuid>,
    pending_before_wake: crate::cognition::directed_pending::Observation,
    beat: &mut BeatClock,
) {
    // With `biased;` the inbox was polled first: a tick winning means
    // nothing admissible was queued, so any pending directed flag is
    // stale (raised for a line that filtered at the door). Clear it,
    // or every self-work lane wait yields forever.
    crate::cognition::directed_pending::acknowledge(ctx.identity.peer_id.as_uuid(), pending_before_wake);
    // (Quiescence is handled at the top of the loop — a held lease never
    // reaches here.) Heartbeat slice — the mind gets time with no inbound
    // activity sets the next beat: if it found something new to work on
    // (last_burst_fp advanced), stay quick; if it went idle, drift toward rest.
    //
    // ── Idle admission under lane pressure (#139) ──────────────────────
    // A self-tick is the lowest-priority work in the system: the mind
    // musing over an unchanged world on its own free time. When every shared
    // decode slot is already busy (glass-boxed 2026-07-15: one inbound
    // message woke six minds, each ran a full ~54s deliberation, and the two
    // lanes serialized them into a 250s tail), spending one on an idle
    // deliberation only deepens the queue that LIVE conversation is waiting
    // behind. So under saturation this beat YIELDS and drifts toward rest —
    // the mind keeps its free time for when a lane is actually free. Message
    // turns are never gated here (the Wake::Msg arm below); a human or peer
    // is waiting, they always get served immediately.
    // [[idle-is-self-directed-free-time]]
    // [[conversational-latency-is-a-misdirection-budget]]
    // Idle admission is a PERMIT, not a gauge — the fix the ambient-reply
    // path (below, ~line 980) already made and this self-tick path never
    // received. The gauge (`shared_model_saturated`) has a stampede race
    // (#139/#385): N idle minds waking on the SAME cadence all read
    // inflight=0 before any has generated a token, all pass, and all fire at
    // the shared decode slot at once — oversubscribing a 1-lane node and
    // #385-wedging it (glass-boxed 2026-08-10: resident_personas=4 vs
    // warm_slots=1, every self-tick died on "no TOKEN progress for 90s").
    // The permit is sized to the LIVE served lane count (LaneAdmission ←
    // set_served_lane_count — true since 2026-08-17; it was a hardcoded 1 for
    // the whole time this comment claimed otherwise) and HELD across the whole
    // self-cycle, so ambient
    // concurrency is bounded to real capacity no matter when everyone woke —
    // the surplus minds genuinely yield toward rest instead of stampeding.
    // Directed turns still bypass entirely (they were named). Self-tick and
    // ambient replies share the same ambient pool — both are lowest-priority
    // non-directed work competing for the same lanes.
    let before = *last_burst_fp;
    // DETERMINISTIC WORK FIRST, THEN INFERENCE. The cycle runs the held-work
    // act question (which takes its OWN serving lane through the delib gate)
    // and the kanban pull (no inference at all) before it ever asks for an
    // ambient permit — that permit caps only the musing tail. Until
    // 2026-09-03 the permit was taken HERE, before anything ran, so with 12
    // citizens on a lanes-1 ambient pool nine of them yielded every tick and
    // never reached the pull (measured live: 31 yields in 10 minutes, 3 of 12
    // residents working an Open deck of 12).
    // Nobody is waiting on a self-cycle: it renders the prompt an unattended
    // turn can afford (card 7496ed9d) — the message path below stays interactive.
    let starved = crate::cognition::audience::with(
        crate::inference::prefill_rate::Audience::Unattended,
        run_self_cycle(ctx, conversation, opts, last_burst_fp, wake_room),
    )
    .await;
    if starved {
        if let Some(yields) =
            crate::inference::resource_admission::take_ambient_yield_report(
                EpochMs::now().millis(), // pre-1970 reads 0: clock before the epoch = 0
            )
        {
            continuum_base::probe!(
                class = "persona.selftick.starved",
                yields_since_last_report = yields,
                total_yields =
                    crate::inference::resource_admission::ambient_yields(),
                "ambient MUSING yielded — the pool was full. NOT a fault: this is \
                 what contention looks like when citizens outnumber non-directed \
                 lanes; held work and the pull already ran. Rate-limited to ≤1 row/min."
            );
        }
    }
    beat.next = next_beat_after(
        if *last_burst_fp != before {
            BeatOutcome::Engaged
        } else if starved {
            BeatOutcome::YieldedNoSlot
        } else {
            BeatOutcome::NothingNew
        },
        beat.next,
        beat.engaged,
        beat.rest_cap,
    );
}
