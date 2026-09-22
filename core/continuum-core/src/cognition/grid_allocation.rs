//! THE GRID ALLOCATES ITS MINDS (card 2eec3977; Joel, 2026-09-20: "Grid size is dynamic.
//! Residents exist between the grid. You need daemons and recipes that are big picture" /
//! "why do we have 25 persona spawned … What even determines this? … run through all
//! computer scenarios and test that our system delivers competency").
//!
//! What determined the population before this file: `identities_available()` — every
//! identity on disk came online (#432), then the draw was bounded by this node's warm
//! slots (card 7c38ff6f). Nothing derived the population from what the GRID can seat, no
//! one decided which node a mind belongs on, and a node that joined or dropped changed
//! nothing but its own draw. Minds accumulated as cruft; seats were never counted.
//!
//! PURE. One function, [`allocate`], over three DECLARED inputs — the [`Role`]s the
//! operator's society needs (each a [`Requirement`]: window, capability, decode floor),
//! the [`Mind`]s that exist (each with a role and a home), and what every node can run
//! right now (a [`NodeOffer`]: the [`LanePlan`]s its own serving planner already computed
//! — model, window, lanes, measured decode) — returns what each node serves, who is seated
//! where, who is dormant, and how many seats stand open (the spawn signal). Nothing here
//! reads a global, spawns a process or holds a constant of Joel's grid: a node that joins
//! is one more offer; a node that drops is one fewer; a bigger box is a better plan in its
//! offer. The daemon that feeds it holds the offers as they arrive and calls this once per
//! change, never per tick (Joel: "find something once and pass it along").
//!
//! The objective, in order (the frame on card 2eec3977):
//!   1. EVERY SEATED MIND AT ITS ROLE'S REQUIREMENT. A plan below a requirement seats
//!      nobody for that role — a 2,048-token lane is not a seat (Joel: "2k is a useless
//!      persona"). A node hosts the highest-priority role it can hold at requirement:
//!      coders go where the capability is, weak nodes host orchestration phenotypes.
//!   2. CAPABILITY — among holding plans the most capable model ("I need my 27b").
//!   3. TARGET FIT — approach measured demand within that capability, when known.
//!   4. SEATS — then the plan with the most lanes.
//!   5. WINDOW — then the largest window.
//! Minds beyond the seats stay DORMANT with identity and memory intact; seats beyond the
//! minds are OPEN — the count the spawner may mint. A mind is seated at home when home has
//! a seat for its role, else on any node that has one: residents exist between the grid.
//!
//! Two more declared inputs (card 426a26fb): an operator's roster [`Hold`] on a node and a
//! recipe's declared team as a [`RoleFloor`] per role. Both are inputs to THIS decision,
//! never something a consumer applies after it — the spawner used to cap its own draw by
//! the hold and skip held-out names at the draw, a second decision about the same seats.

use uuid::Uuid;

/// What a role needs from a lane before one of its minds may sit on it. Declared by the
/// society (a recipe), with measured sizing kept separate from admission.
#[derive(Clone, Debug, PartialEq)]
pub struct Requirement {
    /// Hard minimum declared by the recipe, or the bootstrap working-set floor when undeclared.
    pub window: u32,
    /// Measured sizing target, not permission to exclude every feasible seat.
    /// Among equally capable plans, prefer approaching this target before adding lanes.
    pub target_window: Option<u32>,
    /// The least capable model this role is competent on (`ModelFootprint::capability_rank`).
    pub min_capability: u8,
    /// Decode below this is not a seat (the knee). `None` = the role does not care.
    pub decode_floor_tps: Option<f32>,
}

/// A role in priority order: index 0 is seated first and hosted by every node that can.
#[derive(Clone, Debug)]
pub struct Role {
    pub name: String,
    pub requirement: Requirement,
}

/// A mind that exists (on some disk): its role and the node its memory lives on.
#[derive(Clone, Debug)]
pub struct Mind {
    pub id: Uuid,
    /// Index into `GridInputs::roles`.
    pub role: usize,
    pub home: Option<Uuid>,
    /// The human (or org) this mind belongs to. On one operator's grid every mind and
    /// node share one owner; a peer another human brings is the case below.
    pub owner: Uuid,
}

/// The TERMS a node is offered on — the outcome of the negotiation between owners,
/// declared as data (Joel, 2026-09-20: "peers which are different humans and therefore
/// their own persona … all works the same, just a matter of negotiation amongst a
/// different kind of peer"). Between one owner's own nodes the terms are open and never
/// consulted. A foreign owner's node lends at most `seats_lent` seats, for `roles` only
/// (`None` = any); its own minds are never bounded by its own terms.
#[derive(Clone, Debug, PartialEq)]
pub struct OfferTerms {
    /// Seats lent to minds of OTHER owners; `None` = unbounded, `Some(0)` = none.
    pub seats_lent: Option<u32>,
    /// Roles the owner will host for others; `None` = any.
    pub roles: Option<Vec<usize>>,
}

/// A node with NO declared terms lends nothing to other owners (Cormac's note on #4263:
/// an unlimited loan is never the default). Between one owner's own nodes terms are not
/// consulted, so this default only ever binds a foreign node that declared none.
impl Default for OfferTerms {
    fn default() -> Self {
        Self { seats_lent: Some(0), roles: None }
    }
}

impl OfferTerms {
    /// Everything lent — what one owner's own nodes read as, whether or not consulted.
    pub fn open() -> Self {
        Self { seats_lent: None, roles: None }
    }
    pub fn admits_role(&self, role: usize) -> bool {
        self.roles.as_ref().is_none_or(|r| r.contains(&role))
    }
}

/// One thing a node can run: the shape its own planner computed for one candidate model.
#[derive(Clone, Debug, PartialEq)]
pub struct LanePlan {
    pub model_id: String,
    pub capability_rank: u8,
    pub window: u32,
    pub lanes: u32,
    /// Measured per-lane decode at this lane count; `None` = never measured (an absence
    /// is not a number: it does not refuse the plan).
    pub decode_tps_per_lane: Option<f32>,
}

impl LanePlan {
    /// A node's published plan, read as an offer. The decode measurement lives outside
    /// the plan (the knee record); the caller passes what it has.
    pub fn of(plan: &super::serving_plan::ServingPlan, decode_tps_per_lane: Option<f32>) -> Self {
        Self {
            model_id: plan.base_model.model_id.clone(),
            capability_rank: plan.base_model.capability_rank,
            window: plan.served_context_window,
            lanes: plan.lanes,
            decode_tps_per_lane,
        }
    }

    /// THIS NODE'S OWN ROW, read from what it is actually SERVING — the same three
    /// numbers its capacity beacon publishes (`grid_capacity`: `active_model`, `lanes`,
    /// `served_context_window`), so the allocator judges this node by exactly what every
    /// peer judges it by.
    ///
    /// The defect this closes (Astra, Windows 2026-09-22, card TBD-on-the-PR): the local
    /// row came from the PLAN (`plan_rx`, an INTENT) while every foreign row came from the
    /// peer's SERVED geometry. A node planning 2 lanes × 5,533 while its lane actually
    /// served 1 × 124,160 judged itself `BelowEveryRequirement` and offered zero seats —
    /// with live capacity standing right there, and its own beacon telling the grid so.
    /// One quantity, two meanings, in one comparison.
    ///
    /// It takes NO plan, by signature: whether a proposed layout fits the GPU is an
    /// admission-policy statement about a future lane, and gating the live row on it
    /// would re-hide a healthy server behind an infeasible intent — the same defect in
    /// new clothes (Astra's review question on this change). What the box serves now is a
    /// seat because it is running.
    ///
    /// `None` when nothing is served (no model, no lanes, or an unknown window): an
    /// absence, never a substituted intent and never fabricated capacity.
    pub fn serving(
        snapshot: &crate::inference::llama_server::ServingSnapshot,
        capability_rank: u8,
        decode_tps_per_lane: Option<f32>,
    ) -> Option<Self> {
        let model_id = snapshot.active_model.clone()?;
        if snapshot.lanes == 0 || snapshot.served_context_window == 0 {
            return None;
        }
        Some(Self {
            model_id,
            capability_rank,
            window: snapshot.served_context_window,
            lanes: snapshot.lanes,
            decode_tps_per_lane,
        })
    }

    /// Does one lane of this plan seat a mind of a role with this requirement?
    pub fn holds(&self, req: &Requirement) -> bool {
        self.lanes > 0
            && self.window >= req.window
            && self.capability_rank >= req.min_capability
            && match (self.decode_tps_per_lane, req.decode_floor_tps) {
                (Some(tps), Some(floor)) => tps >= floor,
                _ => true,
            }
    }

    /// Objective 2–4: capability, then lanes, then window.
    pub fn rank(&self) -> (u8, u32, u32) {
        (self.capability_rank, self.lanes, self.window)
    }

    /// The single seat order shared by allocation and between-turn migration.
    pub fn seat_key(&self, req: &Requirement) -> (u8, u32, u32, u32) {
        (self.capability_rank, req.target_window.map_or(0, |target| self.window.min(target)), self.lanes, self.window)
    }

