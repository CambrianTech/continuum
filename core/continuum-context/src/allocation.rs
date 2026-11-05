//! An act's ONE allocation (ONE-ACT stage 2): everything a mind's prompt holds is a claim in one
//! [`flex_allocate`] over her window, run ONCE per act. Nothing is sized beside it.
//!
//! Before this module the deliberation faculty sized three side budgets outside the allocation
//! (what the window left after its reserves, the room her newest result got, the grounding's
//! budget) and ran the allocator three times an act: once for the grants, once with the reply
//! seated at its want for her view's start, and once more inside the start to share it among her
//! view's sources. Each was a second owner of a fact the allocation already decided.
//!
//! Now:
//! - every part is a claim ([`ActClaims`]): framing, the required input, her NEWEST result (the
//!   largest of its own levels that fits beside the required floors: whole whenever the window
//!   holds it, goal 7), her hands, her LAST MOVE (required, whole), the standing grounding, the
//!   rest of the fresh history (optional), the reply, her view's sources;
//! - her view's START is arithmetic on the one plan ([`ActPlan::start_room`],
//!   [`start_shares`]), never a second allocation;
//! - irreducible growth is typed apart from reducible history: required floors past the window
//!   are an [`IrreducibleShortfall`] (her need grows or she waits), while a view that outgrew its
//!   grant is reducible and turns over (collapse older items to handles), never a grant.
//!
//! Pure and allocation-only, so a validator drives it with recorded items in seconds (the dyno's
//! reproduction 7) with no faculty, engine or I/O.

use crate::flex::{flex_allocate, turn_need, AllocationState, FlexAllocation, FlexClaim};

/// The claim ids of an act's allocation.
pub mod id {
    pub const FRAMING: &str = "framing";
    pub const REQUIRED: &str = "required_input";
    pub const NEWEST: &str = "newest_result";
    pub const GROUNDING: &str = "grounding";
    pub const HISTORY: &str = "history";
    pub const LAST_MOVE: &str = "last_move";
    pub const REPLY: &str = "reply";
    pub const TOOLS: &str = "tools";
    /// The prefix of each of her view's sources' claims: `view/<source>`.
    pub const VIEW: &str = "view/";
}

/// The priority every fixed, required part takes: it is served before anything grows.
const FIXED_PRIORITY: u8 = 10;
/// Her hands' weight: above the grounding and history, below the fixed parts.
const TOOLS_PRIORITY: u8 = 7;
/// The standing grounding's weight.
const GROUNDING_PRIORITY: u8 = 6;
/// The fresh history's weight.
const HISTORY_PRIORITY: u8 = 5;

/// A part that can grow: its floor (never less), its min (served in priority order before
/// anything grows) and its want (never more). Charged tokens.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Span {
    pub floor: u32,
    pub min: u32,
    pub want: u32,
}

/// Her newest result as the levels it can be shown at, charged tokens, whole first and each
/// smaller (its summary chain to the end, each with its handle). An unsaved result has no smaller
/// level: its only level is its whole. Empty: this act carries no result.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Newest {
    pub levels: Vec<u32>,
}

/// Every claim on one act's window, in the allocation's unit (charged tokens).
#[derive(Debug, Clone, PartialEq)]
pub struct ActClaims {
    /// Identity, policy and the tool menu: fixed, required.
    pub framing: u32,
    /// The required input other than her newest result: the stimulus, her action statement.
    pub required: u32,
    pub newest: Newest,
    /// Her hands: floor = the core hands, want = her whole surface.
    pub tools: Span,
    /// Enrichment: optional, never a reason to refuse a turn.
    pub grounding: Span,
    /// Her LAST MOVE in this act's fresh tail: her newest complete act and its outcome. Required
    /// and whole (goal 7, ONE-ACT stage 2: the newest complete interaction is whole): clipping it
    /// costs a whole extra turn when she re-issues the act.
    pub last_move: u32,
    /// The rest of this act's fresh tail (older acts, facts, the clock): optional, shed first.
    pub history: Span,
    /// The reply's claim as the turn kind priced it (`reply_claim`). Its floor is capped here so
    /// it never takes from another claim's floor; its uncapped floor is her need.
    pub reply: FlexClaim,
    /// Each source of her view, `view/<source>`, at the source's priority.
    pub view: Vec<FlexClaim>,
}

/// Required floors past the window: growth no collapse can make, typed apart from a view that
/// outgrew its grant (reducible). Her need records it; admission decides (ONE-ACT stage 3/4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IrreducibleShortfall {
    /// What the required floors sum to, charged.
    pub needed: u32,
    pub window: u32,
}

