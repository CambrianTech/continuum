//! THE GRID ALLOCATOR DAEMON (card 10bba591; Joel: "Can it grow to higher capacity as
//! both the M5 and the 5090 come online, ideally no time lost").
//!
//! Before this daemon the pure allocator (`cognition::grid_allocation::allocate`) had no
//! caller on the live path: minds moved only on FAILURE (a lane wait past a bound, a seat
//! window below the turn — `persona::placement_switch::decide`), nobody re-ran the
//! allocation when a node joined or a plan changed, so a mind on a 32k seat stayed there
//! when a 70k seat appeared. This module is the ONE caller: it gathers what already
//! exists — this node's serving plan (`LanePlan::of` the published `ServingPlan` + the
//! measured decode), every peer's offer from the capacity gossip the grid module already
//! consumes (`capacity::gossip::global_ledger`, the same rows, no second listener), the
//! live roster with the recipe's citizens (`roles_from`, #4271 — role + declared
//! requirement, the undeclared sizing target being the largest `LaneRequirement` this
//! seat knows, without promoting that measurement to a declared gate) and the operator's hold —
//! folds them into a [`GridInputs`], and publishes the [`GridAllocation`] through a
//! `watch::Sender` for the placement switch (the opportunity move), the spawner (this
//! node's open seats) and the health line to read.
//!
//! Shape per [[docs/architecture/CONCURRENCY-STYLE-GUIDE.md]]: own tokio task, one
//! `tokio::time::interval` at the gossip cadence, woken early on change (the ledger's
//! `heard_notify`, the plan watch), the body under `catch_unwind` with quarantine after
//! three panics, an atomic gate holding the last inputs key so an unchanged grid costs
//! one hash and publishes nothing (Joel: "pass rich structs by reference, never re-derive
//! per tick"), a `watch::Sender<Option<Arc<Published>>>` readers borrow lock-free, and no
//! lock across an await (the offer book is a `std::sync::Mutex` scoped to the sync pass).
//!
//! Nothing here holds a node, a model or a number of Joel's grid: a node that joins is
//! one more offer; a bigger box is a better plan in its offer.

use crate::cognition::grid_allocation::{
    allocate, inputs_key, roles_from, GridAllocation, GridInputs, GridRoster, Hold, LanePlan,
    Mind, NodeOffer, OfferBook, OfferTerms, OpenSeats, Role,
};
use crate::cognition::serving_plan::ServingPlan;
use crate::cognition::window_allocator::{largest_known, requirements_for};
use crate::experience::recipe::CitizenRecipe;
use crate::runtime::{CommandResult, CommandSchema, ModuleConfig, ModulePriority, ServiceModule};
use futures::FutureExt;
use std::any::Any;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tokio::sync::watch;
use uuid::Uuid;

/// The cadence: the gossip beat. An offer lands once a beat, so the allocation is never
/// more than a beat behind the grid; the change edges below wake it sooner.
const TICK: Duration = Duration::from_millis(crate::capacity::gossip::PUBLISH_INTERVAL_MS);
/// A seat whose beacon is older than this has left the allocator's grid — the SEAT
/// bound the placement rule already uses, not the fleet fold's six-hour verdict.
const SILENT_AFTER_MS: u64 = crate::persona::placement_switch::REMOTE_SEAT_SILENT_MS;
/// Three panics in a row and the task quarantines itself (the style guide's rule).
const QUARANTINE_AFTER_PANICS: u32 = 3;
/// The owner every node and mind on one operator's grid share. The allocator consults
/// terms only between DIFFERENT owners; until a beacon carries an owner and terms (the
/// negotiation card), one grid is one unnamed owner and every offer is open.
const ONE_OWNER: Uuid = Uuid::nil();

/// A mind with no seat, and how long since her last turn began (since boot when none).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DormantMind {
    pub mind: Uuid,
    pub last_turn_age_ms: u64,
}