    /// A strictly better eligible seat, and the first improving axis.
    pub fn better_than(&self, other: &LanePlan, req: &Requirement) -> Option<BetterBy> {
        let (next, current) = (self.seat_key(req), other.seat_key(req));
        if next <= current {
            return None;
        }
        Some(if next.0 != current.0 {
            BetterBy::Capability
        } else if next.1 != current.1 {
            BetterBy::Window
        } else if next.2 != current.2 {
            BetterBy::Lanes
        } else {
            BetterBy::Window
        })
    }
}

/// Why one seat beats another — the first axis, in the allocator's order, on which
/// the better plan wins. `Requirement` is objective 1 (a plan that holds her role's
/// requirement beats a node that has none): the placement switch's reason when her
/// current node seats nobody of her role.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BetterBy {
    Requirement,
    Capability,
    Lanes,
    Window,
}

impl BetterBy {
    pub fn as_str(self) -> &'static str {
        match self {
            BetterBy::Requirement => "requirement",
            BetterBy::Capability => "capability",
            BetterBy::Lanes => "lanes",
            BetterBy::Window => "window",
        }
    }
}

/// What one node can run right now: every runnable plan its planner computed.
#[derive(Clone, Debug)]
pub struct NodeOffer {
    pub node: Uuid,
    /// Who brings this node. Its own minds seat freely; others within `terms`.
    pub owner: Uuid,
    pub terms: OfferTerms,
    pub plans: Vec<LanePlan>,
}

/// The offers this node has heard, by node: a node joins when its first beacon lands, is
/// LIVE while a beacon is fresher than `silent_after_ms`, and drops out when it goes
/// silent — the same three events `fold_liveness` names for the fleet, folded here into
/// the allocator's inputs. Held by the daemon; `live` is what `allocate` sees. PURE.
#[derive(Clone, Debug, Default)]
pub struct OfferBook {
    heard: std::collections::BTreeMap<Uuid, (u64, NodeOffer)>,
}

impl OfferBook {
    pub fn hear(&mut self, offer: NodeOffer, now_ms: u64) {
        self.heard.insert(offer.node, (now_ms, offer));
    }
    /// The offers fresher than `silent_after_ms`, ordered by node id so two nodes
    /// computing the same allocation agree (Cormac's note on #4259).
    pub fn live(&self, now_ms: u64, silent_after_ms: u64) -> Vec<NodeOffer> {
        self.heard
            .values()
            .filter(|(heard_at, _)| now_ms.saturating_sub(*heard_at) <= silent_after_ms)
            .map(|(_, o)| o.clone())
            .collect()
    }
    /// Drop the offers that have gone silent; returns the nodes that left.
    pub fn forget_silent(&mut self, now_ms: u64, silent_after_ms: u64) -> Vec<Uuid> {
        let gone: Vec<Uuid> = self
            .heard
            .iter()
            .filter(|(_, (heard_at, _))| now_ms.saturating_sub(*heard_at) > silent_after_ms)
            .map(|(n, _)| *n)
            .collect();
        for n in &gone {
            self.heard.remove(n);
        }
        gone
    }
}

/// A fingerprint of the allocator's inputs: the daemon recomputes only when it changes
/// (a beacon repeating the same offer is silence, not work — Joel: "find something once").
/// IN-PROCESS ONLY: `DefaultHasher` is not stable across builds; never persist or compare
/// this across nodes.
pub fn inputs_key(inputs: &GridInputs) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    inputs.minds_per_lane.hash(&mut h);
    for r in &inputs.roles {
        r.name.hash(&mut h);
        r.requirement.window.hash(&mut h);
        r.requirement.target_window.hash(&mut h);
        r.requirement.min_capability.hash(&mut h);
        r.requirement.decode_floor_tps.map(f32::to_bits).hash(&mut h);
    }
    for m in &inputs.minds {
        (m.id, m.role, m.home, m.owner).hash(&mut h);
    }
    for n in &inputs.nodes {
        (n.node, n.owner, n.terms.seats_lent, &n.terms.roles).hash(&mut h);
        for p in &n.plans {
            (&p.model_id, p.capability_rank, p.window, p.lanes, p.decode_tps_per_lane.map(f32::to_bits)).hash(&mut h);
        }
    }
    for hold in &inputs.holds {
        (hold.node, &hold.only, hold.exclusive).hash(&mut h);
    }
    for f in &inputs.floors {
        (f.role, f.min_seats).hash(&mut h);
    }
    h.finish()
}

/// The three declared inputs plus the one society-wide ratio (minds a warm lane serves,

/// An operator's roster hold on one node (`persona::roster_hold`, the names resolved to
/// mind ids by whoever builds the inputs). The hold is an INPUT to the allocation, not
/// something a consumer applies after it (Cormac's note on #4259): the spawner used to
/// cap its own draw by the hold (`seats_under`) and skip held-out identities at the draw,
/// a second decision about the same seats in a second place.
///
/// EXCLUSIVE (the operator's file): on `node` only the named minds sit, and the node has
/// at most `only.len()` seats — the hold DEFINES that node's roster, so nothing is minted
/// for a held box. An exclusive hold naming nobody restricts nothing (the file's own
/// semantics in `seats_under`). DERIVED (a working round's team, not exclusive): the named
/// minds seat FIRST on that node and everyone else after — never fewer minds.
#[derive(Clone, Debug, PartialEq)]
pub struct Hold {
    pub node: Uuid,
    pub only: Vec<Uuid>,
    pub exclusive: bool,
}

/// A recipe's declared team: at least `min_seats` of this role are seated before any
/// higher-priority role takes the rest of the grid, and a shortfall in minds of that
/// role is reported OPEN for it before the node's best role — the team is the spawn
/// signal's first claim. A floor of 0 (or none declared) is today's allocation exactly.
#[derive(Clone, Debug, PartialEq)]
pub struct RoleFloor {
    /// Index into `GridInputs::roles`.
    pub role: usize,
    pub min_seats: u32,
}

/// The declared inputs plus the one society-wide ratio (minds a warm lane serves,
/// `citizen_health::MINDS_PER_LANE_STARVED_ABOVE` on the live path — passed, not read).
#[derive(Clone, Debug)]
pub struct GridInputs {
    pub roles: Vec<Role>,
    pub minds: Vec<Mind>,
    pub nodes: Vec<NodeOffer>,
    pub minds_per_lane: u32,
    /// Standing roster holds, at most one per node (a later one for the same node wins).
    pub holds: Vec<Hold>,
    /// The declared team's minimums, at most one per role.
    pub floors: Vec<RoleFloor>,
}

impl GridInputs {
    fn hold_on(&self, node: Uuid) -> Option<&Hold> {
        self.holds.iter().rev().find(|h| h.node == node)
    }
    /// The node a hold names this mind on, if any — where she sits first.
    fn held_at(&self, mind: Uuid) -> Option<Uuid> {
        self.holds.iter().find(|h| h.only.contains(&mind)).map(|h| h.node)
    }
    fn floor_of(&self, role: usize) -> u32 {
        self.floors.iter().filter(|f| f.role == role).map(|f| f.min_seats).max().unwrap_or(0) // unwrap_or: no floor declared = 0, the same law
    }
}