/// What the one allocation decided, and the arithmetic every consumer reads from it.
#[derive(Debug, Clone, PartialEq)]
pub struct ActPlan {
    pub window: u32,
    pub plan: FlexAllocation,
    claims: Vec<FlexClaim>,
    /// The reply's floor before the cap: her measured need.
    reply_need: u32,
    /// Which of her newest result's levels the allocation chose (an index into `Newest::levels`).
    newest_level: usize,
}

/// The fixed, required claim of one part.
fn fixed(id: &str, tokens: u32) -> FlexClaim {
    FlexClaim {
        source_id: id.to_owned(),
        priority: FIXED_PRIORITY,
        floor_tokens: tokens,
        min_tokens: tokens,
        max_tokens: tokens,
        required: true,
    }
}

fn span(id: &str, priority: u8, span: Span, required: bool) -> FlexClaim {
    let floor = span.floor;
    let min = span.min.max(floor);
    FlexClaim {
        source_id: id.to_owned(),
        priority,
        floor_tokens: floor,
        min_tokens: min,
        max_tokens: span.want.max(min),
        required,
    }
}

/// THE allocation of one act: `window` tokens shared across `claims`, once.
pub fn allocate(window: u32, claims: ActClaims) -> ActPlan {
    let mut all = vec![
        fixed(id::FRAMING, claims.framing),
        fixed(id::REQUIRED, claims.required),
        span(id::TOOLS, TOOLS_PRIORITY, claims.tools, true),
        fixed(id::LAST_MOVE, claims.last_move),
        span(id::GROUNDING, GROUNDING_PRIORITY, claims.grounding, false),
        span(id::HISTORY, HISTORY_PRIORITY, claims.history, false),
    ];
    all.extend(claims.view);
    // Her newest result is a discrete choice among its own levels, never a span: whole whenever the
    // window holds it beside the REQUIRED floors (the reply's measured need included), else the
    // largest level that does, else its coarsest. Optional claims (grounding, history, every
    // optional view source) never push it down: they shed in the floor pass, oldest first, which
    // is what optional means (goal 7: the newest end is never cut; Cormac on #5779: counting
    // every floor showed it at a summary on a window that held it whole). A level she cannot see
    // whole still carries content and its handle. As a span (floor coarsest, min
    // whole) it took every token left in the min pass at its priority, showed the largest level
    // under that and wasted the rest, so her view's sources fell to their floors and a room update
    // that fit beside her result vanished (capacity_and_wire, the wire test).
    let required: Vec<FlexClaim> = all.iter().filter(|claim| claim.required).cloned().collect();
    let others = turn_need(&required).saturating_add(claims.reply.floor_tokens);
    let left = window.saturating_sub(others);
    let newest_level = match claims.newest.levels.iter().position(|&cost| cost <= left) {
        Some(level) => level,
        None => claims.newest.levels.len().saturating_sub(1),
    };
    // An act with no result makes no newest claim.
    if let Some(&cost) = claims.newest.levels.get(newest_level) {
        all.insert(2, fixed(id::NEWEST, cost));
    }
    // The reply never takes from a REQUIRED floor: its floor is what the window leaves after the
    // required floors (her newest result at its chosen level among them). The required input once
    // refused at 275 of 0 when the reply's floor filled an 8,192 window to the token. Optional
    // floors (history, grounding, optional view sources) shed before her reply's measured need, as
    // they shed before her newest result (Cormac on #5779): capped against every floor, the reply
    // was cut to 200 of its 800 so optional history and view kept theirs.
    let mut reply = claims.reply;
    let reply_need = reply.floor_tokens;
    let required_with_newest: Vec<FlexClaim> = all.iter().filter(|claim| claim.required).cloned().collect();
    reply.floor_tokens = reply_need.min(window.saturating_sub(turn_need(&required_with_newest)));
    reply.min_tokens = reply.floor_tokens;
    reply.source_id = id::REPLY.to_owned();
    all.push(reply);
    let plan = flex_allocate(window, &all);
    ActPlan { window, plan, claims: all, reply_need, newest_level }
}

impl ActPlan {
    /// The grant of the claim `id`; 0 when none was made.
    pub fn granted(&self, id: &str) -> u32 {
        self.plan.granted(id)
    }

    /// Every claim as the allocation saw it (the receipt).
    pub fn claims(&self) -> &[FlexClaim] {
        &self.claims
    }

    /// Which of her newest result's levels this act shows (an index into `Newest::levels`: 0 is
    /// whole).
    pub fn newest_level(&self) -> usize {
        self.newest_level
    }

    /// The reply's claim as allocated (its floor capped).
    pub fn reply(&self) -> Option<&FlexClaim> {
        self.claims.iter().find(|claim| claim.source_id == id::REPLY)
    }

