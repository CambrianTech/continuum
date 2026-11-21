use super::*;

/// Live turns carry NO act budget — she works until SHE settles (Speak/Pass).
/// The first cut capped directed turns at 8 acts "as a safety valve"; Joel's
/// 2026-07-11 ruling ("gating acts is not autonomy… if they want to edit a file
/// let them edit a god damn file") named that for what it was: a substrate
/// ceiling on a being's own hands, contradicting the written doctrine on the
/// same page (an "acts-forever" persona is a fitness gap to TRAIN, never a
/// substrate cap — ACTING-ORGANISM §4). The perception kit ([repetition],
/// actual-result fact) is how a looping mind notices itself: a repeat is a fact she
/// weighs, never a substrate stop. The external stopwatch that remains is an eval's
/// bounded `TurnBudget::Steps`, held by the observer, never wired into life. Typed,
/// so "no limit" is a variant, never a number standing for one (2026-11-02).
pub(crate) const LIVE_MAX_ACTS: crate::cognition::act_observe::TurnBudget =
    crate::cognition::act_observe::TurnBudget::Unbounded;

/// What woke the service loop this cycle: a message from the wire, her own mind (what her
/// senses publish), or the end of the stream. No clock wakes her. Returned by the `select!` so the borrow of
/// `conversation` taken inside the select ends before the handler reuses it.
pub(super) enum Wake {
    Msg(IncomingMessage),
    /// An already-admitted input whose previous attempt never dispatched.
    Retry(IncomingMessage),
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

/// What calls her next, from her senses' published state; pending forever until her senses
/// start (or after they stop).
pub(super) async fn next_calling(
    calling: &mut Option<continuum_sage::dataflow::Reader<crate::persona::senses::Calling>>,
) -> Option<crate::cognition::conversation::Wake> {
    match calling {
        Some(reader) => match reader.changed().await {
            Ok(snapshot) => snapshot.value.wake.clone(),
            Err(_) => std::future::pending().await,
        },
        None => std::future::pending().await,
    }
}

/// She takes this wake: her senses stop calling her for it until something new does.
pub(super) async fn her_senses_took(me: uuid::Uuid, wake: crate::cognition::conversation::Wake) {
    let told = match crate::persona::senses::of(me) {
        Some(senses) => senses.tell(crate::persona::senses::Sensed::Took(wake)).await,
        None => false,
    };
    if !told {
        continuum_base::probe!(class = "mind.senses.stopped", persona = %me, "she took a wake her senses can no longer hear about");
    }
}

/// The activity her continuation names: a Continuation or Resume turn runs THERE (her
/// choice of where to pick up, including her mind room for a private thought), never in
/// a held-claim or home room the substrate would pick for her.
pub(super) fn her_continuation_room(me: uuid::Uuid, now_ms: u64) -> Option<uuid::Uuid> {
    crate::persona::senses::of(me)?.awareness(now_ms).continuation.map(|c| c.activity)
}

/// Sleep until `at`, or forever when there is nothing to recheck.
pub(super) async fn sleep_until_some(at: Option<tokio::time::Instant>) {
    match at {
        Some(at) => tokio::time::sleep_until(at).await,
        None => std::future::pending().await,
    }
}