/// Why a node serves what it serves.
#[derive(Clone, Debug, PartialEq)]
pub enum NodeVerdict {
    /// The node hosts this role (index into roles) on its chosen plan.
    Hosts { role: usize },
    /// Every runnable plan is below every role's requirement — the node seats nobody.
    /// `best_window`/`best_capability` say how far off it is.
    BelowEveryRequirement { best_window: u32, best_capability: u8 },
    /// The node offered no runnable plan at all.
    NothingRunnable,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NodeAllocation {
    pub node: Uuid,
    pub plan: Option<LanePlan>,
    /// The highest-priority role the chosen plan holds (the plan was chosen for it).
    pub verdict: NodeVerdict,
    /// EVERY role the chosen plan holds, in priority order — one engine at one window
    /// seats every role whose requirement is at or below it (Cormac's condition on
    /// #4259: a 27B at 2 × 70k holds coders AND orchestrators; seating coders only left
    /// its seats open for coders that did not exist while orchestrators went dormant).
    pub holds: Vec<usize>,
    /// lanes × minds_per_lane when hosting, else 0. Shared by every role in `holds`.
    pub seats: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Seat {
    pub mind: Uuid,
    pub node: Uuid,
    pub role: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OpenSeats {
    pub node: Uuid,
    /// Whose seats these are: a spawner mints only into its OWN owner's open seats — a
    /// lender's spare seats are theirs to fill (Cormac's condition on #4263: without this
    /// our spawner minted two coders for a loan already spent).
    pub owner: Uuid,
    pub role: usize,
    pub count: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GridAllocation {
    pub nodes: Vec<NodeAllocation>,
    pub seated: Vec<Seat>,
    /// Minds with no seat anywhere on the grid: identity and memory kept, no lane.
    pub dormant: Vec<Uuid>,
    /// The subset of [`Self::dormant`] whose ROLE no node on this grid can serve — no
    /// node's plans meet the role's hard gate, so there was never a seat to wait for. As
    /// distinct from the ones the grid merely had no ROOM for right now.
    ///
    /// THIS IS NOT THE BIG-MIND CASE, and the name says so deliberately (Cormac on
    /// #4314). Since #4296 an UNDECLARED role's hard gate is the serve floor
    /// (`BOOTSTRAP_WORKING_SET`) with the measured demand as a soft target, so every node
    /// holds such a role: a mind whose TURN overflows every lane is SEATED, never
    /// dormant, and never appears here. That mind is named by `grid.mind.unservable` at
    /// the daemon, where her own measured requirement meets the grid's widest offered
    /// window. This field is the DECLARED-role fact: a role the grid cannot host at all.
    ///
    /// Both cases read as "dormant" and they want opposite responses: a mind with
    /// nowhere to sit needs a wider lane, a smaller context, or an accepted slow clip
    /// ([[dormant-is-not-off-every-mind-gets-a-slow-clip-at-any-grid-size]]); a mind
    /// waiting for room needs patience or more hardware. Measured 2026-09-21: Benchy's
    /// SMALLEST recent prompt (76,952) exceeded the widest lane on the entire grid
    /// (75,776), so every refusal was individually correct and nothing said the one
    /// sentence a human or a peer needed — she read as a citizen who produces nothing
    /// rather than a citizen with nowhere to sit (card b8503234).
    pub role_unservable: Vec<Uuid>,
    /// Seats no existing mind fills — what the spawner may mint, per node and role.
    pub open: Vec<OpenSeats>,
}

/// One node's roster as the allocation reads it: the minds seated there and the seats
/// nobody fills — what the spawner on that node may draw (`seated + open`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GridRoster {
    pub seated: u32,
    pub open: u32,
}

impl GridRoster {
    pub fn seats(self) -> u32 {
        self.seated.saturating_add(self.open)
    }
}

impl GridAllocation {
    pub fn node(&self, id: Uuid) -> Option<&NodeAllocation> {
        self.nodes.iter().find(|n| n.node == id)
    }
    /// The roster on `node` for `owner`'s spawner: how many are seated there and how many
    /// seats stand open for that owner to mint. `None` when the allocation never saw the
    /// node (it made no offer) — an absence, so the caller keeps its own prior.
    pub fn roster_on(&self, node: Uuid, owner: Uuid) -> Option<GridRoster> {
        self.node(node)?;
        Some(GridRoster {
            seated: self.seated.iter().filter(|s| s.node == node).count() as u32,
            open: self.open.iter().filter(|o| o.node == node && o.owner == owner).map(|o| o.count).sum(),
        })
    }
    /// The plan the node a mind is seated on serves — what her seat IS, for a comparison.
    pub fn plan_of_seat(&self, mind: Uuid) -> Option<&LanePlan> {
        self.node(self.seat_of(mind)?)?.plan.as_ref()
    }
    pub fn seat_of(&self, mind: Uuid) -> Option<Uuid> {
        self.seated.iter().find(|s| s.mind == mind).map(|s| s.node)
    }
    /// Seats a role can sit in: every node whose plan holds it (shared with the other
    /// roles that node holds).
    pub fn seats_for(&self, role: usize) -> u32 {
        self.nodes.iter().filter(|n| n.holds.contains(&role)).map(|n| n.seats).sum()
    }
    pub fn open_total(&self) -> u32 {
        self.open.iter().map(|o| o.count).sum()
    }
}

/// The roles and floors an experience DECLARES, in the allocator's terms — `GridInputs`'
/// roles and floors from data, never from a constant. Roles are ordered by first
/// appearance (authoring order is priority); a role's floor is how many citizens the
/// recipe lists for it. An undeclared citizen retains the serving floor as its hard
/// minimum; measured demand is a sizing target. A large thought must not turn every
/// node into a refused seat. Declared recipe requirements remain hard constraints.
pub fn roles_from(
    citizens: &[crate::experience::recipe::CitizenRecipe],
    measured_target: Option<u32>,
) -> (Vec<Role>, Vec<RoleFloor>) {
    let mut roles: Vec<Role> = Vec::new();
    let mut floors: Vec<RoleFloor> = Vec::new();
    for c in citizens {
        let name = c.role.as_str();
        let requirement = match &c.requirement {
            Some(r) => Requirement {
                window: r.window_tokens,
                target_window: None,
                min_capability: r.min_capability,
                decode_floor_tps: r.decode_floor_tps,
            },
            None => Requirement {
                window: super::serving_plan::BOOTSTRAP_WORKING_SET,
                target_window: measured_target,
                min_capability: 0,
                decode_floor_tps: None,
            },
        };
        match roles.iter().position(|r| r.name == name) {
            Some(i) => {
                // A second citizen of a role adds a seat to its floor; a declared
                // requirement on any of them is the role's (the strictest window wins).
                floors[i].min_seats += 1;
                if requirement.window > roles[i].requirement.window {
                    roles[i].requirement.window = requirement.window;
                }
                roles[i].requirement.target_window = roles[i].requirement.target_window.max(requirement.target_window);
                roles[i].requirement.min_capability = roles[i].requirement.min_capability.max(requirement.min_capability);
                // The strictest on EVERY axis (Cormac, #4271): a later, laxer decode floor
                // never loosens the role's.
                roles[i].requirement.decode_floor_tps = match (roles[i].requirement.decode_floor_tps, requirement.decode_floor_tps) {
                    (Some(a), Some(b)) => Some(a.max(b)),
                    (a, b) => a.or(b),
                };
            }
            None => {
                roles.push(Role { name: name.to_string(), requirement });
                floors.push(RoleFloor { role: roles.len() - 1, min_seats: 1 });
            }
        }
    }
    (roles, floors)
}

/// The best plan on a node for a requirement: the most capable holding plan, then the
/// closest measured window target within that capability, then most lanes and width.
/// Only declared minima gate admission. `None` when no plan holds those minima.
pub fn best_plan_for<'a>(plans: &'a [LanePlan], req: &Requirement) -> Option<&'a LanePlan> {
    plans.iter().filter(|p| p.holds(req)).max_by_key(|p| p.seat_key(req))
}

/// What each node serves: its best plan for the highest-priority role it can hold (so
/// coders stay on the capability), and every role that plan holds fills its seats.
fn decide_node(offer: &NodeOffer, roles: &[Role], minds_per_lane: u32, hold: Option<&Hold>) -> NodeAllocation {
    for (role, r) in roles.iter().enumerate() {
        if let Some(plan) = best_plan_for(&offer.plans, &r.requirement) {
            let holds = roles
                .iter()
                .enumerate()
                .filter(|(_, r)| plan.holds(&r.requirement))
                .map(|(i, _)| i)
                .collect();
            let warm = plan.lanes.saturating_mul(minds_per_lane);
            // An exclusive hold naming anyone DEFINES this node's roster: its seats.
            let seats = match hold {
                Some(h) if h.exclusive && !h.only.is_empty() => warm.min(h.only.len() as u32),
                _ => warm,
            };
            return NodeAllocation {
                node: offer.node,
                seats,
                plan: Some(plan.clone()),
                verdict: NodeVerdict::Hosts { role },
                holds,
            };
        }
    }
    let verdict = match offer.plans.iter().max_by_key(|p| p.rank()) {
        Some(best) => NodeVerdict::BelowEveryRequirement {
            best_window: offer.plans.iter().map(|p| p.window).max().unwrap_or(0), // unwrap_or: unreachable, `best` exists
            best_capability: best.capability_rank,
        },
        None => NodeVerdict::NothingRunnable,
    };
    NodeAllocation { node: offer.node, plan: None, verdict, holds: Vec::new(), seats: 0 }
}

/// The allocation. Deterministic: nodes in offer order, minds in their given order.
///
/// Minds are seated in BATCHES: first every role's declared floor (in role order), then
/// every role's remainder (in role order) — so a declared team is seated before a
/// higher-priority role takes the rest, and with no floors the batches collapse to the
/// plain role order. Within a role, the minds a hold names come first (a derived hold
/// seats its team before anyone else), and a hold's node is where a named mind sits
/// first. An exclusive hold admits only its names on its node.
pub fn allocate(inputs: &GridInputs) -> GridAllocation {
    let nodes: Vec<NodeAllocation> = inputs
        .nodes
        .iter()
        .map(|o| decide_node(o, &inputs.roles, inputs.minds_per_lane, inputs.hold_on(o.node)))
        .collect();
    // Free seats per node, drawn down as minds are seated; and the seats each node has
    // LENT to other owners' minds, bounded by its terms.
    let mut free: Vec<u32> = nodes.iter().map(|n| n.seats).collect();
    let mut lent: Vec<u32> = vec![0; nodes.len()];
    let mut seated: Vec<Seat> = Vec::new();
    let mut dormant = Vec::new();
    let mut role_unservable = Vec::new();
    // May this mind sit on node i? ONE predicate: under an exclusive hold, only its names;
    // then her owner's own node freely, another owner's within its terms.
    let admits = |i: usize, m: &Mind, lent_now: u32| -> bool {
        let named = match inputs.hold_on(nodes[i].node) {
            Some(h) if h.exclusive && !h.only.is_empty() => h.only.contains(&m.id),
            _ => true,
        };
        let offer = &inputs.nodes[i];
        named
            && (offer.owner == m.owner
                || (offer.terms.admits_role(m.role) && offer.terms.seats_lent.is_none_or(|cap| lent_now < cap)))
    };
    // Per role, in seating order: the minds a hold names first, then the rest, stable.
    let by_role: Vec<Vec<&Mind>> = (0..inputs.roles.len())
        .map(|role| {
            let (held, rest): (Vec<&Mind>, Vec<&Mind>) =
                inputs.minds.iter().filter(|m| m.role == role).partition(|m| inputs.held_at(m.id).is_some());
            held.into_iter().chain(rest).collect()
        })
        .collect();
    // The batches: (role, the slice of its minds) — floors first, remainders after.
    let mut batches: Vec<(usize, &[&Mind])> = Vec::with_capacity(inputs.roles.len() * 2);
    for (role, minds) in by_role.iter().enumerate() {
        let floor = (inputs.floor_of(role) as usize).min(minds.len());
        batches.push((role, &minds[..floor]));
    }
    for (role, minds) in by_role.iter().enumerate() {
        let floor = (inputs.floor_of(role) as usize).min(minds.len());
        batches.push((role, &minds[floor..]));
    }
    for (role, minds) in batches {
        // Every node whose chosen plan holds this role — not only the one it was chosen
        // for: seats are the node's, and roles fill them in priority order.
        let hosts: Vec<usize> = nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.holds.contains(&role))
            .map(|(i, _)| i)
            .collect();
        let mut unseated: Vec<&Mind> = Vec::new();
        // Pass 1: home has a seat for her role — she stays where her memory is. A hold
        // that names her makes its node her home first.
        for m in minds {
            let home = inputs.held_at(m.id).or(m.home);
            let at_home = home.and_then(|h| hosts.iter().copied().find(|&i| nodes[i].node == h && free[i] > 0 && admits(i, m, lent[i])));
            match at_home {
                Some(i) => {
                    free[i] -= 1;
                    if inputs.nodes[i].owner != m.owner {
                        lent[i] += 1;
                    }
                    seated.push(Seat { mind: m.id, node: nodes[i].node, role });
                }
                None => unseated.push(m),
            }
        }
        // Pass 2: any node on the grid with a free seat for her role — residents exist
        // between the grid. The node with the most free seats first, so load spreads.
        for m in unseated {
            // Her owner's own nodes first (no terms spent), then a lender's within its terms.
            let target = hosts
                .iter()
                .copied()
                .filter(|&i| free[i] > 0 && admits(i, m, lent[i]))
                .max_by_key(|&i| (inputs.nodes[i].owner == m.owner, free[i]));
            match target {
                Some(i) => {
                    free[i] -= 1;
                    if inputs.nodes[i].owner != m.owner {
                        lent[i] += 1;
                    }
                    seated.push(Seat { mind: m.id, node: nodes[i].node, role });
                }
                None => {
                    // WHY she got no seat is known HERE and was thrown away. `hosts` is
                    // the nodes whose plans MEET HER ROLE'S REQUIREMENT — the window and
                    // capability gate. Empty means no node on this grid can serve her
                    // role AT ALL, and no amount of waiting changes that. A non-empty
                    // `hosts` that still seats nobody is the grid being FULL (or a hold
                    // or lending terms refusing her, which `admits` decides) — a
                    // different fact wanting a different response.
                    if hosts.is_empty() {
                        role_unservable.push(m.id);
                    }
                    dormant.push(m.id);
                }
            }
        }
    }
    // A seat no existing mind of ANY held role fills is open: first for a declared
    // floor the grid could not fill for want of minds (the team's claim on the spawn
    // signal), then for the highest-priority role the node holds — the spawner mints
    // what the node is best at. A box under an exclusive hold is never minted for: the
    // hold names its roster. Every row carries the node's owner (#4263): a loan is never
    // minted for.
    let held = |node: Uuid| inputs.hold_on(node).is_some_and(|h| h.exclusive && !h.only.is_empty());
    let mut open: Vec<OpenSeats> = Vec::new();
    for role in 0..inputs.roles.len() {
        let have = seated.iter().filter(|s| s.role == role).count() as u32;
        let mut short = inputs.floor_of(role).saturating_sub(have);
        for (i, n) in nodes.iter().enumerate() {
            if short == 0 {
                break;
            }
            if !n.holds.contains(&role) || free[i] == 0 || held(n.node) {
                continue;
            }
            let count = free[i].min(short);
            free[i] -= count;
            short -= count;
            open.push(OpenSeats { node: n.node, owner: inputs.nodes[i].owner, role, count });
        }
    }
    for ((n, &f), offer) in nodes.iter().zip(free.iter()).zip(inputs.nodes.iter()) {
        if let (NodeVerdict::Hosts { role }, true, false) = (&n.verdict, f > 0, held(n.node)) {
            open.push(OpenSeats { node: n.node, owner: offer.owner, role: *role, count: f });
        }
    }
    GridAllocation { nodes, seated, dormant, role_unservable, open }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (Astra, Windows 2026-09-22): this node's row in the allocator
    // coming from its PLAN while every peer's comes from their SERVED beacon. A box
    // planning a 2 × 5,533 shrink while its lane actually served 1 × 124,160 read as
    // BelowEveryRequirement and offered zero seats, with live capacity standing there.
    // The local row must carry the same three numbers `grid_capacity` beacons — and must
    // be an ABSENCE, never a substituted intent, when nothing is being served.
    #[test]
    fn the_local_row_is_the_served_geometry_the_beacon_publishes_or_nothing() {
        use crate::inference::llama_server::ServingSnapshot;
        let served = ServingSnapshot {
            active_model: Some("qwen3.8-27b".into()),
            lanes: 1,
            served_context_window: 124_160,
            ..ServingSnapshot::empty()
        };
        let row = LanePlan::serving(&served, 9, Some(20.0)).expect("a served lane is a row");
        assert_eq!((row.lanes, row.window), (1, 124_160), "the beacon's numbers, not the plan's");
        assert!(row.holds(&coder().requirement), "a 124k lane seats the 64k role");

        // The intent that caused the misjudgement: the same box planning a shrink. Its
        // row must NOT be built from these numbers.
        let planned_shrink = ServingSnapshot {
            active_model: Some("qwen3.8-27b".into()),
            lanes: 2,
            served_context_window: 5_533,
            ..ServingSnapshot::empty()
        };
        let shrunk = LanePlan::serving(&planned_shrink, 9, Some(20.0)).expect("still a row");
        assert!(!shrunk.holds(&coder().requirement), "5,533 below 65,536 is the reading that starved the node");

        // Absence, not fabrication: nothing served ⇒ no row, on each of the three ways
        // "nothing" arrives.
        assert!(LanePlan::serving(&ServingSnapshot::empty(), 9, None).is_none(), "no model = no row");
        assert!(
            LanePlan::serving(&ServingSnapshot { active_model: Some("m".into()), lanes: 0, served_context_window: 65_536, ..ServingSnapshot::empty() }, 9, None).is_none(),
            "zero lanes = no row"
        );
        assert!(
            LanePlan::serving(&ServingSnapshot { active_model: Some("m".into()), lanes: 2, served_context_window: 0, ..ServingSnapshot::empty() }, 9, None).is_none(),
            "unknown window = no row, never a guess"
        );
    }