    /// The sum of her view's sources' grants.
    pub fn view_granted(&self) -> u32 {
        self.plan
            .allocations
            .iter()
            .filter(|a| a.source_id.starts_with(id::VIEW))
            .map(|a| a.allocated_tokens)
            .fold(0, u32::saturating_add)
    }

    /// What no claim took: room her view grows into by appends within the epoch.
    pub fn room(&self) -> u32 {
        self.plan.unallocated
    }

    /// What her view's START may hold: its grants and the room no claim took, less what the
    /// reply wants beyond its grant. A start is sized as if the reply held its whole want, so a
    /// pass (reply priority 4) and the act after it (10) start her view alike: before, a second
    /// allocation with the reply seated at its want decided this (one 16,384 window started her
    /// conversation at 4,005 tokens on a pass and 2,402 on the act, and her prefix moved).
    pub fn start_room(&self) -> u32 {
        let reply_short = self
            .reply()
            .map_or(0, |reply| reply.max_tokens.saturating_sub(self.granted(id::REPLY))); // map_or: no reply claimed wants nothing more
        self.view_granted().saturating_add(self.room()).saturating_sub(reply_short)
    }

    /// The view sources the allocation could not grant their floor (dropped, never a required
    /// one): a start sheds them; a later act offers them again.
    pub fn shed_view_sources(&self) -> Vec<&str> {
        self.plan
            .allocations
            .iter()
            .filter(|a| a.allocated_tokens < a.requested_floor && a.state != AllocationState::UnderProvisioned)
            .filter_map(|a| a.source_id.strip_prefix(id::VIEW))
            .collect()
    }

    /// Her held need: every claim's floor with the reply at its uncapped need. What a lane must
    /// hold for this act, never what the window granted (`turn_need`).
    pub fn held_need(&self) -> u32 {
        let capped = self.reply().map_or(0, |reply| reply.floor_tokens); // map_or: no reply claimed
        turn_need(&self.claims).saturating_sub(capped).saturating_add(self.reply_need)
    }

    /// The required claims' floors (framing, required input, her newest result at its chosen level,
    /// her core hands, her view's required sources), charged: what cannot be shed.
    pub fn irreducible(&self) -> u32 {
        self.claims
            .iter()
            .filter(|claim| claim.required && claim.source_id != id::REPLY)
            .map(|claim| claim.floor_tokens)
            .fold(0, u32::saturating_add)
    }

    /// `Some` when the required floors with the reply's floor exceed the window: irreducible
    /// growth, never answered by collapsing history.
    pub fn irreducible_shortfall(&self) -> Option<IrreducibleShortfall> {
        let needed = turn_need(&self.claims.iter().filter(|claim| claim.required).cloned().collect::<Vec<_>>());
        (self.plan.escalation_needed || needed > self.window).then_some(IrreducibleShortfall { needed, window: self.window })
    }
}

/// One view source at a start: its floor, and the grant the one allocation gave it. A `fixed`
/// source (any priority above the lowest: identity, policy, standing blocks) starts whole at its
/// grant; the lowest-priority sources (her conversation, results, thoughts) grow by appends and
/// share what the start leaves them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartSource {
    pub priority: u8,
    pub fixed: bool,
    pub floor: u32,
    pub grant: u32,
}

/// Each view source's share of a start of `start` tokens, arithmetic on the one plan's grants
/// (never a second allocation): every source its floor, in priority order, while the start holds
/// it (a floor that does not fit starts that source empty); then the fixed sources up to their
/// grant, in priority order; then what is left shared among the growing sources in proportion to
/// what each lacks of its grant. No source exceeds its grant.
pub fn start_shares(start: u32, sources: &[StartSource]) -> Vec<u32> {
    let mut order: Vec<usize> = (0..sources.len()).collect();
    order.sort_by(|&a, &b| sources[b].priority.cmp(&sources[a].priority));
    let mut shares = vec![0u32; sources.len()];
    let mut left = start;
    for &i in &order {
        let floor = sources[i].floor.min(sources[i].grant);
        if floor <= left {
            shares[i] = floor;
            left -= floor;
        }
    }
    for &i in order.iter().filter(|&&i| sources[i].fixed) {
        let more = sources[i].grant.saturating_sub(shares[i]).min(left);
        shares[i] += more;
        left -= more;
    }
    let lacking: Vec<(usize, u32)> = order
        .iter()
        .filter(|&&i| !sources[i].fixed)
        .map(|&i| (i, sources[i].grant.saturating_sub(shares[i])))
        .collect();
    let total_lacking: u64 = lacking.iter().map(|&(_, lack)| u64::from(lack)).sum();
    if total_lacking > 0 {
        let pool = u64::from(left).min(total_lacking);
        for (i, lack) in lacking {
            shares[i] += (pool * u64::from(lack) / total_lacking) as u32;
        }
    }
    shares
}

#[cfg(test)]
mod tests;