/// What the daemon publishes: the allocation, the key it was computed from, and the
/// dormant order the slack owes a clip to.
#[derive(Clone, Debug)]
pub(crate) struct Published {
    pub allocation: GridAllocation,
    /// The exact hard minima and measured targets used for this allocation.
    pub roles: Vec<Role>,
    pub key: u64,
    pub at_ms: u64,
    /// The id this node offered under (`capacity::gossip::this_process_origin`).
    pub this_node: Uuid,
    pub owner: Uuid,
    /// The dormant minds, OLDEST last turn first — the order the slow clip serves.
    pub dormant_by_age: Vec<DormantMind>,
    /// How often the grid's slack can give each dormant mind a turn; `None` = no slack
    /// or no measured turn yet (an absence, never a number).
    pub clip_interval_ms: Option<u64>,
}

impl Published {
    pub(crate) fn oldest_dormant_turn_age_ms(&self) -> Option<u64> {
        self.dormant_by_age.first().map(|d| d.last_turn_age_ms)
    }
    /// This node's roster as the allocation reads it — the spawner's seat count once
    /// published (`None` = this node made no offer; the spawner keeps its own prior).
    pub(crate) fn roster_here(&self) -> Option<GridRoster> {
        self.allocation.roster_on(self.this_node, self.owner)
    }
    pub(crate) fn is_this_node(&self, node: Uuid) -> bool {
        node == self.this_node || crate::persona::self_peer::is_this_node(node)
    }
}

fn channel() -> &'static watch::Sender<Option<Arc<Published>>> {
    static CHANNEL: OnceLock<watch::Sender<Option<Arc<Published>>>> = OnceLock::new();
    CHANNEL.get_or_init(|| watch::channel(None).0)
}

/// The latest published allocation, borrowed lock-free; `None` before the first publish.
pub(crate) fn current() -> Option<Arc<Published>> {
    channel().borrow().clone()
}

/// Subscribe to publishes — the reconciler parks on this beside the serving plan.
pub(crate) fn subscribe() -> watch::Receiver<Option<Arc<Published>>> {
    channel().subscribe()
}

/// One peer's offer as the tick read it off the ledger.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PeerFacts {
    pub node: Uuid,
    /// `None` = the beacon named no served model or no width (an older core): nothing
    /// placeable, the node is heard but offers no plan.
    pub plan: Option<LanePlan>,
    pub residents: u32,
    pub heard_at_ms: u64,
    pub free_slots_live: u32,
    pub lanes: u32,
}

/// What one pass GATHERS, once, then hands to the pure builder — so the decision is
/// testable with no globals.
#[derive(Clone, Debug)]
pub(crate) struct GridFacts {
    pub now_ms: u64,
    pub this_node: Uuid,
    pub local_plan: Option<LanePlan>,
    pub local_lanes: u32,
    pub local_free_slots: u32,
    pub peers: Vec<PeerFacts>,
    /// The live minds on this node (their home is this node).
    pub minds: Vec<Uuid>,
    /// The recipe's resident citizens — role + what each DECLARES of a lane
    /// (`CitizenRequirement`). The allocator's roles and floors are `roles_from` over
    /// exactly these (#4271), never a second fold.
    pub citizens: Vec<CitizenRecipe>,
    /// The sizing target an UNDECLARED role prefers: the largest requirement this seat knows
    /// (`window_allocator::largest_known` over `requirements_for` — the same derivation
    /// the serving daemon sizes its lanes from), `None` while nothing is measured.
    pub undeclared_window: Option<u32>,
    /// The operator's roster hold resolved to ids, and whether it is exclusive.
    pub hold: Option<(Vec<Uuid>, bool)>,
}