    // ---- fixtures: shapes, not Joel's grid. Numbers are illustrative machine classes. ----
    fn coder() -> Role {
        Role {
            name: "coder".into(),
            requirement: Requirement { window: 65_536, target_window: None, min_capability: 7, decode_floor_tps: Some(10.0) },
        }
    }
    fn orchestrator() -> Role {
        Role {
            name: "orchestrator".into(),
            requirement: Requirement { window: 32_768, target_window: None, min_capability: 3, decode_floor_tps: None },
        }
    }
    fn plan(model: &str, cap: u8, window: u32, lanes: u32, tps: Option<f32>) -> LanePlan {
        LanePlan { model_id: model.into(), capability_rank: cap, window, lanes, decode_tps_per_lane: tps }
    }
    const US: Uuid = Uuid::from_u128(0xA11);
    fn minds(role: usize, home: Option<Uuid>, n: usize) -> Vec<Mind> {
        (0..n).map(|_| Mind { id: Uuid::new_v4(), role, home, owner: US }).collect()
    }
    fn offer(node: Uuid, plans: Vec<LanePlan>) -> NodeOffer {
        NodeOffer { node, owner: US, terms: OfferTerms::open(), plans }
    }
    /// A 64 GB unified-memory box: the 27B at 2 lanes or 1 wide lane; a 7B at 4 lanes.
    fn big_box(node: Uuid) -> NodeOffer {
        offer(
            node,
            vec![
                plan("27b", 9, 67_072, 2, Some(14.0)),
                plan("27b", 9, 131_072, 1, Some(14.3)),
                plan("7b", 5, 131_072, 4, Some(40.0)),
            ],
        )
    }
    /// A 32 GB discrete-GPU box: the 27B at 3 lanes × 128k.
    fn gpu_box(node: Uuid) -> NodeOffer {
        offer(node, vec![plan("27b", 9, 131_072, 3, Some(30.0))])
    }
    /// A 16 GB laptop: a 7B or a 1.5B, never the 27B.
    fn small_box(node: Uuid) -> NodeOffer {
        offer(node, vec![plan("7b", 5, 32_768, 1, Some(12.0)), plan("1.5b", 2, 32_768, 3, Some(60.0))])
    }
    fn inputs(nodes: Vec<NodeOffer>, minds: Vec<Mind>) -> GridInputs {
        GridInputs { roles: vec![coder(), orchestrator()], minds, nodes, minds_per_lane: 2, holds: vec![], floors: vec![] }
    }
    fn on(a: &GridAllocation, node: Uuid) -> Vec<Uuid> {
        a.seated.iter().filter(|s| s.node == node).map(|s| s.mind).collect()
    }

