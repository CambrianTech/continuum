//! Pure seat eligibility, recovery decisions and offload selection.
use uuid::Uuid;

/// A remote seat whose beacon is older than this is DARK for placement purposes —
/// nine missed 10 s beacons. Much shorter than the fleet fold's 6 h "stale": a lane
/// decision is per turn, a fleet verdict is per day.
pub const REMOTE_SEAT_SILENT_MS: u64 = 90_000;
/// A seat heard this recently is BACK; three beacons in a row.
pub const REMOTE_SEAT_FRESH_MS: u64 = 30_000;
/// The bound a seat's MEASURED queue is judged against when this node has no measured
/// queue of its own: the unattended time-to-first-token budget. A seat whose median
/// leased-in wait is past it is not capacity, whatever its rank (card c84d885a, S1b).
pub const SEAT_QUEUE_BOUND_MS: u64 = crate::inference::prefill_rate::UNATTENDED_TTFT.as_millis() as u64;
/// The fall-home reason a queued seat earns.
pub const SEAT_QUEUED_REASON: &str = "seat queued: its measured lane wait is past the bound";
/// The fall-home reason a seat earns by refusing her last request with every lane busy
/// (card 03660a77): taken before her next act, not at the next placement tick.
pub const SEAT_OVERLOADED_REASON: &str = "remote seat refused her last request: every lane busy";

/// PURE: is a seat's measured queue too long to be capacity for a node whose own median
/// lane wait is `home_wait_p50_ms` (`None` = unmeasured)? An UNMEASURED seat (no samples)
/// is never judged queued here — the absence-read-as-a-value guard; its live
/// `free_slots_live` and the grant it must give are what bound it. Cormac's rule: offload
/// only when the remote turn is expected FASTER — a measured seat is capacity only when
/// its wait is under this node's own, or under the budget when this node has none.
pub fn seat_queued(wait_p50_ms: u64, wait_samples: u32, home_wait_p50_ms: Option<u64>) -> bool {
    wait_samples > 0 && wait_p50_ms >= home_wait_p50_ms.unwrap_or(SEAT_QUEUE_BOUND_MS)
}
/// At most one move per persona per cooldown — a flapping tower moves her twice an hour,
/// never twice a minute.
pub const MOVE_COOLDOWN_MS: u64 = 600_000;


/// THE OPPORTUNITY MOVE (card 10bba591; Joel: "can it grow to higher capacity as both
/// the M5 and the 5090 come online, ideally no time lost"). The failure rule above moves
/// a mind only when her seat fails her. This is the mirror: the grid allocator
/// (`modules::grid_allocator`) seats her on the node the grid's offers make best for
/// her role, and when that is NOT where she sits and it is STRICTLY better — first a
/// node that holds her requirement at all, then the allocator's own order (capability,
/// target fit, lanes, window) — the switch moves her there BETWEEN turns, never mid-turn, and never
/// inside her own cooldown. Equal seats are never a reason to move.
#[derive(Debug, Clone, Copy)]
pub struct OpportunityInputs<'a> {
    /// The plan of the seat she is on now, as the allocation reads that node; `None` =
    /// her node offers no plan the allocation counts.
    pub current: Option<&'a crate::cognition::grid_allocation::LanePlan>,
    /// Does her current node's plan hold HER role at all (the node's `holds`)?
    pub current_holds_her: bool,
    /// The plan of the seat the allocation gives her.
    pub target: &'a crate::cognition::grid_allocation::LanePlan,
    /// The same role requirement used by the allocation being followed.
    pub requirement: &'a crate::cognition::grid_allocation::Requirement,
    pub target_is_current: bool,
    pub turn_in_flight: bool,
    pub since_last_move_ms: u64,
    pub cooldown_ms: u64,
    /// The capability of her HOME plan (`PlacementSwitch::home_capability`); `None` =
    /// unknown in this life.
    pub home_capability: Option<u8>,
    /// What HER turn needs of a lane, measured: the same number the fall-home rule reads
    /// ([`seat_starves`]); `None` = unmeasured, the serve floor stands.
    pub turn_requirement: Option<u32>,
}

