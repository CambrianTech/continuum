//! A slot on a seat is a LEASE the seat grants — not a beacon field the spiller subtracts.
//!
//! Before this (card c84d885a, S1b): `choose_offloads` read `lanes − residents` off a
//! beacon ONCE, at bind time, on every node independently. The M5 and the IntelMac each
//! read the 5090 as "2 free" in the same minute and both spilled onto it; the seat then
//! carried every node's overflow on one slot for hours (lane wait p50 216 s, p90 1,387 s,
//! max 7,058 s) and no receipt said why. Cormac's rule: *the responder knows its queue;
//! the requester does not* — so `free` must be a reservation the responder grants and
//! can revoke.
//!
//! Two halves, one file:
//! - **the seat** ([`decide_grant`], [`grant`], [`free_slots_live_now`]): a process-wide
//!   [`ThroughputLeaseRegistry`] of granted slots, TTL'd to the placement cadence bound
//!   ([`RESERVATION_TTL_MS`]) and revocable (Graceful). The beacon's `free_slots_live`
//!   subtracts outstanding grants, so the SECOND node to ask sees the truth the first
//!   node's grant made. A spiller's traffic then shows in the seat's own in-flight gauges
//!   and the plan grows lanes for it (S2, #4197) — the reservation covers the bind window,
//!   not her residency.
//! - **the spiller** ([`request_reservation`]): asks the seat over airc
//!   (`persona/placement/reserve`, the same envelope and reply path as a leased generate)
//!   BEFORE `go_remote` binds her. Refused, or unanswered within the seat's own freshness
//!   window, means she stays home — receipted, never silent.
//!
//! No new primitive: the lease registry, the airc command envelope, and the admission
//! gauges already existed; this composes them into the grant.

use std::sync::{LazyLock, Mutex, MutexGuard};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::cognition::throughput_lease::{
    ThroughputLease, ThroughputLeaseRegistry, ThroughputLeaseRevocationPolicy,
};
use crate::cognition::{ResourceClass, TargetSilicon};

/// How long a granted slot stays reserved for a mind that has not arrived yet. The
/// placement cadence bound: a spiller that could not bind within one move cooldown is
/// not coming, and the slot returns to the beacon.
pub const RESERVATION_TTL_MS: u64 = super::placement_switch::MOVE_COOLDOWN_MS;

/// The wire verb a spiller asks the seat with.
pub const RESERVE_PATH: &str = "persona/placement/reserve";

/// How long a spiller waits for the seat's answer: the seat's own freshness window. A
/// seat that cannot grant a slot inside the time its beacon is trusted is not a seat to
/// bind to this tick.
pub const RESERVE_DEADLINE: std::time::Duration =
    std::time::Duration::from_millis(super::placement_switch::REMOTE_SEAT_FRESH_MS);

/// The seat's answer to a reservation ask — its OWN live truth at the moment of the
/// decision, granted or not. Every field is what the beacon would have said a beacon
/// period later; the ask just gets it now, atomically with the grant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/PlacementReservationReport.ts"
)]
pub struct PlacementReservationReport {
    /// A slot is held for the mind until `until_ms`.
    pub granted: bool,
    /// `granted` / `renewed` / why not.
    pub reason: String,
    /// Slots the seat can still grant AFTER this decision.
    pub free_slots_live: u32,
    /// Grants outstanding (unexpired) AFTER this decision.
    pub outstanding: u32,
    /// The seat's own median receipt-to-first-progress wait for leased-in work, ms.
    #[ts(type = "number")]
    pub lane_wait_p50_ms: u64,
    /// Samples behind that median; 0 = UNMEASURED, never "fast".
    pub lane_wait_samples: u32,
    /// Unix ms the reservation lapses (0 when refused).
    #[ts(type = "number")]
    pub until_ms: u64,
}

static RESERVATIONS: LazyLock<Mutex<ThroughputLeaseRegistry>> =
    LazyLock::new(|| Mutex::new(ThroughputLeaseRegistry::new()));

fn ledger() -> MutexGuard<'static, ThroughputLeaseRegistry> {
    RESERVATIONS.lock().unwrap_or_else(|p| p.into_inner()) // unwrap_or_else: a poisoned ledger still holds every grant; the next expire() pass re-derives the truth
}