    // what this catches (card b8503234, measured 2026-09-21): "no seat" had ONE name for
    // two opposite facts. Benchy's SMALLEST recent prompt (76,952) exceeded the widest
    // lane on the entire grid (75,776), so every node correctly refused her and she
    // landed in `dormant` beside minds who were merely waiting for room. Nothing said
    // the one sentence a human or a peer needed — she read as a citizen who produces
    // nothing rather than a citizen with nowhere to sit. The two want opposite
    // responses: a wider lane / less context / an accepted slow clip, versus patience.
    #[test]
    fn a_mind_with_nowhere_to_sit_is_distinguished_from_one_waiting_for_room() {
        let a_box = Uuid::new_v4();
        // THE SAME GRID BOTH TIMES — only the requirement changes, so the contrast is
        // the fact under test and not the fixture. `big_box` genuinely holds the coder
        // role (capability 9 ≥ 7, a 67k and a 131k plan), which is what makes the
        // waiting case a real wait.
        let full = allocate(&inputs(vec![big_box(a_box)], minds(0, None, 20)));
        assert!(!full.seated.is_empty(), "this role IS servable here");
        assert!(!full.dormant.is_empty(), "twenty minds, far fewer seats");
        assert!(
            full.role_unservable.is_empty(),
            "waiting for room is not unservable: {:?}",
            full.role_unservable,
        );

        // Now a window no offered plan can serve: no node HOLDS the role, `hosts` is
        // empty, and there was never a seat to wait for.
        let mut i = inputs(vec![big_box(a_box)], minds(0, None, 1));
        i.roles[0].requirement.window = 1_000_000;
        let starved = allocate(&i);
        assert!(starved.seated.is_empty(), "no plan on this grid meets her requirement");
        assert_eq!(starved.dormant.len(), 1);
        assert_eq!(
            starved.role_unservable, starved.dormant,
            "no node holds her role: unservable, never merely waiting",
        );

        // And a grid with room for everyone reports neither.
        let roomy = allocate(&inputs(vec![big_box(a_box)], minds(0, None, 1)));
        assert!(roomy.dormant.is_empty() && roomy.role_unservable.is_empty());
    }

    // what this catches (card 426a26fb): the operator's EXCLUSIVE hold as an input — on the
    // held box only its names sit and the box has only that many seats; the rest of the
    // roster goes where the grid has room; a name with no mind behind it is NOT a seat to
    // mint (the spawner used to draw the hold's count and ask the provider every second).
    #[test]
    fn an_exclusive_hold_defines_a_boxs_roster_and_nothing_is_minted_for_it() {
        let a_box = Uuid::new_v4();
        let b_box = Uuid::new_v4();
        let m = minds(0, Some(a_box), 6); // all six call the first box home
        let free = allocate(&inputs(vec![big_box(a_box), big_box(b_box)], m.clone()));
        assert_eq!(on(&free, a_box).len(), 4, "without a hold: four at home, two spill");
        assert_eq!(on(&free, b_box).len(), 2);
        let mut i = inputs(vec![big_box(a_box), big_box(b_box)], m.clone());
        i.holds.push(Hold { node: a_box, only: vec![m[4].id, m[5].id], exclusive: true });
        let held = allocate(&i);
        assert_eq!(held.node(a_box).unwrap().seats, 2, "the hold defines the roster: two seats");
        assert_eq!(on(&held, a_box), vec![m[4].id, m[5].id], "only the named sit there — and they sit FIRST, though last in order");
        assert_eq!(on(&held, b_box).len(), 4, "the other four go where the grid has room");
        assert!(held.dormant.is_empty());
        assert!(held.open.is_empty());
        // A name with no mind behind it: one seat stays empty and is NOT open to mint.
        i.holds[0].only = vec![m[4].id, Uuid::new_v4()];
        let ghost = allocate(&i);
        assert_eq!(on(&ghost, a_box), vec![m[4].id]);
        assert!(ghost.open.iter().all(|o| o.node != a_box), "a held box is never minted for");
        assert_eq!(ghost.dormant.len(), 1, "six minds, five seats (one held empty)");
        // An exclusive hold naming nobody restricts nothing (the file's own semantics).
        i.holds[0].only.clear();
        assert_eq!(on(&allocate(&i), a_box).len(), 4);
        // A hold or a floor is an input: the key moves with it (the daemon recomputes).
        let k = inputs_key(&i);
        i.holds[0].only.push(m[0].id);
        assert_ne!(inputs_key(&i), k);
        i.floors.push(RoleFloor { role: 1, min_seats: 1 });
        assert_ne!(inputs_key(&i), k);
    }

    // what this catches: a DERIVED hold (a working round's team) orders, never restricts —
    // its names seat first on its node, everyone else after, never fewer minds.
    #[test]
    fn a_derived_hold_seats_its_team_first_and_never_fewer() {
        let n = Uuid::new_v4();
        let m = minds(0, Some(n), 6);
        let mut i = inputs(vec![big_box(n)], m.clone());
        i.holds.push(Hold { node: n, only: vec![m[5].id, m[4].id], exclusive: false });
        let a = allocate(&i);
        assert_eq!(a.node(n).unwrap().seats, 4, "a derived hold does not cap the seats");
        let seated = on(&a, n);
        assert_eq!(&seated[..2], &[m[4].id, m[5].id], "the team first, stable in their own order");
        assert_eq!(&seated[2..], &[m[0].id, m[1].id], "then the rest in their order");
        assert_eq!(a.dormant, vec![m[2].id, m[3].id]);
        // A named mind whose home is elsewhere sits on the hold's node first.
        let mut away = minds(0, Some(Uuid::new_v4()), 1);
        away.extend(minds(0, Some(n), 4));
        let mut i = inputs(vec![big_box(n)], away.clone());
        i.holds.push(Hold { node: n, only: vec![away[0].id], exclusive: false });
        assert_eq!(on(&allocate(&i), n)[0], away[0].id, "the hold's node is her home first");
    }

    // what this catches: a recipe's declared team as an input — a 4-seat box, 5 coders and
    // one orchestrator: with no floor the coders take every seat (priority); with a floor of
    // one orchestrator it is 3 + 1. A floor the grid cannot fill for want of MINDS is the
    // spawn signal's first claim: open for the orchestrator before the box's best role.
    #[test]
    fn a_declared_team_is_seated_before_a_higher_role_takes_the_rest_and_claims_the_spawn_signal() {
        let n = Uuid::new_v4();
        let mut m = minds(0, Some(n), 5);
        m.extend(minds(1, Some(n), 1));
        let plain = allocate(&inputs(vec![big_box(n)], m.clone()));
        assert_eq!(plain.seated.iter().filter(|s| s.role == 1).count(), 0, "no floor: priority seats the coders");
        let mut i = inputs(vec![big_box(n)], m.clone());
        i.floors.push(RoleFloor { role: 1, min_seats: 1 });
        let a = allocate(&i);
        assert_eq!(a.seated.iter().filter(|s| s.role == 0).count(), 3);
        assert_eq!(a.seated.iter().filter(|s| s.role == 1).count(), 1);
        assert_eq!(a.dormant.len(), 2, "two coders wait");
        assert!(a.open.is_empty());
        // A floor above the minds that exist reserves nothing for nobody: 3 + 1 still.
        i.floors[0].min_seats = 3;
        let a = allocate(&i);
        assert_eq!(a.seated.iter().filter(|s| s.role == 0).count(), 3);
        assert_eq!(a.seated.iter().filter(|s| s.role == 1).count(), 1);
        assert!(a.open.is_empty(), "no free seat: the coders that exist take them");
        // Two coders, no orchestrator, floor one: the open seats name the orchestrator FIRST.
        let mut i = inputs(vec![big_box(n)], minds(0, Some(n), 2));
        i.floors.push(RoleFloor { role: 1, min_seats: 1 });
        let a = allocate(&i);
        assert_eq!(a.open, vec![OpenSeats { node: n, owner: US, role: 1, count: 1 }, OpenSeats { node: n, owner: US, role: 0, count: 1 }]);
        // A floor for a role no node holds opens nothing (the small box cannot seat a coder).
        let s = Uuid::new_v4();
        let mut i = inputs(vec![small_box(s)], vec![]);
        i.floors.push(RoleFloor { role: 0, min_seats: 2 });
        let a = allocate(&i);
        assert_eq!(a.open, vec![OpenSeats { node: s, owner: US, role: 1, count: 2 }], "the 7B holds orchestration only");
    }

