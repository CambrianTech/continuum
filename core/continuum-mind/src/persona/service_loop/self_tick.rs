use super::*;

/// One cycle her own mind woke her for (what her senses published: a loud room, her
/// continuation falling due, resuming after a restart): runs the self-cycle over the
/// CURRENT world-state. No clock calls this.
pub(super) async fn serve_self_tick(
    ctx: &HostedPersona,
    conversation: &mut dyn CitizenConversation,
    opts: &ServeOptions,
    last_burst_fp: &mut u64,
    wake_room: Option<uuid::Uuid>,
    pending_before_wake: crate::cognition::directed_pending::Observation,
    pending: &mut crate::persona::wake_backlog::PendingInput,
) {
    // With `biased;` the inbox was polled first: her mind's wake winning means nothing
    // admissible was queued, so any pending directed flag is stale (raised for a line that
    // filtered at the door). Clear it, or every self-work lane wait yields forever.
    crate::cognition::directed_pending::acknowledge(
        ctx.identity.peer_id.as_uuid(),
        pending_before_wake,
    );
    // (Quiescence is handled at the top of the loop — a held lease never reaches here.)
    //
    // A self-cycle is the lowest-priority work on the node: its generation asks the lane
    // at `LanePriority::Ambient`, behind directed and held work, for one generation at a
    // time. No permit spans the cycle, so her deterministic work and tools never hold
    // capacity from another mind (LOCKS-AND-FIFOS-AUDIT.md row 9).
    //
    // Nobody is waiting on a self-cycle: it renders the prompt an unattended turn can
    // afford (card 7496ed9d) — the message path stays interactive.
    crate::cognition::audience::with(
        crate::inference::prefill_rate::Audience::Unattended,
        run_self_cycle(ctx, conversation, opts, last_burst_fp, wake_room, pending),
    )
    .await;
}