fn lease_id_of(mind: Uuid) -> String {
    format!("placement:{mind}")
}

/// Slots this seat has in flight right now, every source: its own residents' turns and
/// the generates it serves for other nodes.
fn busy_slots() -> u32 {
    let busy = crate::cognition::resource_admission::inflight_model_calls()
        .saturating_add(crate::cognition::resource_admission::leased_in_calls());
    busy.min(u32::MAX as usize) as u32
}

/// Grants outstanding (unexpired) at `now_ms`, expiring lapsed ones on the way.
pub fn outstanding(now_ms: u64) -> u32 {
    let mut reg = ledger();
    reg.expire(now_ms);
    reg.snapshot(now_ms).active.len().min(u32::MAX as usize) as u32
}

/// THE ONE NUMBER the beacon and the grant share: served lanes − in flight (all sources)
/// − outstanding grants. The seat's own admission truth, minus what it has already
/// promised.
pub fn free_slots_live_now(lanes: u32, now_ms: u64) -> u32 {
    lanes
        .saturating_sub(busy_slots())
        .saturating_sub(outstanding(now_ms))
}

/// PURE over a ledger: grant `mind` one of `free_before_grants` slots (the seat's live
/// free count BEFORE subtracting outstanding grants). A mind that already holds a grant is
/// renewed, never double-counted (an idempotent re-ask across a tick boundary). Refused
/// when every free slot is already granted.
pub fn decide_grant(
    reg: &mut ThroughputLeaseRegistry,
    mind: Uuid,
    requester: Uuid,
    free_before_grants: u32,
    wait: (u64, u32),
    now_ms: u64,
) -> PlacementReservationReport {
    reg.expire(now_ms);
    let lease_id = lease_id_of(mind);
    let until_ms = now_ms.saturating_add(RESERVATION_TTL_MS);
    let report = |granted: bool, reason: &str, reg: &ThroughputLeaseRegistry| {
        let outstanding = reg.snapshot(now_ms).active.len().min(u32::MAX as usize) as u32;
        PlacementReservationReport {
            granted,
            reason: reason.to_string(),
            free_slots_live: free_before_grants.saturating_sub(outstanding),
            outstanding,
            lane_wait_p50_ms: wait.0,
            lane_wait_samples: wait.1,
            until_ms: if granted { until_ms } else { 0 },
        }
    };
    if reg.renew(&lease_id, until_ms, now_ms).is_ok() {
        return report(true, "renewed", reg);
    }
    let held = reg.snapshot(now_ms).active.len().min(u32::MAX as usize) as u32;
    if free_before_grants <= held {
        return report(
            false,
            &format!("no free slot: {free_before_grants} free, {held} already granted"),
            reg,
        );
    }
    let lease = ThroughputLease {
        lease_id,
        artifact_key: "serving-slot".to_string(),
        // The slot is a served lane whichever silicon backs it; the registry does not
        // budget on the class here — the count is the budget.
        resource_class: ResourceClass::LocalGeneration,
        target_silicon: TargetSilicon::Gpu,
        holder_id: requester.to_string(),
        cost_units: 1,
        acquired_at_ms: now_ms,
        expires_at_ms: until_ms,
        revocation_policy: ThroughputLeaseRevocationPolicy::Graceful,
    };
    match reg.acquire(lease, now_ms) {
        Ok(()) => report(true, "granted", reg),
        Err(e) => report(false, &format!("ledger refused: {e:?}"), reg),
    }
}

/// The seat's grant against ITS live numbers — what `persona/placement/reserve` runs.
pub fn grant(mind: Uuid, requester: Uuid, now_ms: u64) -> PlacementReservationReport {
    let lanes = crate::inference::llama_server::current_serving().lanes;
    let free_before_grants = lanes.saturating_sub(busy_slots());
    let wait = crate::cognition::resource_admission::leased_in_wait_p50_ms();
    let report = decide_grant(&mut ledger(), mind, requester, free_before_grants, wait, now_ms);
    crate::probe!(
        class = "placement.reserve.decided",
        mind = %mind,
        requester = %requester,
        granted = report.granted,
        reason = %report.reason,
        lanes = lanes,
        free_after = report.free_slots_live,
        outstanding = report.outstanding,
        lane_wait_p50_ms = report.lane_wait_p50_ms,
        lane_wait_samples = report.lane_wait_samples,
        "a spiller asked this seat for a slot"
    );
    report
}