    // what this catches: the 2026-09-20 07:52Z shape — a box whose only runnable plans are
    // 2,048-token lanes. Before this, sixteen minds drew onto it and every turn refused.
    #[test]
    fn a_box_that_can_only_serve_two_k_seats_nobody_and_says_how_far_off_it_is() {
        let n = Uuid::new_v4();
        let two_k = offer(n, vec![plan("27b", 9, 2_048, 1, None), plan("27b", 9, 2_048, 2, None)]);
        let m = minds(0, Some(n), 6);
        let a = allocate(&inputs(vec![two_k], m.clone()));
        assert_eq!(a.node(n).unwrap().verdict, NodeVerdict::BelowEveryRequirement { best_window: 2_048, best_capability: 9 });
        assert!(a.seated.is_empty());
        assert_eq!(a.dormant.len(), 6, "every mind dormant, identity kept — none seated on a useless lane");
        assert_eq!(a.open_total(), 0, "and nothing tells the spawner to mint more");
    }

    // what this catches: a lone big box seats coders on the 27B at requirement — capability
    // before lane count (the 7B at 4 lanes has more seats and is not chosen), lanes before
    // window (2 × 67k beats 1 × 131k), minds beyond the seats dormant.
    #[test]
    fn a_lone_big_box_seats_its_coders_on_the_most_capable_model_then_the_most_lanes() {
        let n = Uuid::new_v4();
        let m = minds(0, Some(n), 6);
        let a = allocate(&inputs(vec![big_box(n)], m.clone()));
        let node = a.node(n).unwrap();
        assert_eq!(node.verdict, NodeVerdict::Hosts { role: 0 });
        assert_eq!(node.plan.as_ref().unwrap(), &plan("27b", 9, 67_072, 2, Some(14.0)));
        assert_eq!(node.seats, 4);
        assert_eq!(a.seated.len(), 4);
        assert_eq!(a.dormant.len(), 2);
        assert!(a.seated.iter().all(|s| s.node == n && s.role == 0));
    }

    // what this catches: the grid growing — a GPU box joins, the seats grow from 4 to 10,
    // the two dormant minds wake ON THE NEW BOX (residents exist between the grid), the
    // four seated stay home, and the four seats nobody fills are the spawn signal.
    #[test]
    fn a_gpu_box_joining_wakes_the_dormant_keeps_the_seated_home_and_opens_seats_to_mint() {
        let big = Uuid::new_v4();
        let gpu = Uuid::new_v4();
        let m = minds(0, Some(big), 6);
        let alone = allocate(&inputs(vec![big_box(big)], m.clone()));
        let joined = allocate(&inputs(vec![big_box(big), gpu_box(gpu)], m.clone()));
        assert_eq!(joined.seats_for(0), 10);
        assert!(joined.dormant.is_empty(), "every existing mind has a seat now");
        for s in &alone.seated {
            assert_eq!(joined.seat_of(s.mind), Some(big), "a mind seated at home stays home");
        }
        for d in &alone.dormant {
            assert_eq!(joined.seat_of(*d), Some(gpu), "a dormant mind wakes where the seat opened");
        }
        assert_eq!(joined.open, vec![OpenSeats { node: gpu, owner: US, role: 0, count: 4 }]);
    }

    // what this catches: the grid shrinking — the GPU box drops; its minds reseat on the
    // big box's free seats, the rest go dormant, nothing is minted.
    #[test]
    fn a_node_dropping_reseats_what_the_grid_can_hold_and_the_rest_go_dormant() {
        let big = Uuid::new_v4();
        let gpu = Uuid::new_v4();
        let mut m = minds(0, Some(big), 2);
        m.extend(minds(0, Some(gpu), 6));
        let both = allocate(&inputs(vec![big_box(big), gpu_box(gpu)], m.clone()));
        assert_eq!(both.seated.len(), 8);
        assert_eq!(both.open_total(), 2);
        let dropped = allocate(&inputs(vec![big_box(big)], m.clone()));
        assert_eq!(dropped.seated.len(), 4, "four seats on the box that remains");
        assert_eq!(dropped.dormant.len(), 4);
        assert_eq!(dropped.open_total(), 0);
        assert!(dropped.seated.iter().all(|s| s.node == big));
        // The two homed on the big box keep their seats; two of the GPU box's minds move.
        for mind in m.iter().filter(|x| x.home == Some(big)) {
            assert_eq!(dropped.seat_of(mind.id), Some(big));
        }
    }

    // what this catches: a weak node is not a bad coder seat — it hosts the role it CAN hold
    // (orchestration), and coders go where the capability is when a big box joins.
    #[test]
    fn a_small_box_hosts_orchestration_and_coders_go_where_the_capability_is() {
        let small = Uuid::new_v4();
        let big = Uuid::new_v4();
        let mut m = minds(0, Some(small), 2);
        m.extend(minds(1, Some(small), 1));
        let alone = allocate(&inputs(vec![small_box(small)], m.clone()));
        let node = alone.node(small).unwrap();
        assert_eq!(node.verdict, NodeVerdict::Hosts { role: 1 }, "the 7B holds orchestration, never the coder role");
        assert_eq!(node.plan.as_ref().unwrap().model_id, "7b", "the most capable holding model, not the 1.5B with more lanes");
        assert_eq!(alone.dormant.len(), 2, "the coders wait for a box that can hold them");
        assert_eq!(alone.seated.len(), 1);
        let joined = allocate(&inputs(vec![small_box(small), big_box(big)], m.clone()));
        assert!(joined.dormant.is_empty());
        assert!(joined.seated.iter().filter(|s| s.role == 0).all(|s| s.node == big), "coders seat on the big box");
        assert_eq!(joined.node(small).unwrap().verdict, NodeVerdict::Hosts { role: 1 }, "the small box still hosts orchestration");
    }

    // what this catches: the decode knee is a requirement — 2 lanes at 6.3 t/s do not seat a
    // coder whose floor is 10; an UNMEASURED decode is not a refusal (absence ≠ number).
    #[test]
    fn a_starved_decode_does_not_hold_and_an_unmeasured_one_is_not_refused() {
        let req = coder().requirement;
        assert!(!plan("27b", 9, 67_072, 2, Some(6.3)).holds(&req));
        assert!(plan("27b", 9, 67_072, 1, Some(14.3)).holds(&req));
        assert!(plan("27b", 9, 67_072, 2, None).holds(&req));
        assert!(!plan("27b", 9, 65_535, 2, None).holds(&req), "one token under the window is under");
        assert!(!plan("7b", 6, 131_072, 4, Some(40.0)).holds(&req), "capability is a requirement, not a preference");
        assert!(!plan("27b", 9, 131_072, 0, None).holds(&req), "zero lanes seat nobody");
        let n = Uuid::new_v4();
        let knee = offer(n, vec![plan("27b", 9, 67_072, 2, Some(6.3)), plan("27b", 9, 131_072, 1, Some(14.3))]);
        let a = allocate(&inputs(vec![knee], minds(0, Some(n), 3)));
        assert_eq!(a.node(n).unwrap().plan.as_ref().unwrap().lanes, 1, "the knee sheds the second lane; the one lane that decodes is the seat");
        assert_eq!(a.seated.len(), 2);
        assert_eq!(a.dormant.len(), 1);
    }

    // what this catches: the population is the SEATS, never the minds on disk — too few
    // minds opens seats to mint; too many leaves the extra dormant; both are the same law.
    #[test]
    fn open_seats_are_the_spawn_signal_and_extra_minds_are_dormant() {
        let n = Uuid::new_v4();
        let few = allocate(&inputs(vec![big_box(n)], minds(0, None, 1)));
        assert_eq!(few.seated.len(), 1);
        assert_eq!(few.open, vec![OpenSeats { node: n, owner: US, role: 0, count: 3 }]);
        let many = allocate(&inputs(vec![big_box(n)], minds(0, None, 25)));
        assert_eq!(many.seated.len(), 4);
        assert_eq!(many.dormant.len(), 21);
        assert_eq!(many.open_total(), 0);
        let none = allocate(&inputs(vec![], minds(0, None, 3)));
        assert_eq!(none.dormant.len(), 3, "no grid, no seats, every mind dormant");
        let empty = allocate(&inputs(vec![offer(n, vec![])], vec![]));
        assert_eq!(empty.node(n).unwrap().verdict, NodeVerdict::NothingRunnable);
    }