/// Fold the facts into the book and build the allocator's inputs. PURE over its
/// arguments. Peers' RESIDENTS are seated as anonymous minds homed there (ids derived
/// from the node), so a peer's own roster fills its seats before ours spill onto them —
/// without this a 4-resident, 2-lane peer read as four open seats.
pub(crate) fn build_inputs(f: &GridFacts, book: &mut OfferBook) -> GridInputs {
    if let Some(plan) = &f.local_plan {
        book.hear(NodeOffer { node: f.this_node, owner: ONE_OWNER, terms: OfferTerms::open(), plans: vec![plan.clone()] }, f.now_ms);
    }
    for p in &f.peers {
        book.hear(
            NodeOffer { node: p.node, owner: ONE_OWNER, terms: OfferTerms::open(), plans: p.plan.iter().cloned().collect() },
            p.heard_at_ms,
        );
    }
    book.forget_silent(f.now_ms, SILENT_AFTER_MS);
    let mut nodes = book.live(f.now_ms, SILENT_AFTER_MS);
    // This node's own offer is fresh by definition while it has a plan; without one it
    // is no seat (a node serving nothing offers nothing).
    if f.local_plan.is_none() {
        nodes.retain(|n| n.node != f.this_node);
    }
    let (roles, floors) = roles_from(&f.citizens, f.undeclared_window);
    let mut minds: Vec<Mind> = Vec::new();
    if !roles.is_empty() {
        // Every resident hosts the recipe's first role today (the spawner seats one
        // role until role-in-seed lands); the allocator's role 0 is that role.
        minds.extend(f.minds.iter().map(|&id| Mind { id, role: 0, home: Some(f.this_node), owner: ONE_OWNER }));
        for p in &f.peers {
            let Some(plan) = &p.plan else { continue };
            let Some(role) = roles.iter().position(|r| plan.holds(&r.requirement)) else { continue };
            if !nodes.iter().any(|n| n.node == p.node) {
                continue;
            }
            minds.extend((0..p.residents).map(|i| Mind {
                id: Uuid::new_v5(&p.node, format!("resident-{i}").as_bytes()),
                role,
                home: Some(p.node),
                owner: ONE_OWNER,
            }));
        }
    }
    let holds = f
        .hold
        .as_ref()
        .map(|(only, exclusive)| vec![Hold { node: f.this_node, only: only.clone(), exclusive: *exclusive }])
        .unwrap_or_default(); // unwrap_or_default: no hold = no hold row
    GridInputs {
        roles,
        minds,
        nodes,
        minds_per_lane: crate::modules::citizen_health::MINDS_PER_LANE_STARVED_ABOVE as u32,
        holds,
        floors,
    }
}

/// The dormant minds ordered by last-turn age, oldest first — the slow clip's queue.
/// `last_turn_ms` = when her last turn began, `None` = none since boot (aged from boot).
pub(crate) fn dormant_by_age(dormant: &[Uuid], now_ms: u64, boot_ms: u64, last_turn_ms: impl Fn(Uuid) -> Option<u64>) -> Vec<DormantMind> {
    let mut out: Vec<DormantMind> = dormant
        .iter()
        .map(|&mind| DormantMind { mind, last_turn_age_ms: now_ms.saturating_sub(last_turn_ms(mind).unwrap_or(boot_ms)) }) // unwrap_or: no turn yet is aged from boot
        .collect();
    out.sort_by_key(|d| std::cmp::Reverse(d.last_turn_age_ms));
    out
}

/// The grid's SLACK in seats: every open seat weighted by its node's idle fraction
/// (`free_slots_live / lanes` — the seat's own admission truth, so a lane a leased-in
/// generate is using counts less than an empty one).
pub(crate) fn slack_seats(open: &[OpenSeats], idle_fraction_of: impl Fn(Uuid) -> f64) -> f64 {
    open.iter().map(|o| o.count as f64 * idle_fraction_of(o.node).clamp(0.0, 1.0)).sum()
}

/// How often the slack can give each dormant mind a turn: the typical turn, times the
/// dormant population, over the slack — derived, never a constant. `None` when there is
/// no slack, nobody dormant, or no turn measured yet.
pub(crate) fn clip_interval_ms(dormant: usize, slack: f64, typical_turn_ms: Option<u64>) -> Option<u64> {
    if dormant == 0 || slack <= 0.0 {
        return None;
    }
    typical_turn_ms.map(|t| (t as f64 * dormant as f64 / slack).round() as u64)
}

struct Inner {
    plan_rx: watch::Receiver<Option<ServingPlan>>,
    citizens: Vec<CitizenRecipe>,
    book: Mutex<OfferBook>,
    /// The gate: the inputs key of the last publish; `u64::MAX` = never published.
    last_key: AtomicU64,
    passes: AtomicU64,
    skipped: AtomicU64,
}

pub(crate) struct GridAllocatorModule {
    inner: Arc<Inner>,
}

