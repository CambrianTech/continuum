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
    pub fn of(plan: &continuum_engine::inference::planning::serving_plan::ServingPlan, decode_tps_per_lane: Option<f32>) -> Self {
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
    /// `None` when nothing is LIVE — not ready, no model, no lanes, or an unknown window:
    /// an absence, never a substituted intent and never fabricated capacity.
    pub fn serving(
        snapshot: &continuum_engine::inference::llama_server::ServingSnapshot,
        capability_rank: u8,
        decode_tps_per_lane: Option<f32>,
    ) -> Option<Self> {
        // `is_live`, not just "has a model": a mid-relaunch publish keeps its geometry —
        // `serving_consumer` flips `ready = false` with `send_modify` and clears nothing
        // (llama_server's own transitional-publish test builds exactly that shape: lanes 1,
        // window 32,768, ready false). Seating on it would announce a lane that cannot
        // serve as capacity — "a lane that cannot serve is no seat", the same sentence the
        // fall-home path uses. Caught by Astra reviewing this change.
        if !snapshot.is_live() || snapshot.lanes == 0 || snapshot.served_context_window == 0 {
            return None;
        }
        let model_id = snapshot.active_model.clone()?;
        Some(Self {
            model_id,
            capability_rank,
            window: snapshot.served_context_window,
            lanes: snapshot.lanes,
            decode_tps_per_lane,
        })
    }

    /// Does one lane of this plan seat a mind of a role with this requirement?
    ///
    /// ADMISSION GATES ON DECLARED REQUIREMENTS ONLY (Fable on #4856, from the 9/21 law: a
    /// measured number must never be an admission gate; three nodes published ZERO seats that
    /// night). `decode_tps_per_lane` is a real measurement since #4856, and it may refuse a
    /// seat here only against a floor a citizen DECLARED (`decode_floor_tps`). A rate or turn
    /// time the grid derives orders placement and names a rate-starved mind on the health line;
    /// it must never become a floor in this function.
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
    /// The best seat each node has offered while LIVE — its most capable rank and its
    /// widest window — kept through an offer with no plan (a relaunch), dropped only by
    /// silence (an unplug). Relaunching and unplugged are two signals (Cormac on #4416):
    /// a node between plans still bounds the coder floor, so a 1.5B never holds a coder
    /// because a 27B blinked.
    best_seen: std::collections::BTreeMap<Uuid, (u64, Vec<(u8, u32)>)>,
}

impl OfferBook {
    pub fn hear(&mut self, offer: NodeOffer, now_ms: u64) {
        if offer.plans.is_empty() {
            // Alive, no plan this pass: the seats it last offered stand, freshly heard.
            if let Some(entry) = self.best_seen.get_mut(&offer.node) {
                entry.0 = now_ms;
            }
        } else {
            let seats = offer.plans.iter().map(|p| (p.capability_rank, p.window)).collect();
            self.best_seen.insert(offer.node, (now_ms, seats));
        }
        self.heard.insert(offer.node, (now_ms, offer));
    }

    /// Every seat a LIVE node has offered, as (rank, window) PAIRS — a node mid-relaunch
    /// counts by its last plans; a node silent past the window does not. Pairs, never a
    /// max per axis: a 27B at 35k beside a 7B at 65k is two seats, not one 27B at 65k
    /// (Cormac on #4416). Empty when nobody offers.
    pub fn seats_live(&self, now_ms: u64, silent_after_ms: u64) -> Vec<(u8, u32)> {
        self.best_seen
            .values()
            .filter(|(heard_at, _)| now_ms.saturating_sub(*heard_at) <= silent_after_ms)
            .flat_map(|(_, seats)| seats.iter().copied())
            .collect()
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
            self.best_seen.remove(n);
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
        (hold.node, &hold.only, hold.unresolved, hold.exclusive).hash(&mut h);
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
/// at most its declared population in seats, including names not registered yet,
/// so startup cannot shrink the capacity needed for those names to register. Nothing is minted
/// for a held box. An exclusive hold naming nobody restricts nothing (the file's own
/// semantics in `seats_under`). DERIVED (a working round's team, not exclusive): the named
/// minds seat FIRST on that node and everyone else after — never fewer minds.
#[derive(Clone, Debug, PartialEq)]
pub struct Hold {
    pub node: Uuid,
    pub only: Vec<Uuid>,
    /// Named members without a live runtime yet. Capacity includes them; admission
    /// still requires a resolved identity, never a fabricated citizen id.
    pub unresolved: usize,
    pub exclusive: bool,
}

impl Hold {
    fn population(&self) -> usize {
        self.only.len().saturating_add(self.unresolved)
    }
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
    /// Held names still registering. These permit resumption, never new identities.
    pub resuming: Vec<OpenSeats>,
}

/// One node's roster as the allocation reads it: the minds seated there and the seats
/// nobody fills — what the spawner on that node may draw (`seated + open`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GridRoster {
    pub seated: u32,
    pub open: u32,
    pub resuming: u32,
}

impl GridRoster {
    pub fn seats(self) -> u32 {
        self.seated.saturating_add(self.open).saturating_add(self.resuming)
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
            resuming: self.resuming.iter().filter(|o| o.node == node && o.owner == owner).map(|o| o.count).sum(),
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
    citizens: &[continuum_experience::experience::recipe::CitizenRecipe],
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
                window: continuum_engine::inference::planning::serving_plan::BOOTSTRAP_WORKING_SET,
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
/// The ask a role can actually be held to, given the seats the grid offers RIGHT NOW:
/// the declared requirement is what the activity asks for; the grid bounds it so that
/// ONE REAL SEAT always holds it. Capability first (the declared floor, or the most
/// capable seat there is), then the window to the widest seat AT OR ABOVE that
/// capability. Never a max per axis across different seats — a 27B at 35k and a 7B at
/// 65k would give an ask of (27B, 65k) that neither holds. Joel: start at CPU on an
/// Intel Mac alone; scale up when the 5090 joins; down when it leaves.
pub fn ask_within(declared: &Requirement, seats: &[(u8, u32)]) -> Requirement {
    let best_rank = seats.iter().map(|(r, _)| *r).max().unwrap_or(0); // unwrap_or: no seat = no floor to hold
    let min_capability = declared.min_capability.min(best_rank);
    let widest_at_rank = seats
        .iter()
        .filter(|(r, _)| *r >= min_capability)
        .map(|(_, w)| *w)
        .max()
        .unwrap_or(0); // unwrap_or: no seat = no window to hold
    Requirement {
        window: declared.window.min(widest_at_rank),
        min_capability,
        ..declared.clone()
    }
}

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
                Some(h) if h.exclusive && h.population() > 0 => warm.min(h.population().min(u32::MAX as usize) as u32),
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
            Some(h) if h.exclusive && h.population() > 0 => h.only.contains(&m.id),
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
    let held = |node: Uuid| inputs.hold_on(node).is_some_and(|h| h.exclusive && h.population() > 0);
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
    let resuming = nodes.iter().zip(free.iter()).zip(inputs.nodes.iter())
        .filter_map(|((n, &available), offer)| {
            let hold = inputs.hold_on(n.node)?;
            let NodeVerdict::Hosts { role } = &n.verdict else { return None };
            let count = available.min(hold.unresolved.min(u32::MAX as usize) as u32);
            (hold.exclusive && count > 0).then_some(OpenSeats {
                node: n.node, owner: offer.owner, role: *role, count,
            })
        }).collect();
    GridAllocation { nodes, seated, dormant, role_unservable, open, resuming }
}

#[cfg(test)]
mod tests;