/// Release a mind's grant early (she left, or never bound). Missing = already lapsed.
pub fn release(mind: Uuid) {
    let _ = ledger().release(&lease_id_of(mind));
}

/// THE SPILLER'S HALF: ask `peer` for a slot for `mind` over airc and wait for its word,
/// bounded by [`RESERVE_DEADLINE`]. `Err` = no answer or no route: not a seat this tick.
pub async fn request_reservation(
    airc: &std::sync::Arc<airc_lib::Airc>,
    peer: Uuid,
    mind: Uuid,
) -> Result<PlacementReservationReport, String> {
    let now = crate::persona::trace::now_ms();
    let faults = ASK_BACKOFF.lock().unwrap_or_else(|e| e.into_inner()); // unwrap_or_else: a poisoned backoff map is still a map of peer → until_ms; the worst a panicked holder leaves is one stale row, which expires by time
    let remaining = ask_backoff_remaining(&faults, peer, now);
    drop(faults);
    if let Some(left_ms) = remaining {
        return Err(format!("seat {peer} faulted an ask {}s ago — not asked again for {}s", (ASK_BACKOFF_MS.saturating_sub(left_ms)) / 1000, left_ms / 1000));
    }
    let asked = send_reservation_ask(airc, peer, mind).await;
    if let Err(fault) = &asked {
        note_ask_fault(&mut ASK_BACKOFF.lock().unwrap_or_else(|e| e.into_inner()), peer, now); // unwrap_or_else: same map, same reason — a fault row written into a poisoned map still expires by time
        crate::probe!(
            class = "placement.reserve.backoff",
            peer = %peer,
            mind = %mind,
            fault = %fault,
            backoff_ms = ASK_BACKOFF_MS,
            "a seat that faulted the ask (policy, transport, deadline) is not asked again by ANY mind until the backoff passes"
        );
    }
    asked
}

/// A seat is asked again only after this long once an ask FAULTED — refused by policy,
/// not routed, or unanswered within [`RESERVE_DEADLINE`]. A fault is a property of the
/// seat, not of the mind that asked ([[a-per-peer-fault-counted-per-persona-never-trips-the-breaker]]),
/// so the backoff is keyed by peer: measured 2026-09-19 04:26–04:50Z, five M5 minds re-asked
/// the 5090 every tick through a policy refusal — ~6 asks/min into the seat everyone queues
/// on, each an ACL evaluation there and a 30 s deadline wait here when the seat was silent.
/// A capacity refusal (`granted: false`) is the seat's honest "not this tick" and is NOT a
/// fault — it re-asks next tick, since the seat's ledger changes every turn.
pub const ASK_BACKOFF_MS: u64 = super::placement_switch::MOVE_COOLDOWN_MS;

static ASK_BACKOFF: LazyLock<Mutex<std::collections::HashMap<Uuid, u64>>> =
    LazyLock::new(|| Mutex::new(std::collections::HashMap::new()));

/// Milliseconds still to wait before `peer` may be asked again, if a fault is in force.
pub fn ask_backoff_remaining(faults: &std::collections::HashMap<Uuid, u64>, peer: Uuid, now_ms: u64) -> Option<u64> {
    let until = *faults.get(&peer)?;
    (until > now_ms).then(|| until - now_ms)
}

/// Record that `peer` faulted an ask at `now_ms`: no mind asks it again for [`ASK_BACKOFF_MS`].
pub fn note_ask_fault(faults: &mut std::collections::HashMap<Uuid, u64>, peer: Uuid, now_ms: u64) {
    faults.insert(peer, now_ms.saturating_add(ASK_BACKOFF_MS));
}