impl GridAllocatorModule {
    /// `plan_rx` = the serving daemon's published plan (`ServingDaemonModule::subscribe`);
    /// `citizens` = the recipe's resident roles (the same list the spawner hosts).
    pub(crate) fn new(plan_rx: watch::Receiver<Option<ServingPlan>>, citizens: Vec<CitizenRecipe>) -> Self {
        Self {
            inner: Arc::new(Inner {
                plan_rx,
                citizens,
                book: Mutex::new(OfferBook::default()),
                last_key: AtomicU64::new(u64::MAX),
                passes: AtomicU64::new(0),
                skipped: AtomicU64::new(0),
            }),
        }
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0) // unwrap_or: a pre-epoch clock reads 0, as every other now_ms in the substrate
}

impl Inner {
    /// Gather the pass's facts: cheap reads of pre-staged state, no probe, no await.
    fn gather(&self) -> GridFacts {
        let now = now_ms();
        let serving = crate::inference::llama_server::current_serving();
        let rank_of = |id: &str| -> Option<u8> {
            crate::model_registry::global().model(id).and_then(|m| m.serving.measured_capability)
        };
        // This node's plan, read as an offer: the published plan's shape with the decode
        // this box measured for its model — `tps_for`, the rate at the LIGHTEST trusted
        // concurrency. The plan's own lane count already respects the measured knee
        // (`plan_serving` applies `knee_lanes`), which is the statement "every lane here
        // decodes at or above the floor"; this number says how fast that is. `None` =
        // never measured, which the allocator reads as an absence, never as a refusal.
        let local_plan = self.plan_rx.borrow().as_ref().filter(|p| p.fits_on_gpu).map(|p| {
            LanePlan::of(p, crate::inference::decode_knee::tps_for(&p.base_model.model_id).map(|t| t as f32))
        });
        let peers = crate::capacity::gossip::global_ledger()
            .foreign_offers_with_age()
            .into_iter()
            .filter(|(p, _, _)| !crate::persona::self_peer::is_this_node(*p))
            .map(|(node, o, heard_at_ms)| PeerFacts {
                node,
                plan: match (&o.served_model, o.served_context_window > 0 && o.lanes > 0) {
                    (Some(model), true) => Some(LanePlan {
                        model_id: model.clone(),
                        // The registry's measured rank, else the planner's own unmeasured
                        // proxy cap — the same number this node's plan would carry.
                        capability_rank: rank_of(model).unwrap_or(crate::modules::serving_daemon::UNMEASURED_RANK_CAP), // unwrap_or: an unmeasured model ranks at the proxy cap, the planner's number
                        window: o.served_context_window,
                        lanes: o.lanes,
                        decode_tps_per_lane: None,
                    }),
                    _ => None,
                },
                residents: o.residents,
                heard_at_ms,
                free_slots_live: o.free_slots_live,
                lanes: o.lanes,
            })
            .collect();
        let registry = crate::persona::airc_runtime_registry::PersonaAircRuntimeRegistry::try_global();
        let minds: Vec<Uuid> = registry.as_ref().map(|r| r.live_personas()).unwrap_or_default(); // unwrap_or_default: no registry = no residents yet
        // ONE requirement notion in the crate (card 10bba591, Cormac on #4281): what a
        // mind needs of a lane is `window_allocator::LaneRequirement` — the untrimmed
        // demand with headroom, Unknown until she has turned — and an undeclared ROLE
        // stands at the largest of those this seat knows. Never a second median here.
        let undeclared_window = largest_known(&requirements_for(&minds, &crate::cognition::working_set::global()));
        let hold = crate::persona::roster_hold::active().map(|h| {
            let ids: Vec<Uuid> = registry
                .as_ref()
                .map(|r| h.only.iter().filter_map(|n| r.get_by_agent_name(n).map(|rt| rt.persona_id())).collect())
                .unwrap_or_default(); // unwrap_or_default: no registry = no name resolves
            (ids, h.exclusive)
        });
        GridFacts {
            now_ms: now,
            this_node: crate::capacity::gossip::this_process_origin(),
            local_plan,
            local_lanes: serving.lanes,
            local_free_slots: crate::persona::placement_reservation::free_slots_live_now(serving.lanes, now),
            peers,
            minds,
            citizens: self.citizens.clone(),
            undeclared_window,
            hold,
        }
    }

