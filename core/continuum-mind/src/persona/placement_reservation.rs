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

use continuum_base::time::EpochMs;
use std::sync::{LazyLock, Mutex, MutexGuard};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::resources::throughput_class::{ResourceClass, TargetSilicon};
use crate::resources::throughput_lease::{
    ThroughputLease, ThroughputLeaseRegistry, ThroughputLeaseRevocationPolicy,
};

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
    let busy = crate::inference::resource_admission::inflight_model_calls()
        .saturating_add(crate::inference::resource_admission::leased_in_calls());
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
    let wait = crate::inference::resource_admission::leased_in_wait_p50_ms();
    let report = decide_grant(
        &mut ledger(),
        mind,
        requester,
        free_before_grants,
        wait,
        now_ms,
    );
    continuum_base::probe!(
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

/// THE PRODUCTION BOUNDARY for capability doctrine #3941 (send-back ca4758d2 on 8014ac25; cards d62743d6/8bd75362): a spill or return may ask seat `peer` for a slot ONLY when its beacon affirmatively advertises [`crate::capacity::gossip::CapabilityId::PlacementReservationGrant`] — a seat that never learned the field still beacons `free_slots_live` as lanes − residents, which overcounts what a second node would see; asking it rebuilds the 2026-09-18 double-spill the grant exists to close. No row at all is the same refusal: typed absence = safe (an old core's serde default is an empty list). Pure over the ledger, so the acceptance fixture exercises exactly this boundary with no wire.
pub fn reservation_capability_gate(
    ledger: &crate::capacity::gossip::GridCapacityLedger,
    peer: Uuid,
) -> crate::capacity::gossip::CapabilityDecision {
    let required = crate::capacity::gossip::CapabilityId::PlacementReservationGrant;
    match ledger
        .foreign_offers_with_age()
        .into_iter()
        .find(|(p, _, _)| *p == peer)
    {
        Some((_, offer, _)) => crate::capacity::gossip::capability_decision(&offer, required),
        None => crate::capacity::gossip::CapabilityDecision::Refuse { required }, // no beacon at all: absence is a refusal, not an invitation
    }
}

/// Why a reservation ask did not produce a grant — typed so callers can count
/// capability-vs-transport without parsing strings (card 8e4c42de). The pre-wire variants
/// send nothing and write no fault row; only [`ReservationAskError::Transport`] is a wire
/// failure, and it alone notes the seat's backoff inside [`request_reservation`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReservationAskError {
    /// `peer` is this node under one of its own ids: asking itself for a slot. No wire sent,
    /// no fault row — a canary after #4219 and the self-peer resolver.
    SelfNode,
    /// The seat's beacon does not affirmatively advertise the capability this ask requires
    /// (#3941): no wire sent, no fault row — a stable fact of the beacon, re-read each tick
    /// for one ledger scan.
    Capability {
        required: crate::capacity::gossip::CapabilityId,
    },
    /// The seat faulted an earlier ask and per-peer backoff is still in effect (not refreshed
    /// by this check). No new wire sent this tick; the existing fault row stays as-is.
    Backoff { remaining_ms: u64 },
    /// The ask was sent and failed to grant: no route, no answer within deadline,
    /// unparseable reply, or a command-level refusal. This variant alone writes the seat's
    /// fault row (inside [`request_reservation`]) and probes `placement.reserve.backoff`.
    Transport(String),
}

impl std::fmt::Display for ReservationAskError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SelfNode => write!(f, "a node never asks itself for a slot"),
            Self::Capability { required } => write!(
                f,
                "the seat does not advertise the capability this ask requires ({required:?}) — its slot arithmetic is lanes−residents, never asked"
            ),
            Self::Backoff { remaining_ms } => {
                write!(f, "the seat faulted an earlier ask — not asked again for {}s", remaining_ms / 1000)
            }
            Self::Transport(detail) => f.write_str(detail),
        }
    }
}