async fn send_reservation_ask(
    airc: &std::sync::Arc<airc_lib::Airc>,
    peer: Uuid,
    mind: Uuid,
) -> Result<PlacementReservationReport, String> {
    use airc_core::{Body, MentionTarget, PeerId};
    use continuum_airc_protocol::{AircCommandRequest, AircCommandResponse, KIND_PEER};

    let requester = airc.peer_id().0;
    let params = serde_json::json!({ "mind": mind, "requester": requester });
    let envelope = AircCommandRequest::new(RESERVE_PATH.to_string(), KIND_PEER.to_string(), None, params);
    let body = Body::Json(serde_json::to_value(&envelope).map_err(|e| format!("serialize reserve envelope: {e}"))?); // airc wire: the ask leaves this process for the seat's core over the mesh
    let headers = crate::routing::airc_transport::AircTransport::build_headers(&envelope);
    let room = airc.current_room().await.map_err(|e| format!("no room to ask in: {e}"))?;
    let pending = airc
        .request_in(&room, MentionTarget::Peer(PeerId(peer)), headers, body, RESERVE_DEADLINE)
        .await
        .map_err(|e| format!("reserve ask to {peer} not routed: {e}"))?;
    let reply = airc
        .await_reply(pending)
        .await
        .map_err(|e| format!("reserve ask to {peer} unanswered within {}s: {e}", RESERVE_DEADLINE.as_secs()))?;
    let value = match reply.body {
        Some(Body::Json(v)) => v,
        Some(Body::Binary(_)) => return Err("seat replied with binary; expected json".to_string()),
        None => return Err("seat replied with no body".to_string()),
    };
    let response: AircCommandResponse = serde_json::from_value(value).map_err(|e| format!("decode reserve reply: {e}"))?; // airc wire: the seat's reply arrives as the mesh's JSON body
    let result = response.into_result().map_err(|e| format!("seat refused the ask: {e}"))?;
    serde_json::from_value(result).map_err(|e| format!("decode reservation report: {e}")) // airc wire: the report inside the seat's command reply
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_700_000_000_000;

    // what this catches (card c84d885a, the title): two nodes each read one beacon saying
    // "2 free" and each choose 2 minds. Without a grant the seat takes 4 on 2 slots. With
    // the grant the seat says yes to the first two asks and no to the rest — total spill
    // ≤ the seat's free slots, whatever the spillers believed. A lapsed grant frees the
    // slot; a re-ask for the same mind renews, never double-counts.
    // what this catches (2026-09-19, ~6 asks/min into the 5090 through a policy refusal):
    // a seat that FAULTED an ask is a fact about the seat — every mind on this node holds
    // off it for the backoff, one fault row per peer, and the row expires by time (its own
    // mirror). A second seat is unaffected.
    #[test]
    fn a_seat_that_faulted_an_ask_is_not_asked_again_by_any_mind_until_the_backoff_passes() {
        let (seat, other) = (Uuid::new_v4(), Uuid::new_v4());
        let mut faults = std::collections::HashMap::new();
        assert_eq!(ask_backoff_remaining(&faults, seat, NOW), None, "no fault: ask freely");
        note_ask_fault(&mut faults, seat, NOW);
        assert_eq!(ask_backoff_remaining(&faults, seat, NOW + 1), Some(ASK_BACKOFF_MS - 1));
        assert_eq!(ask_backoff_remaining(&faults, other, NOW + 1), None, "keyed on the seat that faulted, not on all seats");
        assert_eq!(ask_backoff_remaining(&faults, seat, NOW + ASK_BACKOFF_MS), None, "expires by time — the mirror of the fault");
        assert_eq!(faults.len(), 1, "five minds asking one faulted seat share ONE row");
    }

    #[test]
    fn two_nodes_over_by_n_against_one_seat_with_two_free_slots_spill_at_most_two() {
        let mut reg = ThroughputLeaseRegistry::new();
        let (node_a, node_b) = (Uuid::from_u128(0xa), Uuid::from_u128(0xb));
        let a_minds = [Uuid::from_u128(0xa1), Uuid::from_u128(0xa2)];
        let b_minds = [Uuid::from_u128(0xb1), Uuid::from_u128(0xb2)];
        let free = 2;
        let granted = |r: &PlacementReservationReport| r.granted;
        let a: Vec<_> = a_minds.iter().map(|m| decide_grant(&mut reg, *m, node_a, free, (0, 0), NOW)).collect();
        let b: Vec<_> = b_minds.iter().map(|m| decide_grant(&mut reg, *m, node_b, free, (0, 0), NOW + 5)).collect();
        assert_eq!(a.iter().filter(|r| granted(r)).count(), 2, "first asker gets the seat's two: {a:?}");
        assert_eq!(b.iter().filter(|r| granted(r)).count(), 0, "second asker is told the truth: {b:?}");
        assert_eq!(a[1].free_slots_live, 0, "after the second grant the seat has nothing left");
        assert_eq!(b[0].outstanding, 2);
        assert!(b[0].reason.starts_with("no free slot"), "{}", b[0].reason);
        assert_eq!(b[0].until_ms, 0, "a refusal holds nothing");

        // The same mind asking again is a renewal, not a third slot.
        let again = decide_grant(&mut reg, a_minds[0], node_a, free, (0, 0), NOW + 10);
        assert!(again.granted && again.reason == "renewed", "{again:?}");
        assert_eq!(again.outstanding, 2);

        // Past the TTL the grants lapse and the seat can say yes again.
        let later = NOW + 10 + RESERVATION_TTL_MS + 1;
        let c = decide_grant(&mut reg, b_minds[0], node_b, free, (0, 0), later);
        assert!(c.granted && c.reason == "granted", "{c:?}");
        assert_eq!(c.outstanding, 1);
        assert_eq!(c.until_ms, later + RESERVATION_TTL_MS);
    }

    // what this catches (2026-09-19, the return storm): seven minds that fell home from one
    // seat all qualify to RETURN the tick its wait dips under the bound; the seat grants
    // only what it has, so a 2-slot seat takes two back and the other five stay home until
    // its beacon shows room — the same ledger a spill asks, so a return can never take a
    // slot a spill was granted. The seat's own arithmetic bounds the fleet's swing.
    #[test]
    fn seven_returning_minds_take_only_the_seats_free_slots_and_the_rest_stay_home() {
        let mut reg = ThroughputLeaseRegistry::new();
        let node = Uuid::from_u128(0xa);
        let grants: Vec<bool> = (1..=7u128)
            .map(|i| decide_grant(&mut reg, Uuid::from_u128(0x700 + i), node, 2, (150_000, 40), NOW).granted)
            .collect();
        assert_eq!(grants.iter().filter(|g| **g).count(), 2, "two slots, two returns: {grants:?}");
        assert!(grants[..2].iter().all(|g| *g) && grants[2..].iter().all(|g| !*g), "first come, first granted; the rest wait");
        // A spill from another node now finds nothing either — one ledger for both paths.
        assert!(!decide_grant(&mut reg, Uuid::from_u128(0xb1), Uuid::from_u128(0xb), 2, (150_000, 40), NOW + 1).granted);
    }

    // what this catches: the grant carries the seat's measured wait WITH its sample count,
    // so a refusal or a grant from a seat that has never served leased-in work reads as
    // unmeasured (0, 0) — the absence-read-as-a-value guard, kept at the grant too.
    #[test]
    fn a_grant_carries_the_seats_wait_with_its_count_and_free_never_goes_negative() {
        let mut reg = ThroughputLeaseRegistry::new();
        let r = decide_grant(&mut reg, Uuid::from_u128(1), Uuid::from_u128(9), 0, (0, 0), NOW);
        assert!(!r.granted, "{r:?}");
        assert_eq!((r.free_slots_live, r.outstanding), (0, 0));
        assert_eq!((r.lane_wait_p50_ms, r.lane_wait_samples), (0, 0));
        let r = decide_grant(&mut reg, Uuid::from_u128(2), Uuid::from_u128(9), 1, (216_000, 40), NOW);
        assert!(r.granted, "{r:?}");
        assert_eq!((r.lane_wait_p50_ms, r.lane_wait_samples), (216_000, 40), "the wait rides the grant");
        assert_eq!(r.free_slots_live, 0);
    }
}