    // what this catches (Cormac's condition on #4259): one engine at one window holds every
    // role at or below it — a 4-seat coder box with 2 coders and 3 orchestrators seats
    // 2 + 2, leaves 1 orchestrator dormant and NO seat open. Before, it seated the coders,
    // held 2 seats open for coders that did not exist, and sent 3 orchestrators dormant.
    #[test]
    fn a_node_seats_every_role_its_plan_holds_in_priority_order() {
        let n = Uuid::new_v4();
        let mut m = minds(0, Some(n), 2);
        m.extend(minds(1, Some(n), 3));
        let a = allocate(&inputs(vec![big_box(n)], m));
        let node = a.node(n).unwrap();
        assert_eq!(node.verdict, NodeVerdict::Hosts { role: 0 }, "chosen for the coders — capability first");
        assert_eq!(node.holds, vec![0, 1], "…and it holds orchestrators too");
        assert_eq!(a.seated.iter().filter(|s| s.role == 0).count(), 2);
        assert_eq!(a.seated.iter().filter(|s| s.role == 1).count(), 2);
        assert_eq!(a.dormant.len(), 1);
        assert_eq!(a.open_total(), 0, "no seat is open while a mind of a held role waits");
        // Priority holds under pressure: 5 coders and 3 orchestrators on the same 4 seats
        // seat 4 coders; every orchestrator waits.
        let mut m = minds(0, Some(n), 5);
        m.extend(minds(1, Some(n), 3));
        let a = allocate(&inputs(vec![big_box(n)], m));
        assert_eq!(a.seated.iter().filter(|s| s.role == 0).count(), 4);
        assert_eq!(a.seated.iter().filter(|s| s.role == 1).count(), 0);
        assert_eq!(a.dormant.len(), 4);
    }

    // what this catches: a homeless mind (fresh identity, home unknown) seats wherever the
    // grid has a free seat, spread to the node with the most room.
    #[test]
    fn a_mind_without_a_home_seats_where_the_grid_has_the_most_room() {
        let big = Uuid::new_v4();
        let gpu = Uuid::new_v4();
        let a = allocate(&inputs(vec![big_box(big), gpu_box(gpu)], minds(0, None, 1)));
        assert_eq!(a.seat_of(a.seated[0].mind), Some(gpu), "six free seats there, four here");
    }

    // what this catches (Joel: "prove that last case too"): a node ANOTHER HUMAN brings
    // is one more offer, with terms. Their coder box lends 2 seats for coders only. Our
    // dormant coders take exactly those two — never a third, never an orchestrator —
    // their own minds seat on it freely, and when they withdraw the lend, ours come home
    // or go dormant. The negotiation's outcome is the terms; the machinery is unchanged.
    #[test]
    fn a_peer_owned_node_lends_seats_within_its_terms_and_never_beyond() {
        let them = Uuid::from_u128(0xB0B);
        let ours = Uuid::new_v4();
        let theirs = Uuid::new_v4();
        let mut m = minds(0, Some(ours), 7); // 7 coders, our box seats 4
        m.push(Mind { id: Uuid::new_v4(), role: 1, home: Some(ours), owner: US }); // an orchestrator of ours
        let their_minds: Vec<Mind> = (0..2).map(|_| Mind { id: Uuid::new_v4(), role: 0, home: Some(theirs), owner: them }).collect();
        m.extend(their_minds.iter().cloned());
        let lending = NodeOffer {
            node: theirs,
            owner: them,
            terms: OfferTerms { seats_lent: Some(2), roles: Some(vec![0]) },
            plans: vec![plan("27b", 9, 131_072, 3, Some(30.0))], // 6 seats
        };
        let a = allocate(&inputs(vec![big_box(ours), lending.clone()], m.clone()));
        let on_theirs = |a: &GridAllocation, owner: Uuid| a.seated.iter().filter(|s| s.node == theirs && m.iter().any(|x| x.id == s.mind && x.owner == owner)).count();
        assert_eq!(on_theirs(&a, them), 2, "their own minds seat on their box, no terms spent");
        assert_eq!(on_theirs(&a, US), 2, "we take exactly the two seats lent");
        assert_eq!(a.seated.iter().filter(|s| s.node == ours).count(), 4, "our box full: 4 coders (the orchestrator waits — coders first)");
        assert_eq!(a.dormant.len(), 2, "one coder and the orchestrator: two lent seats, coders only, and our box is full");
        assert_eq!(a.open, vec![OpenSeats { node: theirs, owner: them, role: 0, count: 2 }], "their two spare seats are THEIRS to fill — our spawner mints nothing for a loan already spent");
        assert!(a.open.iter().all(|o| o.owner != US), "no open seat of ours anywhere");
        // A foreign node that declared no terms lends nothing.
        let silent_terms = NodeOffer { terms: OfferTerms::default(), ..lending.clone() };
        let a = allocate(&inputs(vec![big_box(ours), silent_terms], m.clone()));
        assert_eq!(on_theirs(&a, US), 0, "no declared terms = no loan");
        assert_eq!(on_theirs(&a, them), 2);
        // They withdraw the lend: ours come off their box.
        let withdrawn = NodeOffer { terms: OfferTerms { seats_lent: Some(0), roles: None }, ..lending.clone() };
        let a = allocate(&inputs(vec![big_box(ours), withdrawn], m.clone()));
        assert_eq!(on_theirs(&a, US), 0);
        assert_eq!(on_theirs(&a, them), 2);
        assert_eq!(a.dormant.len(), 4);
        // Open terms between our OWN nodes are never consulted: the earlier scenarios.
        assert!(OfferTerms::open().admits_role(7));
    }

    // what this catches: the offer book folds beacons into the allocator's inputs — a node
    // JOINS on its first beacon, stays while fresh, LEAVES when silent, RETURNS on the next
    // beacon — and the inputs key changes exactly when the allocation could.
    #[test]
    fn the_offer_book_names_join_silence_and_return_and_the_key_moves_only_with_them() {
        let silent = 6 * 60 * 60 * 1000;
        let big = Uuid::new_v4();
        let gpu = Uuid::new_v4();
        let m = minds(0, Some(big), 6);
        let mut book = OfferBook::default();
        book.hear(big_box(big), 1_000);
        let k1 = inputs_key(&inputs(book.live(1_000, silent), m.clone()));
        assert_eq!(inputs_key(&inputs(book.live(2_000, silent), m.clone())), k1, "a repeated beacon is not a change");
        book.hear(gpu_box(gpu), 3_000);
        let joined = book.live(3_000, silent);
        assert_eq!(joined.len(), 2, "join");
        assert_eq!(joined[0].node.min(joined[1].node), joined[0].node, "ordered by node id");
        let k2 = inputs_key(&inputs(joined.clone(), m.clone()));
        assert_ne!(k2, k1);
        assert_eq!(allocate(&inputs(joined, m.clone())).dormant.len(), 0, "six seated across both");
        let later = 3_000 + silent + 1;
        let alone = book.live(later, silent);
        assert_eq!(alone.len(), 0, "both silent: nobody live");
        book.hear(big_box(big), later);
        assert_eq!(book.live(later, silent).len(), 1, "the big box returned; the GPU box is still silent");
        assert_eq!(inputs_key(&inputs(book.live(later, silent), m.clone())), k1, "the same grid as at the start keys the same");
        assert_eq!(book.forget_silent(later, silent), vec![gpu], "the silent one is forgotten, named");
        assert_eq!(allocate(&inputs(book.live(later, silent), m.clone())).dormant.len(), 2, "…and the allocation is the lone-box one again");
    }