impl ReservationAskError {
    /// The machine-countable class of this refusal (card 8e4c42de): the callers' probes carry it as a field so downstream counts capability-vs-transport without parsing reason text.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::SelfNode => "self_node",
            Self::Capability { .. } => "capability",
            Self::Backoff { .. } => "backoff",
            Self::Transport(_) => "transport",
        }
    }
}

/// THE SPILLER'S HALF: ask `peer` for a slot for `mind` over airc and wait for its word,
/// bounded by [`RESERVE_DEADLINE`]. Pre-wire refusals (self-node, capability absence,
/// backoff) arrive as their own typed variants — none of them sends or writes a fault row;
/// only a failed wire round-trip is [`ReservationAskError::Transport`], which notes the
/// seat's backoff.
pub async fn request_reservation(
    airc: &std::sync::Arc<airc_lib::Airc>,
    peer: Uuid,
    mind: Uuid,
) -> Result<PlacementReservationReport, ReservationAskError> {
    let now = EpochMs::now().millis();
    reservation_preflight(peer, now)?;
    let asked = send_reservation_ask(airc, peer, mind).await;
    if let Err(fault) = &asked {
        {
            let mut faults = ASK_BACKOFF.lock().unwrap_or_else(|e| e.into_inner()); // unwrap_or_else: same map, same reason — a fault row written into a poisoned map still expires by time
            note_ask_fault(&mut faults, peer, now);
        }
        continuum_base::probe!(
            class = "placement.reserve.backoff",
            peer = %peer,
            mind = %mind,
            fault = %fault,
            backoff_ms = ASK_BACKOFF_MS,
            "a seat that faulted the ask (policy, transport, deadline) is not asked again by ANY mind until the backoff passes"
        );
    }
    asked.map_err(ReservationAskError::Transport)
}

