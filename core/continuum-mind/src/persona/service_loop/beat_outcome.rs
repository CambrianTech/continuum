use super::*;

/// What a self-tick beat DID, and therefore what the next interval should be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BeatOutcome {
    /// She ran a cycle and the room had something new → stay engaged.
    Engaged,
    /// She ran a cycle and nothing had changed → rest deeper (earned backoff).
    NothingNew,
}

/// The self-tick cadence rule. Pure so every row is table-testable without a runtime.
pub fn next_beat_after(
    outcome: BeatOutcome,
    current: std::time::Duration,
    engaged: std::time::Duration,
    rest_cap: std::time::Duration,
) -> std::time::Duration {
    match outcome {
        BeatOutcome::Engaged => engaged,
        BeatOutcome::NothingNew => (current + current / 2).min(rest_cap),
    }
}

/// Engaged heartbeat period — how often a persona pursuing its OWN intentions
/// (no inbound message) gets another self-directed slice. This is the SELF-CHATTER
/// pace, NOT message responsiveness: a real message wakes the loop instantly through
/// `next_event` in the `select!`, regardless of this beat. So this trades ONLY
/// background self-direction against the shared GPU — and it MUST leave headroom for
/// a real conversation to feel like magic. Glass-boxed 2026-07-14: at 3_000ms, four
/// idle personas each re-fired a full ~20s LLM turn every 3s → ~90% GPU duty cycle
/// EACH → a user's message crawled at 0.3 tok/s (67s for one line) while quiescing the
/// fleet jumped it to 2.3 tok/s (8×). Background thinking must be LOW duty cycle so the
/// active speaker owns the GPU; a friend who answers in a beat is alive, one who makes
/// you wait 67s is a lab demo. (The deeper solo-speed lever — LoRA overhead + `--parallel`
/// splitting one 24B — is a separate serving-config fix.)
/// [[multimodal-live-mode-is-a-latency-obsession-cbar-doctrine]]
pub(super) const SELF_TICK_MS: u64 = 15_000;

/// Restful ceiling for the intrinsic heartbeat. A truly idle persona's beat backs off
/// toward this (exponential, NOT a fixed metronome), so a quiet citizen rests deeply
/// instead of hammering the shared model, and many idle minds spread across time rather
/// than stampeding it. Raised with the engaged beat so an idle fleet leaves the GPU
/// almost entirely free for live conversation + video. A message or fresh work snaps the
/// beat back to `SELF_TICK_MS`. See the loop in `serve_persona_loop`.
pub(super) const SELF_TICK_REST_CAP_MS: u64 = 240_000;

/// Live turns carry NO act budget — she works until SHE settles (Speak/Pass).
/// The first cut capped directed turns at 8 acts "as a safety valve"; Joel's
/// 2026-07-11 ruling ("gating acts is not autonomy… if they want to edit a file
/// let them edit a god damn file") named that for what it was: a substrate
/// ceiling on a being's own hands, contradicting the written doctrine on the
/// same page (an "acts-forever" persona is a fitness gap to TRAIN, never a
/// substrate cap — ACTING-ORGANISM §4). The perception kit ([repetition],
/// actual-result fixed-point guard) is how a looping mind notices itself. Three
/// consecutive repeats of both request and result yield the turn without declaring
/// its work complete; changed results reset that streak. The external
/// stopwatch that remains is the eval grader's `max_acts` — a proctored exam's
/// clock, held by the observer, never wired into life.
pub(crate) const LIVE_MAX_ACTS: usize = usize::MAX;

/// What woke the service loop this cycle. A message from the wire, the never-stop
/// heartbeat, or the end of the stream. Returned by the `select!` so the borrow of
/// `conversation` taken inside the select ends before the handler reuses it.
pub(super) enum Wake {
    Msg(IncomingMessage),
    /// An already-admitted input whose previous attempt never dispatched.
    Retry(IncomingMessage),
    Tick,
    /// Her own mind woke her (EVENT-MIND §1b): a room still unread and loud past her
    /// dial, her continuation falling due, or resuming a saved one after a restart.
    /// `room` is where she wakes (Perceive), else her continuation's activity.
    Mind { why: &'static str, room: Option<uuid::Uuid> },
    /// A wake that needs no turn (a Perceive whose line her inbox already took); the
    /// loop goes back to its select.
    Idle,
    Stop,
}

/// How long a Perceive wake waits before it may start a turn: the same line reaches her
/// inbox through the pump, and a line the inbox takes marks its room perceived, so a
/// recheck after this finds nothing pending and no second turn starts.
pub(super) const PERCEIVE_SETTLE: std::time::Duration = std::time::Duration::from_millis(1_000);

/// The slow re-perceive floor, by HER dial (Fable, 2026-10-04: "her opening her eyes,
/// not the tick choosing"): how long her mind may go without checking whether a
/// continuation fell due or a room is loud and unread.
pub(super) fn mind_check_floor(dial: crate::cognition::attention::AttentionDial) -> std::time::Duration {
    use crate::cognition::attention::Depth;
    std::time::Duration::from_secs(match dial.depth {
        Depth::Deep => 15 * 60,
        Depth::Normal => 5 * 60,
        Depth::Broad => 2 * 60,
    })
}

/// The next wake from her region's channel; pending forever while she has none.
pub(super) async fn next_mind_wake(
    wakes: &mut Option<tokio::sync::mpsc::Receiver<crate::cognition::conversation::Wake>>,
) -> crate::cognition::conversation::Wake {
    match wakes {
        Some(rx) => match rx.recv().await {
            Some(w) => w,
            None => std::future::pending().await,
        },
        None => std::future::pending().await,
    }
}

/// The activity her continuation names: a Continuation or Resume turn runs THERE (her
/// choice of where to pick up, including her mind room for a private thought), never in
/// a held-claim or home room the substrate would pick for her.
pub(super) fn her_continuation_room(me: uuid::Uuid) -> Option<uuid::Uuid> {
    crate::persona::perception_feed::with_region(me, |r| r.continuation().map(|c| c.activity)).flatten()
}

/// Sleep until `at`, or forever when there is nothing to recheck.
pub(super) async fn sleep_until_some(at: Option<tokio::time::Instant>) {
    match at {
        Some(at) => tokio::time::sleep_until(at).await,
        None => std::future::pending().await,
    }
}