/// The rule. Pure.
pub fn decide_opportunity(i: &OpportunityInputs<'_>) -> Option<crate::cognition::grid_allocation::BetterBy> {
    if i.target_is_current || i.turn_in_flight || i.since_last_move_ms < i.cooldown_ms {
        return None;
    }
    // ONE ANSWER TO "CAN THIS SEAT HOLD HER TURN" (card 60d90c95): the placer asked the
    // role's requirement while the fall-home rule asked her measured turn, so a seat the
    // placer called better was the seat fall-home rejected, and she bounced (Iris
    // IntelMac↔M5 six times in 30 min, 2026-09-26 13:33-14:05Z). A seat that starves her
    // turn is never an opportunity: the same predicate, both directions.
    if seat_starves(Some(i.target.window), i.turn_requirement) {
        return None;
    }
    // AN OPPORTUNITY NEVER STEPS DOWN (card 60d90c95, Fable 2026-09-26 16:4xZ): Kimi, served
    // on a 27B, was moved "on opportunity (requirement)" to a 1.5B CPU coder the moment her
    // home plan was momentarily absent, and went dark. Capability below her home is a
    // failure move's business (fall-home, spill), which says so in its receipt; never
    // "better". Unknown home with no current plan is not permission either: an absence
    // is not a number, and the spill path still answers a real shortage here.
    match i.home_capability {
        Some(home) if i.target.capability_rank < home => return None,
        None if i.current.is_none() => return None,
        _ => {}
    }
    match i.current {
        Some(c) if i.current_holds_her => i.target.better_than(c, i.requirement),
        // Her node seats nobody of her role: any seat that holds her is objective 1.
        _ => Some(crate::cognition::grid_allocation::BetterBy::Requirement),
    }
}

/// Her cooldown between opportunity moves, in HER units: her measured turn cadence (the
/// gap between her turn starts), never under her turn duration — so she completes at
/// least one turn on the new seat before she may be moved again. Nothing measured yet =
/// the failure rule's cooldown, the one number that already bounds a move. Pure.
pub fn opportunity_cooldown_ms(shape: Option<crate::cognition::turn_ledger::TurnShape>) -> u64 {
    match shape {
        Some(s) if s.cadence_ms.max(s.turn_ms) > 0 => s.cadence_ms.max(s.turn_ms),
        _ => MOVE_COOLDOWN_MS,
    }
}

/// The bound "remote" seat is one of THIS node's own ids (card 2500d2f1): she was never
/// off-box, so she comes home at once — no cooldown, no beacon read — and the durable
/// record that named the seat is retired so the next boot does not repeat it.
pub const SEAT_STARVES_REASON: &str = "seat window below her turn: a lane that cannot serve is no seat";
pub const SELF_SEAT_REASON: &str = "seat is this node: never a remote seat";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Seat {
    Remote = 0,
    Home = 1,
}

/// What the pure rule sees.
#[derive(Debug, Clone, Copy)]
pub struct PlacementInputs {
    pub seat: Seat,
    /// `None` = the seat's beacon was never heard this boot.
    pub beacon_age_ms: Option<u64>,
    pub lane_cold: bool,
    pub local_available: bool,
    pub since_last_move_ms: u64,
    /// Has the seat's PEER ever served (or is it brand new)? A fresh beacon proves the
    /// peer is ALIVE, not that it will ANSWER; without this a rescued citizen is
    /// returned to a peer that has never served a single request (card ad96f5d1). From
    /// the peer breaker's `served_ok` — the summed-success mirror of `lane_cold`.
    pub peer_served_ok: bool,
    /// The bound seat's own median leased-in wait from its beacon; `None` = unmeasured or
    /// no beacon. Measured and past the bound → she is a candidate again (S1b).
    pub seat_wait_p50_ms: Option<u64>,
    /// This node's own median lane wait; `None` = unmeasured. "Home, if home is faster."
    pub home_wait_p50_ms: Option<u64>,
    /// The bound seat is one of this node's own airc ids ([`crate::persona::self_peer`]).
    pub seat_is_self: bool,
    /// The seat's freshest beacon offers at least one free slot (`free_slots_live > 0`).
    /// A RETURN asks for its slot like a spill does, and a spill only asks a seat whose
    /// beacon offers one; without this mirror every home-bound mind asked a full seat
    /// every tick (the M5, 2026-09-19 22:3xZ: 15 minds × 60 s into "no free slot: 0
    /// free", 30 wire messages a minute for nothing).
    pub seat_offers_slot: bool,
    /// The bound seat's per-slot window from its beacon; `None` = unknown.
    pub seat_window: Option<u32>,
    /// What HER turn needs of a lane; `None` = unmeasured, the serve floor stands.
    pub requirement: Option<u32>,
}

/// THE TWO NUMBERS A SEAT VERDICT COMPARED. Carried to the probe so the glass-box record
/// answers the question it raises: a `fell_home` line reading "seat window below her turn"
/// without the width and the demand cannot tell a seat that is honestly too small from a
/// beacon that disagrees with its own node's plan — and on 2026-09-21 that ambiguity is
/// exactly what stalled the diagnosis of three citizens falling off one seat. A verdict
/// that omits its inputs is the same defect as `NodeVerdict::BelowEveryRequirement` never
/// naming the requirement it missed. `None` is honest: an unpublished width, or a mind
/// whose turn has not been measured yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SeatFit {
    /// The seat's per-slot window as ITS beacon published it — never this node's plan for it.
    pub window: Option<u32>,
    /// What her turn needs of a lane, measured.
    pub requirement: Option<u32>,
}

