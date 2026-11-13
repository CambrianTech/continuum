//! Card-state transitions onto the internal bus: the one emitter, fed by the `work/state`
//! verb and by the wire echo a citizen's inbound stream hears.

use std::sync::{Arc, OnceLock};

use serde_json::Value;
use uuid::Uuid;

use crate::runtime::{MessageBus, ModuleRegistry};

/// The bus event published the moment a card transitions — by [`bridge_wire_work_event`]
/// from the transition's own wire echo, so EVERY writer fires it: a citizen's
/// `work/state`, the operator's `airc work state`, a remote peer. Subscribers
/// glob-match this to REACT (e.g. the SWE grade-on-done handler, board freshness,
/// auto-close) — the system is event-based, never polling
/// ([[the-whole-system-is-event-based-not-polling]]). Payload: `{card_id, state}`.
pub const WORK_CARD_STATE_CHANGED: &str = "work.card.state_changed";

/// Process-global handle to the bus + registry so [`bridge_wire_work_event`] (called
/// from persona inbound streams, which hold no `ModuleContext`) can publish the
/// transition event. Set once by `WorkModule::initialize`; the bus and registry ARE
/// process-global (one core), same granularity as the gossip ledger and the admission
/// gates — so a process-global is the honest shape, not a field threaded through every
/// command constructor.
pub static WORK_EVENT_BUS: OnceLock<(Arc<MessageBus>, Arc<ModuleRegistry>)> = OnceLock::new();

/// Decode a wire transcript event into the `work.card.state_changed` payload — or
/// `None` if it isn't a card-state transition. Pure over the event so the contract
/// is unit-testable against the real producer (`airc_work::encode_work_event`).
///
/// Payload contract: `{card_id, state}` with `card_id` the FULL hyphenated UUID and
/// `state` the snake_case `CardState` serde form ("closed"/"merged"/…) — the exact
/// vocabulary `benchmark_grade::is_terminal` and `parse_state` already speak.
pub fn wire_card_state_payload(event: &airc_core::TranscriptEvent) -> Option<Value> {
    if !airc_work::transcript_is_work_event(event) {
        return None;
    }
    let item = airc_work::decode_transcript_work_event(event).ok()?;
    let airc_work::WorkEvent::CardStateChanged(changed) = item.event else {
        return None;
    };
    let state = serde_json::to_value(changed.state).ok()?;
    Some(serde_json::json!({
        "card_id": changed.card_id.as_uuid().to_string(),
        "state": state,
        // The card's OWN room — boards are per-room, so a subscriber that reads
        // the board or posts a verdict must scope to THIS room, never to whatever
        // current_room() happens to be (the documented #345 wrong-room trap;
        // found live 2026-08-15 when the grader read the grading citizen's
        // academy board and never found the bench card).
        "room_id": event.room_id.as_uuid().to_string(),
    }))
}

/// Once-per-process sighting of a (card_id, state) TRANSITION — the cross-path dedup
/// between the in-process verb emit and the wire-echo bridge.
///
/// # Why two feeders exist at all (2026-08-17, the night two solved cards graded as
/// nothing)
///
/// `work/state` used to rely ENTIRELY on its own transcript echo returning through a
/// persona subscribe stream (`bridge_wire_work_event`, the only caller). Measured that
/// night: Atlas closed astropy-14182 and astropy-14995 — both `Closed` in the store,
/// on the board — and `persona.inbound.raw_event` counted 2 rows in 80 minutes (a
/// comparable window earlier the same day: 83). #434 (post-reboot durable delivery to
/// citizen scopes down) starves the echo, the bridge never fires, the grader never
/// hears, and finished work grades as nothing. A grade tail that depends on wire
/// delivery working is a grade tail with a known-open bug in its spine.
///
/// So the VERB now emits directly (in-process, delivery-proof) and the bridge remains
/// for every writer that is NOT this core's `work/state` (operator CLI, remote peers).
/// When delivery works both paths see the same transition; THIS ring makes it publish
/// once. Keyed by (card, state) rather than wire event id because the verb path has no
/// wire event yet. A LEGITIMATE re-transition to the same state hours later would be
/// deduped only if the ring still held it — 256 transitions of churn ages it out long
/// before that matters.
pub fn first_transition_sighting(card_id: &str, state: &str) -> bool {
    use std::collections::VecDeque;
    use std::sync::Mutex;
    // derived-or-floor: a dedup ring for duplicates that arrive within one fan-out instant, sized
    // far above any burst; an aged-out id only risks a duplicate publish, never a loss.
    const SEEN_CAP: usize = 256;
    static SEEN: OnceLock<Mutex<VecDeque<String>>> = OnceLock::new();
    let key = format!("{card_id}\u{1}{state}");
    let seen = SEEN.get_or_init(|| Mutex::new(VecDeque::with_capacity(SEEN_CAP)));
    let mut seen = seen.lock().unwrap_or_else(|p| p.into_inner()); // poisoned lock = read the last state, same policy as every lock in this crate
    if seen.contains(&key) {
        return false;
    }
    if seen.len() >= SEEN_CAP {
        seen.pop_front();
    }
    seen.push_back(key);
    true
}

