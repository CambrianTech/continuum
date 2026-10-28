//! Apply the allocator's better-seat opportunities between turns.
use crate::persona::placement_switch::{
    decide_opportunity, opportunity_cooldown_ms, switches, OpportunityInputs, Seat,
};

/// The opportunity pass (card 10bba591): read the grid allocator's last publish and,
/// for every switch the allocation seats somewhere strictly better than where she sits,
/// move her there between turns — a remote seat asks for its slot like a spill does; a
/// home seat retires her override. Returns the org-room lines. No allocation published
/// = nothing to follow.
pub(crate) async fn follow_the_allocation(now_ms: u64) -> Vec<String> {
    use crate::cognition::turn_ledger::{turn_in_flight, turn_shape_of};
    let mut lines = Vec::new();
    let Some(p) = crate::grid_allocator::current() else {
        return lines;
    };
    let working_set = crate::cognition::working_set::global();
    let home_plan = p.allocation.node(p.this_node).and_then(|n| n.plan.as_ref());
    for sw in switches() {
        if let Some(plan) = home_plan {
            sw.note_home_capability(plan.capability_rank);
        }
        let mind = sw.persona_id();
        let Some(seat) = p.allocation.seated.iter().find(|s| s.mind == mind) else {
            continue; // dormant: the slack's clip, not a move
        };
        let target_here = p.is_this_node(seat.node);
        let (current_node, target_is_current) = match sw.seat() {
            Seat::Home => (Some(p.this_node), target_here),
            Seat::Remote => (sw.peer(), sw.peer() == Some(seat.node)),
        };
        let current_alloc = current_node.and_then(|n| p.allocation.node(n));
        let Some(target_plan) = p.allocation.node(seat.node).and_then(|n| n.plan.as_ref()) else {
            continue;
        };
        let inputs = OpportunityInputs {
            current: current_alloc.and_then(|n| n.plan.as_ref()),
            current_holds_her: current_alloc.is_some_and(|n| n.holds.contains(&seat.role)),
            target: target_plan,
            requirement: &p.roles[seat.role].requirement,
            target_is_current,
            turn_in_flight: turn_in_flight(mind),
            since_last_move_ms: now_ms.saturating_sub(sw.moved_at_ms()),
            cooldown_ms: opportunity_cooldown_ms(turn_shape_of(mind)),
            home_capability: sw.home_capability(),
            turn_requirement: working_set
                .sent_median_of(&[mind])
                .map(crate::inference::planning::serving_plan::prompt_floor_of),
        };
        let Some(why) = decide_opportunity(&inputs) else {
            continue;
        };
        let from = current_node.map(|n| n.to_string()).unwrap_or_else(|| "nowhere".to_string()); // unwrap_or_else: a switch with no seat named reads "nowhere" in the receipt
        // THE WORKSPACE MOVES WITH THE MIND (card 73eefbbb): an opportunity move is a
        // seat change between machines like a fall-home, a return or a spill — the same
        // gate, asked BEFORE the reservation so a deferral never burns a grant. A better
        // seat is still there next tick; her unpushed work is not recoverable.
        if let Some(blocked) = crate::persona::workspace_transfer::move_blocker_bounded(mind).await {
            continuum_base::probe!(
                class = "placement.move.deferred_unpushed",
                persona = %sw.persona_name(),
                peer = %seat.node,
                move_kind = "opportunity",
                why = %blocked,
                "her workspace is not on origin yet (or a turn is in flight) — the better seat waits for the next tick"
            );
            continue;
        }
        if target_here {
            // Home is the better seat: she comes home for good — if this node can build
            // her a lane (a node that cannot never claims her back).
            if !sw.local_available().await {
                continue;
            }
            sw.come_home_for_good(now_ms);
            crate::cognition::health_ledger::note_move(true);
            continuum_base::probe!(
                class = "placement.move.opportunity",
                persona = %sw.persona_name(),
                from = %from,
                to = "home",
                reason = why.as_str(),
                model = %target_plan.model_id,
                lanes = target_plan.lanes,
                window = target_plan.window,
                cooldown_ms = inputs.cooldown_ms,
                "the grid allocation seats her at home on a strictly better seat — moved between turns"
            );
            lines.push(format!(
                "[placement] {} came home from {} on opportunity ({}): {} at {} × {}k",
                sw.persona_name(), from, why.as_str(), target_plan.model_id, target_plan.lanes, target_plan.window / 1000
            ));
            continue;
        }
        // A remote seat: a slot is a lease the seat grants — ask on her wire first.
        let asked = match sw.airc_handle() {
            Some(a) => crate::persona::placement_reservation::request_reservation(&a, seat.node, mind).await,
            None => Err(crate::persona::placement_reservation::ReservationAskError::Transport(
                "no airc handle for her — cannot ask the seat".to_string(),
            )), // no local wire at all: closest typed class is transport (nothing was sent, so no fault row either)
        };
        match asked {
            Ok(g) if g.granted => {}
            Ok(g) => {
                continuum_base::probe!(
                    class = "placement.move.opportunity_refused",
                    persona = %sw.persona_name(),
                    to = %seat.node,
                    reason = %g.reason,
                    seat_free = g.free_slots_live,
                    "the better seat has no slot for her this tick — she stays where she is"
                );
                continue;
            }
            Err(e) => {
                continuum_base::probe!(
                    class = "placement.move.opportunity_refused",
                    persona = %sw.persona_name(),
                    to = %seat.node,
                    kind = e.kind(), // capability vs transport without parsing reason (card 8e4c42de)
                    reason = %e,
                    "the move ask produced no grant this tick — she stays where she is"
                );
                continue;
            }
        }
        match sw.go_remote(seat.node, &target_plan.model_id, now_ms) {
            Ok(()) => {
                crate::cognition::health_ledger::note_move(true);
                continuum_base::probe!(
                    class = "placement.move.opportunity",
                    persona = %sw.persona_name(),
                    from = %from,
                    to = %seat.node,
                    reason = why.as_str(),
                    model = %target_plan.model_id,
                    lanes = target_plan.lanes,
                    window = target_plan.window,
                    cooldown_ms = inputs.cooldown_ms,
                    "the grid allocation seats her on a strictly better seat — moved between turns"
                );
                lines.push(format!(
                    "[placement] {} → {} on opportunity ({}): {} at {} × {}k",
                    sw.persona_name(), seat.node, why.as_str(), target_plan.model_id, target_plan.lanes, target_plan.window / 1000
                ));
            }
            Err(e) => continuum_base::probe!(
                class = "persona.placement.offload_failed",
                persona = %sw.persona_name(),
                peer = %seat.node,
                error = %e,
                "could not bind the better seat's lane — she stays where she is"
            ),
        }
    }
    lines
}
