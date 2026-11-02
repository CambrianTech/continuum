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
    conversation: &mut dyn CitizenConversation,
    opts: &ServeOptions,
    last_burst_fp: &mut u64,
    wake_room: Option<uuid::Uuid>,
    pending_before_wake: crate::cognition::directed_pending::Observation,
    beat: &mut BeatClock,
    pending: &mut crate::persona::wake_backlog::PendingInput,
) {
    // With `biased;` the inbox was polled first: a tick winning means
    // nothing admissible was queued, so any pending directed flag is
    // stale (raised for a line that filtered at the door). Clear it,
    // or every self-work lane wait yields forever.
    crate::cognition::directed_pending::acknowledge(
        ctx.identity.peer_id.as_uuid(),
        pending_before_wake,
    );
    // (Quiescence is handled at the top of the loop — a held lease never
    // reaches here.) Heartbeat slice — the mind gets time with no inbound
    // activity sets the next beat: if it found something new to work on
    // (last_burst_fp advanced), stay quick; if it went idle, drift toward rest.
    //
    // A self-tick is the lowest-priority work on the node: its generation asks the lane
    // at `LanePriority::Ambient`, behind directed and held work, for one generation at a
    // time. No permit spans the cycle, so her deterministic work and tools never hold
    // capacity from another mind (LOCKS-AND-FIFOS-AUDIT.md row 9).
    let before = *last_burst_fp;
    // Deterministic work first (the held-work act question, the kanban pull), then the
    // musing tail.
    // Nobody is waiting on a self-cycle: it renders the prompt an unattended
    // turn can afford (card 7496ed9d) — the message path below stays interactive.
    crate::cognition::audience::with(
        crate::inference::prefill_rate::Audience::Unattended,
        run_self_cycle(ctx, conversation, opts, last_burst_fp, wake_room, pending),
    )
    .await;
    beat.next = next_beat_after(
        if *last_burst_fp != before {
            BeatOutcome::Engaged
        } else {
            BeatOutcome::NothingNew
        },
        beat.next,
        beat.engaged,
        beat.rest_cap,
    );
}