/// Publish one card-state transition onto the internal bus — the ONE emitter both
/// feeders (the `work/state` verb, the wire bridge) route through. Dedup lives HERE so
/// neither feeder needs to know the other exists.
pub(crate) async fn emit_card_state_changed(payload: Value, via: &'static str) {
    let card_id = payload["card_id"].as_str().unwrap_or("").to_string();
    let state = payload["state"].as_str().unwrap_or("").to_string();
    if !first_transition_sighting(&card_id, &state) {
        return;
    }
    continuum_base::probe!(
        class = "work.card.state_changed.bridged",
        via = via,
        card_id = %card_id,
        state = %state,
        "card-state transition published onto the internal bus"
    );
    if let Some((bus, registry)) = WORK_EVENT_BUS.get() {
        bus.publish(WORK_CARD_STATE_CHANGED, payload, registry)
            .await;
    }
}

/// Once-per-process sighting of a wire event id. Every resident persona's subscribe
/// stream yields the SAME room event once, so the bridge below would otherwise
/// publish N copies for N residents; first sighting wins. Bounded ring — old ids
/// age out, which is safe because duplicate deliveries arrive within the same
/// fan-out instant, not hours apart.
pub fn first_sighting(event_id: Uuid) -> bool {
    use std::collections::VecDeque;
    use std::sync::Mutex;
    // derived-or-floor: a dedup ring for duplicates that arrive within one fan-out instant, sized
    // far above any burst; an aged-out id only risks a duplicate publish, never a loss.
    const SEEN_CAP: usize = 256;
    static SEEN: OnceLock<Mutex<VecDeque<Uuid>>> = OnceLock::new();
    let seen = SEEN.get_or_init(|| Mutex::new(VecDeque::with_capacity(SEEN_CAP)));
    let mut seen = seen.lock().unwrap_or_else(|p| p.into_inner()); // poisoned lock = read the last state, same policy as every lock in this crate
    if seen.contains(&event_id) {
        return false;
    }
    if seen.len() >= SEEN_CAP {
        seen.pop_front();
    }
    seen.push_back(event_id);
    true
}

/// THE one emitter of [`WORK_CARD_STATE_CHANGED`] — bridge a card-state transition
/// heard on the WIRE onto the internal bus. The state change in the airc store is
/// the single source of truth, and its transcript echo is the single event source:
/// a citizen's own `work/state` (her echo arrives on her own subscribe stream), the
/// operator's `airc work state`, and a remote peer's transition all fire subscribers
/// (the grade-on-done handler) identically. Before this bridge, only the continuum
/// `work/state` VERB emitted — a card closed by any other writer changed the board
/// and graded nothing (found live 2026-08-15: Asha's real rle_roundtrip artifact,
/// operator close, zero grade).
pub async fn bridge_wire_work_event(event: &airc_core::TranscriptEvent, observer: Uuid) {
    // Review delivery is durable/idempotent in its existing storage owner. Do
    // not mark it seen before awaiting: cancellation must permit exact replay.
    crate::persona::training_producer::reviewed::bridge_review(event, observer).await;
    let Some(payload) = wire_card_state_payload(event) else {
        return;
    };
    if !first_sighting(event.event_id.as_uuid()) {
        return;
    }
    emit_card_state_changed(payload, "wire-echo").await;
}

