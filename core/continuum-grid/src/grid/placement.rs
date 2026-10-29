//! Beacon-driven recovery and offload orchestration.
use crate::persona::placement_switch::{
    choose_offloads, decide, moves_between_machines, seat_starves, switches, LocalShape,
    PeerOffer, PlacementInputs, PlacementMove, PlacementSwitch, Seat, SeatFit,
    MOVE_COOLDOWN_MS, REMOTE_SEAT_FRESH_MS,
};
use std::sync::Arc;
use uuid::Uuid;

mod opportunity;
use opportunity::follow_the_allocation;

/// One pass: read beacon ages, decide per switch (fall home / return), then offload a
/// starved node's tail minds onto peers' free lanes. Returns the org-room lines. Called
/// from the grid module's tick (60 s) — no task of its own.
pub async fn follow_the_fleet(
    now_ms: u64,
    peers: Vec<PeerOffer>,
    local: LocalShape,
    rank_of: &(dyn Fn(&str) -> Option<u8> + Sync),
) -> Vec<String> {
    let heard: std::collections::HashMap<Uuid, u64> =
        peers.iter().map(|p| (p.peer, p.beacon_age_ms)).collect();
    let offers: std::collections::HashMap<Uuid, &PeerOffer> = peers.iter().map(|p| (p.peer, p)).collect();
    let home_wait_p50_ms = {
        let (p50, samples) = crate::inference::resource_admission::local_lane_wait_p50_ms();
        (samples > 0).then_some(p50)
    };
    let mut lines = Vec::new();
    let mut at_home: Vec<(Uuid, u64)> = Vec::new();
    // What a turn needs of a lane, read once per pass (Joel: "find something once and
    // pass it along"). HERS, and only hers — the node's typical used to stand in when she
    // had sent nothing, which is the line this fix removed; see `requirement` below.
    let working_set = crate::cognition::working_set::global();
    // Home-bound minds whose seat is fresh and serving but offers no slot this tick —
    // one line per seat per tick, not one refused ask per mind per tick.
    let mut withheld: std::collections::HashMap<Uuid, u32> = std::collections::HashMap::new();
    for sw in switches() {
        let cooled = now_ms.saturating_sub(sw.moved_at_ms()) >= MOVE_COOLDOWN_MS;
        let Some(peer) = sw.peer() else {
            // Home-born, never bound: a candidate for an offload when eligible.
            if cooled {
                at_home.push((sw.persona_id(), crate::cognition::health_ledger::lane_grants_of(sw.persona_id())));
            }
            continue;
        };
        let seat_is_self = crate::persona::self_peer::is_this_node(peer);
        let inputs = PlacementInputs {
            seat: sw.seat(),
            beacon_age_ms: heard.get(&peer).copied(),
            lane_cold: sw.lane_cold(),
            local_available: match sw.seat() {
                Seat::Remote => sw.local_available().await,
                Seat::Home => true,
            },
            since_last_move_ms: now_ms.saturating_sub(sw.moved_at_ms()),
            // The peer's summed service evidence — the return gate's mirror of
            // `lane_cold` (card ad96f5d1). breaker_for creates a pristine (served_ok =
            // true) entry for a peer never seen, so a new seat is not self-sealed.
            peer_served_ok: crate::inference::airc_remote::peer_breaker::breaker_for(
                &peer.to_string(),
            )
            .served_ok(),
            seat_wait_p50_ms: offers
                .get(&peer)
                .and_then(|o| (o.lane_wait_samples > 0).then_some(o.lane_wait_p50_ms)),
            home_wait_p50_ms,
            seat_is_self,
            seat_offers_slot: offers.get(&peer).is_some_and(|o| o.free_slots_live > 0),
            seat_window: offers.get(&peer).and_then(|o| o.served_context_window),
            // HER demand, or NOTHING — never the population's. This read `.or(node_median)`,
            // so a mind with no measurement of her own was judged against the median of every
            // resident's sent peak, and evicted on it: `seat_starves` feeds `FallHome`, and a
            // node hosting ONE huge prompt drags that median over every small mind's head.
            // Measured 2026-09-21 on the M5 — Benchy's peak is ~110k while Aiko wants 22-24k
            // and Atlas 21-35k, and all three were walked off the same 75,776 seat that holds
            // the latter two with room to spare.
            //
            // An absence is not a number ([[unknown-is-not-a-quantity-context-needs-the-vram-machinery]]).
            // `None` is the honest answer for an unmeasured mind, and `persona_lane_holds`
            // already treats it correctly — it asks only `window >= MIN_SERVE_CTX`, so any
            // real seat holds her and nobody is evicted on a stranger's demand. The node
            // median remains right where it belongs: SIZING lanes for the whole population
            // (`ServingDemand`), never ADMITTING one mind to a seat.
            requirement: working_set
                .sent_median_of(&[sw.persona_id()])
                .map(crate::inference::planning::serving_plan::prompt_floor_of),
        };
        // Withheld = she would have returned had the seat offered a slot. Pure and exact:
        // the same rule with the offer granted, so a cooldown or a queued seat is never
        // miscounted as "no slot".
        if !inputs.seat_offers_slot
            && decide(PlacementInputs { seat_offers_slot: true, ..inputs }) == PlacementMove::ReturnRemote
        {
            *withheld.entry(peer).or_default() += 1;
        }
        let mut mv = decide(inputs);
        // THE WORKSPACE MOVES WITH THE MIND (card 73eefbbb): a seat change happens only
        // between turns and only after her last push landed — asked BEFORE the return's
        // reservation below, so a deferred move never burns a grant. Deferred, not
        // refused: the next tick asks again, and the act-end sync retries the push.
        if moves_between_machines(&mv, seat_is_self) {
            if let Some(why) = crate::persona::workspace_transfer::move_blocker_bounded(sw.persona_id()).await {
                continuum_base::probe!(
                    class = "placement.move.deferred_unpushed",
                    persona = %sw.persona_name(),
                    peer = %peer,
                    move_kind = ?mv,
                    why = %why,
                    "her workspace is not on origin yet (or a turn is in flight) — the move waits for the next tick"
                );
                mv = PlacementMove::Stay;
            }
        }
        // A RETURN ASKS FOR ITS SLOT LIKE A SPILL DOES (2026-09-19 00:3xZ): the 5090's beacon
        // first carried its measured wait, read QUEUED, and the IntelMac brought all seven
        // bound minds home in one tick — correct. The moment that seat's wait dips under
        // the bound, `decide` says ReturnRemote for all seven at once, and a return re-bound
        // without a grant would rebuild the very queue they left: the flap at fleet scale.
        // The seat grants what it has (`free_slots_live` − outstanding grants); the rest
        // stay home until its beacon shows room. Refused or unanswered = Stay, receipted.
        if mv == PlacementMove::ReturnRemote {
            let asked = match sw.airc_handle() {
                Some(a) => crate::persona::placement_reservation::request_reservation(&a, peer, sw.persona_id()).await,
                None => Err(crate::persona::placement_reservation::ReservationAskError::Transport(
                    "no airc handle for her — cannot ask the seat".to_string(),
                )), // no local wire at all: closest typed class is transport (nothing was sent, so no fault row either)
            };
            match asked {
                Ok(g) if g.granted => continuum_base::probe!(
                    class = "placement.return.leased",
                    persona = %sw.persona_name(),
                    peer = %peer,
                    until_ms = g.until_ms,
                    seat_free_after = g.free_slots_live,
                    seat_wait_p50_ms = g.lane_wait_p50_ms,
                    "the seat granted her slot back — returning"
                ),
                Ok(g) => {
                    continuum_base::probe!(
                        class = "placement.return.refused",
                        persona = %sw.persona_name(),
                        peer = %peer,
                        reason = %g.reason,
                        seat_free = g.free_slots_live,
                        seat_outstanding = g.outstanding,
                        "the seat has no slot for her return this tick — she stays home"
                    );
                    mv = PlacementMove::Stay;
                }
                Err(e) => {
                    continuum_base::probe!(
                        class = "placement.return.refused",
                        persona = %sw.persona_name(),
                        peer = %peer,
                        kind = e.kind(), // capability vs transport without parsing reason (card 8e4c42de)
                        reason = %e,
                        "the return ask produced no grant this tick — she stays home"
                    );
                    mv = PlacementMove::Stay;
                }
            }
        }
        if let Some(line) = sw.apply(&mv, now_ms, SeatFit::of(&inputs)).await {
            lines.push(line);
        }
        // A seat that is this node is retired whatever the move (idempotent): the
        // override goes now, so the record never outlives the run that found it; the
        // loopback lane goes once she is home (a parked mind keeps the lane she has).
        if seat_is_self {
            sw.retire_self_seat();
        }
        // A SEAT THAT CANNOT HOLD HER TURN IS RETIRED THE SAME WAY, and for the same
        // reason: the record must not outlive the run that disproved it. `decide` already
        // refuses to return her to such a seat — but only while this process lives, so
        // before this the override survived every deploy and re-bound her at the next
        // boot (Benchy, Aiko, Atlas off one seat, 8 fall-homes / 0 returns, 2026-09-21).
        // Asked through the SAME predicate `decide` used, never a second reading.
        if seat_starves(inputs.seat_window, inputs.requirement) {
            sw.retire_starving_seat();
        }
        // A bound switch sitting HOME past her cooldown is a chooser candidate again —
        // the chooser may bind her to a seat that is capacity now (S1b). Her old seat is
        // only re-chosen if it is; `decide` above already refused a queued return.
        if mv == PlacementMove::Stay && sw.seat() == Seat::Home && cooled {
            at_home.push((sw.persona_id(), crate::cognition::health_ledger::lane_grants_of(sw.persona_id())));
        }
    }
    for (peer, minds) in withheld {
        let Some(o) = offers.get(&peer) else { continue };
        if o.beacon_age_ms > REMOTE_SEAT_FRESH_MS {
            continue; // a dark seat is the fall-home story, not a full one
        }
        continuum_base::probe!(
            class = "placement.return.withheld",
            peer = %peer,
            minds = minds,
            seat_free = o.free_slots_live,
            seat_wait_p50_ms = o.lane_wait_p50_ms,
            seat_wait_samples = o.lane_wait_samples,
            "her seat's beacon offers no slot this tick — no return ask sent; she stays home until it does"
        );
    }
    let moves = choose_offloads(local, &peers, rank_of, &at_home, &|mind| {
        working_set.sent_median_of(&[mind])
            .map(crate::inference::planning::serving_plan::prompt_floor_of)
    });
    if !moves.is_empty() {
        let by_id: std::collections::HashMap<Uuid, Arc<PlacementSwitch>> =
            switches().into_iter().map(|s| (s.persona_id(), s)).collect();
        for (mind, peer, model) in moves {
            let Some(sw) = by_id.get(&mind) else { continue };
            // A spill is a move between machines too (card 73eefbbb): same gate, before
            // the seat is asked for a grant that a deferral would waste.
            if let Some(why) = crate::persona::workspace_transfer::move_blocker_bounded(mind).await {
                continuum_base::probe!(
                    class = "placement.move.deferred_unpushed",
                    persona = %sw.persona_name(),
                    peer = %peer,
                    move_kind = "spill",
                    why = %why,
                    "her workspace is not on origin yet (or a turn is in flight) — the spill waits for the next tick"
                );
                continue;
            }
            // A SLOT IS A LEASE THE SEAT GRANTS: ask before she moves — on HER wire, in
            // her room, where the seat's core listens. Refused or unanswered = she stays
            // home this tick, receipted (S1b).
            let asked = match sw.airc_handle() {
                Some(a) => crate::persona::placement_reservation::request_reservation(&a, peer, mind).await,
                None => Err(crate::persona::placement_reservation::ReservationAskError::Transport(
                    "no airc handle for her — cannot ask the seat".to_string(),
                )), // no local wire at all: closest typed class is transport (nothing was sent, so no fault row either)
            };
            let grant = match asked {
                Ok(g) if g.granted => g,
                Ok(g) => {
                    continuum_base::probe!(
                        class = "placement.spill.refused",
                        persona = %sw.persona_name(),
                        peer = %peer,
                        reason = %g.reason,
                        seat_free = g.free_slots_live,
                        seat_outstanding = g.outstanding,
                        seat_wait_p50_ms = g.lane_wait_p50_ms,
                        seat_wait_samples = g.lane_wait_samples,
                        "the seat refused the slot its beacon offered — she stays home"
                    );
                    continue;
                }
                Err(e) => {
                    continuum_base::probe!(
                        class = "placement.spill.refused",
                        persona = %sw.persona_name(),
                        peer = %peer,
                        kind = e.kind(), // capability vs transport without parsing reason (card 8e4c42de)
                        reason = %e,
                        "the spill ask produced no grant this tick — she stays home"
                    );
                    continue;
                }
            };
            continuum_base::probe!(
                class = "placement.spill.leased",
                persona = %sw.persona_name(),
                peer = %peer,
                until_ms = grant.until_ms,
                seat_free_after = grant.free_slots_live,
                seat_outstanding = grant.outstanding,
                seat_wait_p50_ms = grant.lane_wait_p50_ms,
                seat_wait_samples = grant.lane_wait_samples,
                "the seat granted a slot — binding her lane"
            );
            match sw.go_remote(peer, &model, now_ms) {
                Ok(()) => {
                    crate::cognition::health_ledger::note_move(false);
                    continuum_base::probe!(
                        class = "persona.placement.offloaded",
                        persona = %sw.persona_name(),
                        peer = %peer,
                        model = %model,
                        local_resident = local.resident,
                        local_lanes = local.lanes,
                        "more minds than lanes here — her brain moved to a peer's free lane"
                    );
                    lines.push(format!(
                        "[placement] {} → {} ({}): this node has {} minds on {} lanes; the seat granted a slot ({} left, its wait p50 {} ms over {} samples)",
                        sw.persona_name(), peer, model, local.resident, local.lanes,
                        grant.free_slots_live, grant.lane_wait_p50_ms, grant.lane_wait_samples
                    ));
                }
                Err(e) => continuum_base::probe!(
                    class = "persona.placement.offload_failed",
                    persona = %sw.persona_name(),
                    peer = %peer,
                    error = %e,
                    "could not bind the remote lane — she stays home"
                ),
            }
        }
    }
    lines.extend(follow_the_allocation(now_ms).await);
    lines
}