    /// One pass: gather → inputs → key; unchanged = nothing; else allocate and publish.
    fn pass(&self) {
        self.passes.fetch_add(1, Ordering::Relaxed);
        let facts = self.gather();
        let inputs = {
            let mut book = self.book.lock().unwrap_or_else(|p| p.into_inner()); // JUSTIFIED unwrap_or_else: a poisoned book still holds the offers; it is refilled every pass
            build_inputs(&facts, &mut book)
        };
        let key = inputs_key(&inputs);
        if self.last_key.load(Ordering::Acquire) == key {
            self.skipped.fetch_add(1, Ordering::Relaxed);
            return;
        }
        let allocation = allocate(&inputs);
        let boot = crate::cognition::resource_admission::boot_ms();
        let dormant = dormant_by_age(&allocation.dormant, facts.now_ms, boot, crate::cognition::resource_admission::last_turn_ms);
        let idle_of = |node: Uuid| -> f64 {
            if node == facts.this_node {
                return if facts.local_lanes > 0 { facts.local_free_slots as f64 / facts.local_lanes as f64 } else { 0.0 };
            }
            facts
                .peers
                .iter()
                .find(|p| p.node == node)
                .map(|p| if p.lanes > 0 { p.free_slots_live as f64 / p.lanes as f64 } else { 0.0 })
                .unwrap_or(0.0) // unwrap_or: a node the facts do not name has no idle seat to lend
        };
        let slack = slack_seats(&allocation.open, idle_of);
        let clip = clip_interval_ms(dormant.len(), slack, crate::cognition::resource_admission::typical_turn_ms(&facts.minds));
        let published = Published {
            key,
            at_ms: facts.now_ms,
            this_node: facts.this_node,
            owner: ONE_OWNER,
            dormant_by_age: dormant,
            clip_interval_ms: clip,
            allocation,
            roles: inputs.roles,
        };
        let seats_per_node: Vec<String> = published
            .allocation
            .nodes
            .iter()
            .map(|n| format!("{}:{}", &n.node.to_string()[..8], n.seats))
            .collect();
        crate::probe!(
            class = "grid.allocation.published",
            key = key,
            nodes = published.allocation.nodes.len() as u64,
            seats_per_node = %seats_per_node.join(","),
            seated = published.allocation.seated.len() as u64,
            open_seats = published.allocation.open_total(),
            dormant = published.allocation.dormant.len() as u64,
            // Of the dormant, the ones no node could HOLD — a seat was free somewhere
            // and every free seat refused them. That is not "waiting for room", it is
            // "this grid cannot serve this mind", and it is the sentence nobody said
            // while Benchy read as a citizen who produces nothing (card b8503234).
            unservable = published.allocation.unservable.len() as u64,
            minds = inputs.minds.len() as u64,
            roles = published.roles.len() as u64,
            "the grid allocation changed — published for the switch, the spawner and the health line"
        );
        // A COUNT DOES NOT REACH A PERSON. The published allocation knows exactly which
        // minds this grid cannot hold, so it says their ids rather than leaving a reader
        // to diff two lists. Only when there ARE any — a grid that serves everyone stays
        // silent here.
        if !published.allocation.unservable.is_empty() {
            crate::probe!(
                class = "grid.mind.unservable",
                // 8-hex prefixes, the same shape a citizen reads on a board line.
                // Spelled here rather than borrowing `modules::work::short8`, which is
                // private to that module — a receipt is not a reason to widen someone
                // else's surface.
                minds = %published
                    .allocation
                    .unservable
                    .iter()
                    .map(|m| m.to_string().chars().take(8).collect::<String>())
                    .collect::<Vec<_>>()
                    .join(","),
                count = published.allocation.unservable.len() as u64,
                "a seat was free and every free seat refused them — their turn exceeds this grid, and waiting cannot fix it"
            );
        }
        crate::probe!(
            class = "grid.dormant.oldest_turn_age_ms",
            dormant = published.dormant_by_age.len() as u64,
            oldest_turn_age_ms = published.oldest_dormant_turn_age_ms().unwrap_or(0), // unwrap_or: 0 with dormant 0 = nobody waits
            slack_seats = slack,
            clip_interval_ms = published.clip_interval_ms.unwrap_or(0), // unwrap_or: 0 = no clip derivable (no slack, or no turn measured)
            "the dormant order and the interval the grid's slack owes each of them"
        );
        self.last_key.store(key, Ordering::Release);
        channel().send_replace(Some(Arc::new(published)));
    }
}