/// The ask's pre-wire decision — self-node, capability (#3941), backoff, in that order,
/// before anything touches the wire or a fault row is written. Pure over process state and
/// ledgers so it tests without an airc handle; [`request_reservation`] runs it first and
/// only sends on Ok (card 8e4c42de).
pub fn reservation_preflight(peer: Uuid, now_ms: u64) -> Result<(), ReservationAskError> {
    if crate::persona::self_peer::is_this_node(peer) {
        return Err(ReservationAskError::SelfNode);
    }
    // THE capability gate (#3941): ask ONLY a seat whose beacon affirmatively advertises the grant. A refusal here sends NOTHING over the wire — no ACL evaluation at the seat, no deadline wait — and writes NO fault row: it is a stable fact of the seat's beacon (which may refresh with new capabilities next beat), not a transport or policy fault; re-reading it each tick costs one ledger scan.
    if let crate::capacity::gossip::CapabilityDecision::Refuse { required } =
        reservation_capability_gate(&crate::capacity::gossip::global_ledger(), peer)
    {
        return Err(ReservationAskError::Capability { required });
    }
    // The guard lives in its own block: scope the lock to the read, nothing held past it.
    let remaining = {
        let faults = ASK_BACKOFF.lock().unwrap_or_else(|e| e.into_inner()); // unwrap_or_else: a poisoned backoff map is still a map of peer → until_ms; the worst a panicked holder leaves is one stale row, which expires by time
        ask_backoff_remaining(&faults, peer, now_ms)
    };
    if let Some(left_ms) = remaining {
        return Err(ReservationAskError::Backoff {
            remaining_ms: left_ms,
        });
    }
    Ok(())
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
pub fn ask_backoff_remaining(
    faults: &std::collections::HashMap<Uuid, u64>,
    peer: Uuid,
    now_ms: u64,
) -> Option<u64> {
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
    let envelope = AircCommandRequest::new(
        RESERVE_PATH.to_string(),
        KIND_PEER.to_string(),
        None,
        params,
    );
    let body = Body::Json(
        serde_json::to_value(&envelope).map_err(|e| format!("serialize reserve envelope: {e}"))?, // airc wire: encode/decode the remote reservation envelope.
    ); // airc wire: the ask leaves this process for the seat's core over the mesh
    let headers = crate::routing::airc_transport::AircTransport::build_headers(&envelope);
    let pending = airc
        .request(
            MentionTarget::Peer(PeerId(peer)),
            headers,
            body,
            RESERVE_DEADLINE,
        )
        .await
        .map_err(|e| format!("reserve ask to {peer} not routed: {e}"))?;
    let reply = airc.await_reply(pending).await.map_err(|e| {
        format!(
            "reserve ask to {peer} unanswered within {}s: {e}",
            RESERVE_DEADLINE.as_secs()
        )
    })?;
    let value = match reply.body().clone() {
        Body::Json(v) => v,
        Body::Binary(_) => return Err("seat replied with binary; expected json".to_string()),
    };
    let response: AircCommandResponse =
        serde_json::from_value(value).map_err(|e| format!("decode reserve reply: {e}"))?; // airc wire: the seat's reply arrives as the mesh's JSON body
    let result = response
        .into_result()
        .map_err(|e| format!("seat refused the ask: {e}"))?;
    serde_json::from_value(result).map_err(|e| format!("decode reservation report: {e}"))
    // airc wire: encode/decode the remote reservation envelope.
    // airc wire: the report inside the seat's command reply
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
        assert_eq!(
            ask_backoff_remaining(&faults, seat, NOW),
            None,
            "no fault: ask freely"
        );
        note_ask_fault(&mut faults, seat, NOW);
        assert_eq!(
            ask_backoff_remaining(&faults, seat, NOW + 1),
            Some(ASK_BACKOFF_MS - 1)
        );
        assert_eq!(
            ask_backoff_remaining(&faults, other, NOW + 1),
            None,
            "keyed on the seat that faulted, not on all seats"
        );
        assert_eq!(
            ask_backoff_remaining(&faults, seat, NOW + ASK_BACKOFF_MS),
            None,
            "expires by time — the mirror of the fault"
        );
        assert_eq!(
            faults.len(),
            1,
            "five minds asking one faulted seat share ONE row"
        );
    }

    #[test]
    fn two_nodes_over_by_n_against_one_seat_with_two_free_slots_spill_at_most_two() {
        let mut reg = ThroughputLeaseRegistry::new();
        let (node_a, node_b) = (Uuid::from_u128(0xa), Uuid::from_u128(0xb));
        let a_minds = [Uuid::from_u128(0xa1), Uuid::from_u128(0xa2)];
        let b_minds = [Uuid::from_u128(0xb1), Uuid::from_u128(0xb2)];
        let free = 2;
        let granted = |r: &PlacementReservationReport| r.granted;
        let a: Vec<_> = a_minds
            .iter()
            .map(|m| decide_grant(&mut reg, *m, node_a, free, (0, 0), NOW))
            .collect();
        let b: Vec<_> = b_minds
            .iter()
            .map(|m| decide_grant(&mut reg, *m, node_b, free, (0, 0), NOW + 5))
            .collect();
        assert_eq!(
            a.iter().filter(|r| granted(r)).count(),
            2,
            "first asker gets the seat's two: {a:?}"
        );
        assert_eq!(
            b.iter().filter(|r| granted(r)).count(),
            0,
            "second asker is told the truth: {b:?}"
        );
        assert_eq!(
            a[1].free_slots_live, 0,
            "after the second grant the seat has nothing left"
        );
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
            .map(|i| {
                decide_grant(
                    &mut reg,
                    Uuid::from_u128(0x700 + i),
                    node,
                    2,
                    (150_000, 40),
                    NOW,
                )
                .granted
            })
            .collect();
        assert_eq!(
            grants.iter().filter(|g| **g).count(),
            2,
            "two slots, two returns: {grants:?}"
        );
        assert!(
            grants[..2].iter().all(|g| *g) && grants[2..].iter().all(|g| !*g),
            "first come, first granted; the rest wait"
        );
        // A spill from another node now finds nothing either — one ledger for both paths.
        assert!(
            !decide_grant(
                &mut reg,
                Uuid::from_u128(0xb1),
                Uuid::from_u128(0xb),
                2,
                (150_000, 40),
                NOW + 1
            )
            .granted
        );
    }

    // what this catches: the grant carries the seat's measured wait WITH its sample count,
    // so a refusal or a grant from a seat that has never served leased-in work reads as
    // unmeasured (0, 0) — the absence-read-as-a-value guard, kept at the grant too.
    #[test]
    fn a_grant_carries_the_seats_wait_with_its_count_and_free_never_goes_negative() {
        let mut reg = ThroughputLeaseRegistry::new();
        let r = decide_grant(
            &mut reg,
            Uuid::from_u128(1),
            Uuid::from_u128(9),
            0,
            (0, 0),
            NOW,
        );
        assert!(!r.granted, "{r:?}");
        assert_eq!((r.free_slots_live, r.outstanding), (0, 0));
        assert_eq!((r.lane_wait_p50_ms, r.lane_wait_samples), (0, 0));
        let r = decide_grant(
            &mut reg,
            Uuid::from_u128(2),
            Uuid::from_u128(9),
            1,
            (216_000, 40),
            NOW,
        );
        assert!(r.granted, "{r:?}");
        assert_eq!(
            (r.lane_wait_p50_ms, r.lane_wait_samples),
            (216_000, 40),
            "the wait rides the grant"
        );
        assert_eq!(r.free_slots_live, 0);
    }
    // what this catches (send-back ca4758d2 on 8014ac25 — THE production acceptance line): a spill or return asks a seat for a slot ONLY when its beacon AFFIRMATIVELY advertises PlacementReservationGrant. Missing row, pre-capability wire shape (no key at all), and an Unknown-only advertisement all refuse; exact presence allows; a different id does not stand in. The gate sits before send_reservation_ask in request_reservation, so a refusal never reaches the wire — no ACL evaluation at the seat, no deadline wait — and writes no fault row: it is a stable fact of the beacon, re-read each tick for one ledger scan.
    #[test]
    fn the_seat_is_never_asked_unless_its_beacon_affirmatively_advertises_the_grant(
    ) -> Result<(), serde_json::Error> {
        use crate::capacity::gossip::{CapabilityDecision, CapabilityId, GridCapacityLedger};

        let peer = Uuid::from_u128(0x5e4a7c3b9d21); // a foreign seat — never this node
        let ledger = GridCapacityLedger::default();

        fn beacon(
            caps: Option<serde_json::Value>,
        ) -> Result<crate::capacity::gossip::CapacityOffer, serde_json::Error> {
            let mut j = serde_json::json!({
                "gpuTotalBytes": 1u64,
                "gpuFreeBytesLive": 0u64,
                "systemRamFreeBytes": 0u64,
                "atMs": NOW,
            });
            if let Some(c) = caps {
                j["capabilities"] = c;
            }
            serde_json::from_value(j) // wire shape of a foreign core's beacon — parse failure is the test's own error
        }

        // (a) No row at all — the seat never beaconed: typed absence is a refusal, not an invitation.
        assert_eq!(
            reservation_capability_gate(&ledger, peer),
            CapabilityDecision::Refuse {
                required: CapabilityId::PlacementReservationGrant
            },
            "no beacon, no ask"
        );

        // (b) Pre-capability wire shape — NO "capabilities" key at all → serde default empty.
        ledger.hear(peer, beacon(None)?, NOW);
        assert_eq!(
            reservation_capability_gate(&ledger, peer),
            CapabilityDecision::Refuse {
                required: CapabilityId::PlacementReservationGrant
            },
            "an older core's beacon still parses and still refuses"
        );

        // (c) Unknown-only advertisement — a name this build does not know folds to the parse-only
        // Unknown, which never grants.
        ledger.hear(
            peer,
            beacon(Some(serde_json::json!(["some_future_capability"])))?,
            NOW,
        );
        assert_eq!(
            reservation_capability_gate(&ledger, peer),
            CapabilityDecision::Refuse {
                required: CapabilityId::PlacementReservationGrant
            },
            "the parse-only fold cannot stand in for the grant"
        );

        // (d) A different capability does not stand in — exact-name membership only.
        ledger.hear(
            peer,
            beacon(Some(serde_json::json!([
                "served_window_stamp",
                "lane_wait_reporting"
            ])))?,
            NOW,
        );
        assert_eq!(
            reservation_capability_gate(&ledger, peer),
            CapabilityDecision::Refuse {
                required: CapabilityId::PlacementReservationGrant
            },
            "exact-name membership, not 'has any capability'"
        );

        // (e) Exact presence — the seat affirmatively advertises the grant → allow.
        ledger.hear(
            peer,
            beacon(Some(serde_json::json!([
                "placement_reservation_grant",
                "some_future_capability"
            ])))?,
            NOW,
        );
        assert_eq!(
            reservation_capability_gate(&ledger, peer),
            CapabilityDecision::Allow,
            "affirmative advertisement admits the ask"
        );

        Ok(())
    }

    // what this catches (card 8e4c42de, the title): callers' probes carry `kind` as a field so
    // downstream counts capability-vs-transport without parsing reason text. The four classes
    // are exactly these strings — a rename or merge here silently corrupts every fleet count
    // that leans on it; and each class still renders human-readable text, because the refusal
    // is logged too.
    #[test]
    fn each_refusal_carries_its_own_machine_countable_kind() {
        use crate::capacity::gossip::CapabilityId;

        let errs = [
            ReservationAskError::SelfNode,
            ReservationAskError::Capability {
                required: CapabilityId::PlacementReservationGrant,
            },
            ReservationAskError::Backoff {
                remaining_ms: 1_500,
            },
            ReservationAskError::Transport("reserve ask to the seat not routed".to_string()),
        ];
        let kinds: Vec<&'static str> = errs.iter().map(|e| e.kind()).collect();
        assert_eq!(
            kinds,
            vec!["self_node", "capability", "backoff", "transport"]
        );
        assert!(errs[1].to_string().contains("capability"), "{}", errs[1]);
        assert_eq!(
            errs[3].to_string(),
            "reserve ask to the seat not routed",
            "the transport text IS the wire detail"
        );
        assert!(
            errs[2].to_string().contains("not asked again"),
            "{}",
            errs[2]
        );
    }

    // what this catches (card 8e4c42de + #4219): preflight refuses BEFORE anything touches the
    // wire, in a fixed order — self-node first (a node never asks itself for a slot under any
    // of its own ids; the canary after M5's loopback spill), then capability absence. Neither
    // refusal sends or writes a fault row; both are re-read each tick for one ledger scan.
    #[test]
    fn preflight_refuses_itself_and_the_unadvertised_seat_before_anything_touches_the_wire() {
        use crate::capacity::gossip::CapabilityId;

        let own = Uuid::new_v4();
        crate::persona::self_peer::register(own); // every id this process wears is "me", including one registered just now
        assert_eq!(
            reservation_preflight(own, NOW),
            Err(ReservationAskError::SelfNode),
            "a node never asks itself for a slot — under any of its ids"
        );

        let stranger = Uuid::new_v4(); // never worn by this node; no beacon in the global ledger either
        assert_eq!(
            reservation_preflight(stranger, NOW),
            Err(ReservationAskError::Capability { required: CapabilityId::PlacementReservationGrant }),
            "a seat with no affirmative advertisement is refused before the wire — absence is a refusal"
        );
    }
}