impl SeatFit {
    pub fn of(i: &PlacementInputs) -> Self {
        Self { window: i.seat_window, requirement: i.requirement }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlacementMove {
    FallHome { reason: &'static str },
    ReturnRemote,
    /// The seat is dark and this node has nothing to run her on: say so, move nothing.
    Park { reason: &'static str },
    Stay,
}

/// CAN THIS SEAT HOLD HER TURN AT ALL? The ONE answer, because two callers ask it: the
/// rule below (which move to make) and the tick (whether the durable override naming the
/// seat should survive the reboot). Before this was extracted only [`decide`] asked, so a
/// seat proven unable to serve her was walked home every boot and re-bound by the next
/// one — three citizens (Benchy, Aiko, Atlas) off the same seat across four deploys on
/// 2026-09-21, 8 `fell_home` events against 0 returns ([[the-compression-principle]]).
///
/// Unknown width (an older core's beacon, or a seat that has never published one) is NOT
/// a refusal — an absence is not a number, and [`crate::inference::planning::serving_plan::persona_lane_holds`]
/// already answers the unmeasured-requirement case with the serve floor. Pure.
pub fn seat_starves(seat_window: Option<u32>, requirement: Option<u32>) -> bool {
    seat_window.is_some_and(|w| !crate::inference::planning::serving_plan::persona_lane_holds(w, requirement))
}

/// The rule. Pure so the four scenarios are hand-computed tests.
pub fn decide(i: PlacementInputs) -> PlacementMove {
    // A seat that is this node is no seat: bound to it she runs a loopback lane, so she
    // comes home now (cooldown is for moves between machines); at home she is never
    // "returned" to it, however fresh its beacon reads (card 2500d2f1).
    if i.seat_is_self {
        return match i.seat {
            Seat::Remote if i.local_available => PlacementMove::FallHome { reason: SELF_SEAT_REASON },
            Seat::Remote => PlacementMove::Park { reason: SELF_SEAT_REASON },
            Seat::Home => PlacementMove::Stay,
        };
    }
    // A SEAT WHOSE WINDOW CANNOT HOLD HER TURN IS NO SEAT: every prompt on it is refused
    // before the model runs (2026-09-20: `delib.prompt.capacity` 172× an hour on a 2,048-
    // token seat). She comes home now — no cooldown, this is not a flap between machines,
    // it is a lane that cannot serve — and she is never returned to it. Unknown width
    // (an older core's beacon) is not a refusal: an absence is not a number.
    if seat_starves(i.seat_window, i.requirement) {
        return match i.seat {
            Seat::Remote if i.local_available => PlacementMove::FallHome { reason: SEAT_STARVES_REASON },
            Seat::Remote => PlacementMove::Park { reason: SEAT_STARVES_REASON },
            Seat::Home => PlacementMove::Stay,
        };
    }
    let dark = i.beacon_age_ms.map(|a| a > REMOTE_SEAT_SILENT_MS).unwrap_or(true); // JUSTIFIED unwrap_or: never heard = dark, by definition
    let cooled = i.since_last_move_ms >= MOVE_COOLDOWN_MS;
    // A measured seat whose queue is past the bound (this node's own wait, else the
    // unattended TTFT budget) is not capacity: she comes home if home is faster, and
    // `follow_the_fleet` may re-choose her for a seat that is (card c84d885a, S1b —
    // Cormac: "a bound switch is never re-chosen" was the defect).
    let queued = i
        .seat_wait_p50_ms
        .is_some_and(|w| seat_queued(w, 1, i.home_wait_p50_ms));
    match i.seat {
        Seat::Remote => {
            let reason = if i.lane_cold {
                "breaker cold: three deadline misses in a row"
            } else if dark {
                "seat silent: no capacity beacon"
            } else if queued {
                SEAT_QUEUED_REASON
            } else {
                return PlacementMove::Stay;
            };
            if !cooled {
                return PlacementMove::Stay;
            }
            if i.local_available {
                PlacementMove::FallHome { reason }
            } else {
                PlacementMove::Park { reason }
            }
        }
        Seat::Home => {
            let fresh = i.beacon_age_ms.map(|a| a < REMOTE_SEAT_FRESH_MS).unwrap_or(false); // JUSTIFIED unwrap_or: never heard = not fresh
            // A fresh beacon proves the peer is ALIVE; `peer_served_ok` proves it will
            // ANSWER (or has never been tried). Without the service term a rescued
            // citizen is returned to a never-serving peer the moment the 5-minute cold
            // window lapses — the flap card ad96f5d1 measured.
            // …and the beacon must OFFER a slot before she asks for one: the ask is the
            // seat's admission, the beacon is its invitation. A full seat is not asked.
            if fresh && !i.lane_cold && cooled && i.peer_served_ok && !queued && i.seat_offers_slot {
                PlacementMove::ReturnRemote
            } else {
                PlacementMove::Stay
            }
        }
    }
}

/// PURE: is this a move BETWEEN MACHINES — the kind her workspace must be carried across
/// first (card 73eefbbb)? A fall-home or a return is; Stay and Park move nothing; and a
/// "remote" seat that is this very node (card 2500d2f1) is a loopback lane, not another
/// machine — her workspace is already here, so it never waits on a push.
pub fn moves_between_machines(mv: &PlacementMove, seat_is_self: bool) -> bool {
    !seat_is_self && matches!(mv, PlacementMove::FallHome { .. } | PlacementMove::ReturnRemote)
}

/// What a peer's beacon says it can host (stage B input, from `CapacityOffer`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerOffer {
    pub peer: Uuid,
    pub served_model: Option<String>,
    pub lanes: u32,
    pub residents: u32,
    pub beacon_age_ms: u64,
    /// The seat's OWN free slots (lanes minus everything in flight, local and leased-in),
    /// from its beacon (card c84d885a, S1). `lanes − residents` is roster arithmetic that
    /// every node evaluated independently and so spilled onto the same slot; this is the
    /// seat's admission truth. 0 on an older beacon = no free slot offered.
    pub free_slots_live: u32,
    /// The seat's own median receipt-to-first-progress wait for leased-in generates, ms,
    /// with its sample count — unmeasured (0 samples) is never a fast seat.
    pub lane_wait_p50_ms: u64,
    pub lane_wait_samples: u32,
    /// The seat's per-slot window from its beacon; `None` = unknown (older core).
    pub served_context_window: Option<u32>,
}

/// This node's own shape for the chooser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalShape {
    /// This node's own median lane wait, ms; `None` = unmeasured. `Some(0)` — a median
    /// under a millisecond — means the lanes here are not contended and nobody moves.
    pub lane_wait_p50_ms: Option<u64>,
    pub resident: u32,
    pub lanes: u32,
    /// Measured capability of the local served model, or the planner's unmeasured proxy cap.
    pub rank: u8,
}

/// The pure chooser: which minds move to which peer. `candidates` = minds at Home with
/// their lane grants this hour (fewest first = most starved). A node with more minds than
/// lanes borrows a fresh peer's FREE lanes (lanes − its residents) when that peer's served
/// model ranks at least as high; never more minds than the node is over by.
pub fn choose_offloads(
    local: LocalShape,
    peers: &[PeerOffer],
    rank_of: &(dyn Fn(&str) -> Option<u8> + Sync),
    candidates: &[(Uuid, u64)],
    requirement_of: &dyn Fn(Uuid) -> Option<u32>,
) -> Vec<(Uuid, Uuid, String)> {
    let mut over = local.resident.saturating_sub(local.lanes) as usize;
    // No queue at home (a measured median under a millisecond) = nothing to gain by
    // moving anyone, however many minds share the lanes (Cormac's shape 3: offload only
    // when the remote turn is expected faster). Unmeasured home = minds over lanes is
    // the only evidence there is; move on it.
    let home_queued = local.lane_wait_p50_ms != Some(0);
    if over == 0 || candidates.is_empty() || !home_queued {
        return Vec::new();
    }
    let mut minds: Vec<_> = candidates.iter()
        .map(|(mind, grants)| (*mind, *grants, requirement_of(*mind)))
        .collect();
    minds.sort_by_key(|(_, grants, _)| *grants);
    let mut out = Vec::new();
    for p in peers {
        if over == 0 {
            break;
        }
        if p.beacon_age_ms > REMOTE_SEAT_FRESH_MS {
            continue;
        }
        let Some(model) = p.served_model.as_deref() else { continue };
        let Some(rank) = rank_of(model) else { continue };
        if rank < local.rank {
            continue;
        }
        if seat_queued(p.lane_wait_p50_ms, p.lane_wait_samples, local.lane_wait_p50_ms) {
            continue;
        }
        // THE SEAT'S live grant-able count (lanes − in flight − already granted), not
        // `lanes − residents` roster arithmetic every node once evaluated on its own. The
        // grant itself is acquired by the caller before she moves.
        let mut free = p.free_slots_live as usize;
        while free > 0 && over > 0 {
            // Fit this mind, not the population median. Keep an unseated candidate
            // available for a later, wider peer; she must not consume this slot or
            // prevent a smaller candidate from using it.
            let Some(index) = minds.iter().position(|(_, _, requirement)| {
                !seat_starves(p.served_context_window, *requirement)
            }) else { break };
            let (mind, _, _) = minds.remove(index);
            out.push((mind, p.peer, model.to_string()));
            free -= 1;
            over -= 1;
        }
    }
    out
}