#[async_trait::async_trait]
impl ServiceModule for GridAllocatorModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "grid_allocator",
            priority: ModulePriority::Background,
            command_prefixes: &["allocator/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            // Own task (below), not the runtime's tick loop: the pass wakes on CHANGE
            // (an offer heard, the plan republished) as well as on the interval.
            tick_interval: None,
        }
    }

    async fn initialize(&self, _ctx: &crate::runtime::ModuleContext) -> Result<(), String> {
        let inner = self.inner.clone();
        tokio::spawn(async move {
            let mut plan_rx = inner.plan_rx.clone();
            let mut plan_closed = false;
            let mut interval = tokio::time::interval(TICK);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            let mut panics = 0u32;
            loop {
                tokio::select! {
                    _ = interval.tick() => {}
                    changed = plan_rx.changed(), if !plan_closed => {
                        if changed.is_err() {
                            plan_closed = true; // the daemon is gone: the interval and the gossip edge remain
                        }
                    }
                    _ = crate::capacity::gossip::heard_notify().notified() => {}
                }
                match AssertUnwindSafe(async { inner.pass() }).catch_unwind().await {
                    Ok(()) => panics = 0,
                    Err(_) => {
                        panics += 1;
                        crate::probe!(
                            class = "grid.allocation.pass_panicked",
                            consecutive = panics,
                            "the allocator pass panicked — counted; quarantined after three in a row"
                        );
                        if panics >= QUARANTINE_AFTER_PANICS {
                            crate::probe!(
                                class = "grid.allocation.quarantined",
                                consecutive = panics,
                                "the allocator daemon quarantined itself: the last published allocation stands; a reboot restarts it"
                            );
                            break;
                        }
                    }
                }
            }
        });
        Ok(())
    }

    async fn handle_command(&self, command: &str, _params: serde_json::Value) -> Result<CommandResult, String> {
        match command {
            "allocator/grid" => {
                let published = current();
                let json = match published.as_deref() {
                    None => serde_json::json!({ "published": false, "passes": self.inner.passes.load(Ordering::Relaxed), "skipped": self.inner.skipped.load(Ordering::Relaxed) }),
                    Some(p) => serde_json::json!({
                        "published": true,
                        "key": p.key,
                        "at_ms": p.at_ms,
                        "this_node": p.this_node,
                        "roles": p.roles.iter().map(|r| serde_json::json!({
                            "name": r.name,
                            "minimum_window": r.requirement.window,
                            "target_window": r.requirement.target_window,
                            "min_capability": r.requirement.min_capability,
                            "decode_floor_tps": r.requirement.decode_floor_tps,
                        })).collect::<Vec<_>>(),
                        "passes": self.inner.passes.load(Ordering::Relaxed),
                        "skipped": self.inner.skipped.load(Ordering::Relaxed),
                        "nodes": p.allocation.nodes.iter().map(|n| serde_json::json!({
                            "node": n.node, "seats": n.seats, "holds": n.holds,
                            "plan": n.plan.as_ref().map(|pl| serde_json::json!({ "model": pl.model_id, "rank": pl.capability_rank, "window": pl.window, "lanes": pl.lanes, "decode_tps": pl.decode_tps_per_lane })),
                            "verdict": format!("{:?}", n.verdict),
                        })).collect::<Vec<_>>(),
                        "seated": p.allocation.seated.iter().map(|s| serde_json::json!({ "mind": s.mind, "node": s.node, "role": s.role })).collect::<Vec<_>>(),
                        "dormant_by_age": p.dormant_by_age.iter().map(|d| serde_json::json!({ "mind": d.mind, "last_turn_age_ms": d.last_turn_age_ms })).collect::<Vec<_>>(),
                        "open": p.allocation.open.iter().map(|o| serde_json::json!({ "node": o.node, "role": o.role, "count": o.count })).collect::<Vec<_>>(),
                        "clip_interval_ms": p.clip_interval_ms,
                        "roster_here": p.roster_here().map(|r| serde_json::json!({ "seated": r.seated, "open": r.open })),
                    }),
                };
                Ok(CommandResult::Json(json))
            }
            other => Err(format!("grid_allocator: unknown command '{other}' — try 'allocator/grid'")),
        }
    }

    fn command_schemas(&self) -> Vec<CommandSchema> {
        vec![CommandSchema {
            name: "allocator/grid",
            description: "The grid allocation as the daemon last published it: what each node serves, who is seated where, the dormant order, and the open seats",
            params: vec![],
        }]
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(model: &str, cap: u8, window: u32, lanes: u32) -> LanePlan {
        LanePlan { model_id: model.into(), capability_rank: cap, window, lanes, decode_tps_per_lane: None }
    }
    fn facts(this_node: Uuid, local: Option<LanePlan>, peers: Vec<PeerFacts>, minds: Vec<Uuid>) -> GridFacts {
        GridFacts {
            now_ms: 100_000,
            this_node,
            local_plan: local,
            local_lanes: 2,
            local_free_slots: 1,
            peers,
            minds,
            citizens: vec![CitizenRecipe { role: crate::persona::role_template::RoleId::Coder, requirement: None }],
            undeclared_window: Some(70_000),
            hold: None,
        }
    }
    fn peer(node: Uuid, plan: Option<LanePlan>, residents: u32, heard_at_ms: u64) -> PeerFacts {
        let lanes = plan.as_ref().map(|p| p.lanes).unwrap_or(0);
        PeerFacts { node, plan, residents, heard_at_ms, free_slots_live: 1, lanes }
    }

    // what this catches (card 10bba591): an UNCHANGED grid publishes nothing — the key
    // gate. Two passes over the same facts hash the same and the second is a skip; a new
    // node's offer moves the key; a repeated beacon from the same node does not.
    #[test]
    fn an_unchanged_grid_keys_the_same_and_a_new_offer_moves_the_key() {
        let me = Uuid::new_v4();
        let minds: Vec<Uuid> = (0..3).map(|_| Uuid::new_v4()).collect();
        let f = facts(me, Some(plan("27b", 9, 70_000, 2)), vec![], minds.clone());
        let mut book = OfferBook::default();
        let k1 = inputs_key(&build_inputs(&f, &mut book));
        let k2 = inputs_key(&build_inputs(&f, &mut book));
        assert_eq!(k1, k2, "the same grid keys the same: the gate skips");
        let gpu = Uuid::new_v4();
        let joined = facts(me, Some(plan("27b", 9, 70_000, 2)), vec![peer(gpu, Some(plan("27b", 9, 131_072, 3)), 0, 99_000)], minds.clone());
        let k3 = inputs_key(&build_inputs(&joined, &mut book));
        assert_ne!(k3, k1, "a node joining is a change");
        let again = facts(me, Some(plan("27b", 9, 70_000, 2)), vec![peer(gpu, Some(plan("27b", 9, 131_072, 3)), 0, 99_500)], minds);
        assert_eq!(inputs_key(&build_inputs(&again, &mut book)), k3, "a repeated beacon is silence");
    }

    // what this catches: a peer's own RESIDENTS fill its seats before ours spill there —
    // a 4-resident 2-lane peer offers no open seat, not four; and a beacon with no width
    // or no model offers no plan at all (heard, not a seat).
    #[test]
    fn a_peers_residents_fill_its_seats_first_and_a_widthless_beacon_is_no_seat() {
        let me = Uuid::new_v4();
        let full = Uuid::new_v4();
        let f = facts(me, Some(plan("27b", 9, 70_000, 2)), vec![peer(full, Some(plan("27b", 9, 131_072, 2)), 4, 99_000)], (0..6).map(|_| Uuid::new_v4()).collect());
        let a = allocate(&build_inputs(&f, &mut OfferBook::default()));
        assert_eq!(a.seated.iter().filter(|s| s.node == full).count(), 4, "their four residents sit at home");
        assert_eq!(a.seated.iter().filter(|s| s.node == me).count(), 4, "our four seats");
        assert_eq!(a.dormant.len(), 2, "two of ours wait: the peer is full");
        assert_eq!(a.open_total(), 0);
        let blind = facts(me, Some(plan("27b", 9, 70_000, 2)), vec![peer(Uuid::new_v4(), None, 0, 99_000)], vec![]);
        let i = build_inputs(&blind, &mut OfferBook::default());
        assert_eq!(i.nodes.len(), 2);
        assert!(i.nodes.iter().any(|n| n.plans.is_empty()), "heard, offering nothing placeable");
        let a = allocate(&i);
        assert_eq!(a.open_total(), 4, "only this node's seats are open");
    }

    // what this catches: this node without a fitting plan is not an offer (a node serving
    // nothing offers nothing), and a silent peer drops out of the live offers.
    #[test]
    fn a_planless_node_and_a_silent_peer_are_not_offers() {
        let me = Uuid::new_v4();
        let gpu = Uuid::new_v4();
        let mut book = OfferBook::default();
        let f = facts(me, None, vec![peer(gpu, Some(plan("27b", 9, 131_072, 3)), 0, 100_000 - SILENT_AFTER_MS - 1)], vec![Uuid::new_v4()]);
        let i = build_inputs(&f, &mut book);
        assert!(i.nodes.is_empty(), "no plan here, the peer silent: nobody live");
        assert_eq!(allocate(&i).dormant.len(), 1);
        let fresh = facts(me, None, vec![peer(gpu, Some(plan("27b", 9, 131_072, 3)), 0, 99_000)], vec![Uuid::new_v4()]);
        let i = build_inputs(&fresh, &mut book);
        assert_eq!(i.nodes.len(), 1);
        assert_eq!(i.nodes[0].node, gpu);
        assert_eq!(allocate(&i).seat_of(fresh.minds[0]), Some(gpu), "she seats on the peer: residents exist between the grid");
    }

    // what this catches: the dormant order is OLDEST last turn first (never turned = since
    // boot, the oldest of all), and the clip interval is derived — typical turn × dormant
    // over the slack — never a constant; no slack or no measurement = no interval.
    #[test]
    fn the_dormant_order_is_oldest_first_and_the_clip_interval_is_derived_from_slack() {
        let (a, b, c) = (Uuid::from_u128(1), Uuid::from_u128(2), Uuid::from_u128(3));
        let last = |m: Uuid| match m.as_u128() {
            1 => Some(90_000),
            2 => Some(50_000),
            _ => None,
        };
        let order: Vec<Uuid> = dormant_by_age(&[a, b, c], 100_000, 10_000, last).into_iter().map(|d| d.mind).collect();
        assert_eq!(order, vec![c, b, a], "never turned (since boot) first, then the staler turn");
        let n = Uuid::new_v4();
        let open = vec![OpenSeats { node: n, owner: ONE_OWNER, role: 0, count: 2 }];
        assert_eq!(slack_seats(&open, |_| 0.5), 1.0, "two open seats half idle = one seat of slack");
        assert_eq!(clip_interval_ms(4, 1.0, Some(60_000)), Some(240_000), "four dormant share one seat: a turn each per four turns");
        assert_eq!(clip_interval_ms(4, 2.0, Some(60_000)), Some(120_000));
        assert_eq!(clip_interval_ms(0, 2.0, Some(60_000)), None, "nobody dormant");
        assert_eq!(clip_interval_ms(4, 0.0, Some(60_000)), None, "no slack");
        assert_eq!(clip_interval_ms(4, 2.0, None), None, "no turn measured: an absence, never a number");
    }

    // what this catches: the exclusive hold is an INPUT (the box's roster is its names),
    // and the roster the spawner reads is seated + open for this node.
    #[test]
    fn the_hold_is_an_input_and_the_roster_here_is_seated_plus_open() {
        let me = Uuid::new_v4();
        let minds: Vec<Uuid> = (0..2).map(|_| Uuid::new_v4()).collect();
        let mut f = facts(me, Some(plan("27b", 9, 70_000, 2)), vec![], minds.clone());
        let a = allocate(&build_inputs(&f, &mut OfferBook::default()));
        assert_eq!(a.roster_on(me, ONE_OWNER), Some(GridRoster { seated: 2, open: 2 }));
        f.hold = Some((vec![minds[1]], true));
        let a = allocate(&build_inputs(&f, &mut OfferBook::default()));
        assert_eq!(a.roster_on(me, ONE_OWNER), Some(GridRoster { seated: 1, open: 0 }), "a held box seats its name and mints nothing");
        assert_eq!(a.seat_of(minds[1]), Some(me));
        assert_eq!(a.dormant, vec![minds[0]]);
    }
}