    // what this catches (card 10bba591): a recipe's citizens become the allocator's roles
    // and floors from DATA — declared requirements taken as written, undeclared ones at
    // the measured typical prompt (never a constant), authoring order as priority, a
    // repeated role adding a seat to its floor with the strictest requirement kept.
    #[test]
    fn a_recipes_citizens_are_the_allocators_roles_and_floors() {
        use crate::experience::recipe::{CitizenRecipe, CitizenRequirement};
        use crate::persona::role_template::RoleId;
        let coder = |window: u32| CitizenRecipe {
            role: RoleId::Coder,
            requirement: Some(CitizenRequirement { window_tokens: window, min_capability: 7, decode_floor_tps: Some(10.0) }),
        };
        let helper = CitizenRecipe { role: RoleId::Helper, requirement: None };
        let lax = CitizenRecipe {
            role: RoleId::Coder,
            requirement: Some(CitizenRequirement { window_tokens: 8_192, min_capability: 3, decode_floor_tps: Some(5.0) }),
        };
        let (roles, floors) = roles_from(&[coder(65_536), helper.clone(), coder(131_072), lax], Some(70_071));
        assert_eq!(roles.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), vec!["coder", "helper"], "authoring order is priority");
        assert_eq!(roles[0].requirement, Requirement { window: 131_072, target_window: None, min_capability: 7, decode_floor_tps: Some(10.0) }, "the strictest declared requirement wins on every axis — a later laxer citizen loosens nothing");
        assert_eq!(roles[1].requirement, Requirement { window: super::super::serving_plan::BOOTSTRAP_WORKING_SET, target_window: Some(70_071), min_capability: 0, decode_floor_tps: None }, "undeclared demand is a target, never a gate");
        assert_eq!(floors, vec![RoleFloor { role: 0, min_seats: 3 }, RoleFloor { role: 1, min_seats: 1 }]);
        let (roles, _) = roles_from(&[helper], None);
        assert_eq!(roles[0].requirement.window, super::super::serving_plan::BOOTSTRAP_WORKING_SET, "nothing measured yet = the bootstrap working-set floor");
        assert!(roles_from(&[], Some(1)).0.is_empty());
    }

    // Regression: a measured turn larger than every host window made all three
    // healthy nodes BelowEveryRequirement, leaving Kimi and Sahar dormant.
    #[test]
    fn measured_window_above_the_fleet_does_not_remove_its_seats() {
        use crate::experience::recipe::CitizenRecipe;
        use crate::persona::role_template::RoleId;
        let node = Uuid::new_v4();
        let (roles, floors) = roles_from(
            &[CitizenRecipe { role: RoleId::Helper, requirement: None }],
            Some(150_000),
        );
        let mut i = inputs(vec![
            offer(node, vec![plan("27b", 255, 65_280, 1, Some(50.0))]),
            offer(Uuid::new_v4(), vec![plan("peer", 42, 78_592, 3, None)]),
            offer(Uuid::new_v4(), vec![plan("thin", 40, 32_768, 1, None)]),
        ], minds(0, Some(node), 2));
        i.roles = roles;
        i.floors = floors;
        let a = allocate(&i);
        assert_eq!(a.seated.len(), 2);
        assert!(a.dormant.is_empty());
        assert!(a.nodes.iter().all(|n| n.seats > 0));
        let key = inputs_key(&i);
        i.roles[0].requirement.target_window = Some(160_000);
        assert_ne!(key, inputs_key(&i), "new measurements must recompute selection");
        i.nodes[0].plans[0].window = super::super::serving_plan::BOOTSTRAP_WORKING_SET - 1;
        assert_eq!(allocate(&i).node(node).unwrap().seats, 0, "the bootstrap working-set floor remains hard");
    }

    // Regression: measured targets must neither become declared gates nor loosen
    // declared requirements when citizens of the same role are combined.
    #[test]
    fn measured_targets_preserve_declared_gates_and_capability_order() {
        use crate::experience::recipe::{CitizenRecipe, CitizenRequirement};
        use crate::persona::role_template::RoleId;
        let declared = CitizenRecipe {
            role: RoleId::Coder,
            requirement: Some(CitizenRequirement { window_tokens: 32_768, min_capability: 7, decode_floor_tps: Some(10.0) }),
        };
        let measured = CitizenRecipe { role: RoleId::Coder, requirement: None };
        for citizens in [[declared.clone(), measured.clone()], [measured.clone(), declared.clone()]] {
            let (roles, _) = roles_from(&citizens, Some(100_000));
            let req = &roles[0].requirement;
            assert_eq!(req.window, 32_768);
            assert_eq!(req.target_window, Some(100_000));
            assert!(!plan("short", 9, 32_767, 1, Some(20.0)).holds(req));
            assert!(!plan("weak", 6, 131_072, 1, Some(20.0)).holds(req));
            assert!(!plan("slow", 9, 131_072, 1, Some(5.0)).holds(req));
            let candidates = vec![
                plan("strong-wide", 9, 65_536, 1, Some(20.0)),
                plan("strong-many", 9, 32_768, 3, Some(20.0)),
                plan("weaker-fitting", 8, 131_072, 4, Some(20.0)),
            ];
            assert_eq!(best_plan_for(&candidates, req).unwrap().model_id, "strong-wide");
            assert_eq!(candidates[0].better_than(&candidates[1], req), Some(BetterBy::Window));
            assert_eq!(candidates[1].better_than(&candidates[0], req), None, "migration cannot undo target-fit selection");
        }
    }

    // what this catches (card 10bba591): the "better seat" order IS the allocator's —
    // capability first, then lanes, then window — strict on every axis, so equal plans
    // are never a reason to move and a wider window never outranks a stronger model.
    #[test]
    fn a_better_seat_is_judged_in_the_allocators_own_order_and_never_on_equality() {
        let base = plan("27b", 9, 67_072, 2, None);
        let req = Requirement { window: 0, target_window: None, min_capability: 0, decode_floor_tps: None };
        assert_eq!(base.better_than(&base, &req), None, "equal plans: no move");
        assert_eq!(plan("27b", 9, 67_072, 2, Some(14.0)).better_than(&base, &req), None, "decode is not an axis of the order");
        assert_eq!(plan("30b", 10, 2_048, 1, None).better_than(&base, &req), Some(BetterBy::Capability), "capability beats everything below it");
        assert_eq!(base.better_than(&plan("30b", 10, 2_048, 1, None), &req), None);
        assert_eq!(plan("27b", 9, 32_768, 3, None).better_than(&base, &req), Some(BetterBy::Lanes), "same rank: lanes before window");
        assert_eq!(plan("27b", 9, 131_072, 1, None).better_than(&base, &req), None, "fewer lanes: no move, however wide");
        assert_eq!(plan("27b", 9, 131_072, 2, None).better_than(&base, &req), Some(BetterBy::Window), "same rank and lanes: the wider window");
        assert_eq!(BetterBy::Window.as_str(), "window");
    }

    // what this catches (card 10bba591): the node roster the spawner draws from is what
    // the allocation seated there PLUS what stands open for that owner — a lender's open
    // seats are not ours, and a node that made no offer is an absence (the spawner then
    // keeps its own prior), never a zero.
    #[test]
    fn a_nodes_roster_is_what_is_seated_there_plus_what_is_open_for_that_owner() {
        let n = Uuid::new_v4();
        let a = allocate(&inputs(vec![big_box(n)], minds(0, Some(n), 1)));
        assert_eq!(a.roster_on(n, US), Some(GridRoster { seated: 1, open: 3 }));
        assert_eq!(a.roster_on(n, US).map(GridRoster::seats), Some(4), "one seated, three to mint: four seats");
        assert_eq!(a.roster_on(n, Uuid::from_u128(0xB0B)), Some(GridRoster { seated: 1, open: 0 }), "the open seats are the owner's, not a stranger's");
        assert_eq!(a.roster_on(Uuid::new_v4(), US), None, "a node that made no offer is an absence");
        assert_eq!(a.plan_of_seat(a.seated[0].mind).map(|p| p.lanes), Some(2));
    }

    // what this catches: the outlier validation — the REAL per-node planner's plan reads
    // as an offer with no translation layer, so the daemon feeds `allocate` what it
    // already publishes. A cold 64 GB-class budget planning a 27B-class footprint.
    #[test]
    fn a_real_serving_plan_reads_as_a_lane_plan() {
        use super::super::serving_plan::{plan_serving, HostBudget, ModelFootprint, ServingDemand};
        let model = ModelFootprint {
            model_id: "coder-27b".into(),
            weights_bytes: 19_000_000_000,
            kv_per_token: 32_768,
            context_window: 262_144,
            capability_rank: 9,
            fixed_per_lane_bytes: 0,
        };
        let host = HostBudget { usable_bytes: 25_000_000_000, perf_cores: 12 };
        let plan = plan_serving(&host, &[model], &ServingDemand::new(2, Some(70_000))).expect("a 27B fits a 25 GB budget");
        let lane = LanePlan::of(&plan, None);
        assert_eq!(lane.model_id, "coder-27b");
        assert_eq!(lane.capability_rank, 9);
        assert_eq!(lane.window, plan.served_context_window);
        assert_eq!(lane.lanes, plan.lanes);
        assert!(lane.lanes >= 1);
        let n = Uuid::new_v4();
        let i = inputs(vec![offer(n, vec![lane.clone()])], minds(0, Some(n), 2));
        let a = allocate(&i);
        // The node hosts the first role its real plan holds — a 25 GB budget planning a
        // 27B for two lanes lands under the coder window here (2 × ~44k) and hosts
        // orchestration; the verdict names the plan's real width either way.
        let expected = match i.roles.iter().position(|r| lane.holds(&r.requirement)) {
            Some(role) => NodeVerdict::Hosts { role },
            None => NodeVerdict::BelowEveryRequirement { best_window: lane.window, best_capability: 9 },
        };
        assert_eq!(a.node(n).unwrap().verdict, expected);
        assert!(lane.window < coder().requirement.window, "the shape this fixture pins: two lanes on 25 GB do not reach the coder window");
    }
}
