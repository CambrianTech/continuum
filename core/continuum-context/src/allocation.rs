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
//!   largest of its own levels that fits beside every hard limit: whole whenever the window holds
//!   it, goal 7), her hands, her LAST MOVE (required, whole), the standing grounding, the rest of
//!   the fresh history (optional), the reply, her view's sources;
//! - every part made of items is a [`Leveled`] consumer (the one contract, [`crate::ladder`]): its
//!   items' levels come from their shapes and its hard limit is derived from them, never declared.
//!   Their levels are searched together ([`search`]) and the continuous parts share what
//!   they leave, so a window too small for every part whole collapses what matters least first, by
//!   level, and the turn is short only when every hard limit together does not fit (Joel,
//!   2026-11-07: "The rag budgeting should collapse rather than crap out");
//! - context has a price ([`priced`], card 7b7a696a): every token she holds slows every act by
//!   her measured seconds ([`crate::price`]), so the plan holds the window where her act's value
//!   per second peaks, never simply the window it was served; unmeasured prices nothing;
//! - her view's START is arithmetic on the one plan ([`ActPlan::start_room`],
//!   [`start_shares`]), never a second allocation;
//! - irreducible growth is typed apart from reducible history: required floors past the window
//!   are an [`IrreducibleShortfall`] (her need grows or she waits), while a view that outgrew its
//!   grant is reducible and turns over (collapse older items to handles), never a grant.
//!
//! Pure and allocation-only, so a validator drives it with recorded items in seconds (the dyno's
//! reproduction 7) with no faculty, engine or I/O.

use crate::flex::{flex_allocate, turn_need, AllocationState, FlexAllocation, FlexClaim};
pub use crate::ladder::{Choice, Leveled, Rungs};
pub mod priced;
pub mod search;
use crate::price::ContextPrice;
pub use priced::Priced;
use search::{overflow, search, Group, Searched};

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

/// A part that grows continuously: its floor (never less), its min (served in priority order
/// before anything grows) and its want (never more). Charged tokens. Only for a part whose every
/// size between floor and want is a rendering (the fresh history, which the fit shortens by whole
/// messages from its oldest end); a part made of items is a [`Leveled`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Span {
    pub floor: u32,
    pub min: u32,
    pub want: u32,
}

/// A leveled part of her prompt's HEAD (her tools, the tool menu in her framing): the chat
/// template renders it before every message, so a level change re-prefills the whole prompt. The
/// levels her head's epoch holds (`held`, one rung per item) stay while the window holds them;
/// `None` before an epoch has chosen.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Head {
    pub leveled: Leveled,
    pub held: Option<Vec<usize>>,
}

impl Head {
    /// A head part no epoch holds yet.
    pub fn free(leveled: Leveled) -> Self {
        Self { leveled, held: None }
    }

    /// The held levels while they still name one level of each item.
    fn holding(&self) -> Option<Choice> {
        let rungs = self.held.as_ref()?;
        let fits = rungs.len() == self.leveled.items().len()
            && rungs.iter().zip(self.leveled.items()).all(|(&rung, item)| rung < item.costs().len());
        fits.then(|| Choice { cost: self.leveled.cost_of(rungs), rungs: rungs.clone() })
    }
}

/// Every claim on one act's window, in the allocation's unit (charged tokens). Every part made of
/// items is a [`Leveled`]: its floor is DERIVED from its items' hard limits (their pointers, or the
/// whole of input she must read), never declared, so no part can claim a floor its items do not
/// have (`every_leveled_claims_floor_is_its_items_hard_limit`).
#[derive(Debug, Clone, PartialEq)]
pub struct ActClaims {
    /// Identity, policy and the tool menu: the irreducible blocks whole, each menu category whole
    /// or collapsed to its handle.
    pub framing: Head,
    /// The required input other than her newest result: the stimulus, her action statement. Input
    /// she must read: its hard limit is its whole.
    pub required: Leveled,
    /// Her newest result: one item, whole first and each level smaller (its summary chain to the
    /// end, each with its handle). An unsaved result has only its whole. Empty: no result.
    pub newest: Leveled,
    /// Her hands: each verb whole, at its summary, or at its pointer (its name, expanded by
    /// `commands/help`).
    pub tools: Head,
    /// Enrichment: each grounding piece whole, at its summaries, or at its handle.
    pub grounding: Leveled,
    /// Her LAST MOVE in this act's fresh tail: her newest complete act and its outcome. Required
    /// and whole (goal 7, ONE-ACT stage 2: the newest complete interaction is whole): clipping it
    /// costs a whole extra turn when she re-issues the act.
    pub last_move: Leveled,
    /// The rest of this act's fresh tail (older acts, facts, the clock): optional, shed first.
    pub history: Span,
    /// The reply's claim as the turn kind priced it (`reply_claim`). Its floor is capped here so
    /// it never takes from another claim's floor; its uncapped floor is her need.
    pub reply: FlexClaim,
    /// Each source of her view, `view/<source>`, at the source's priority.
    pub view: Vec<FlexClaim>,
    /// What holding context costs her act on the node and model serving her ([`crate::price`]):
    /// the allocation holds the window where her value per second peaks ([`priced`]).
    /// `Unmeasured` prices nothing and fills the window as before.
    pub price: ContextPrice,
}

/// Every hard limit and her reply's measured need past the window: growth no collapse can make,
/// typed apart from a view that outgrew its grant (reducible). Her need records it; admission
/// decides (ONE-ACT stage 3/4). `binding` is the claim whose hard limit is the largest: the one a
/// larger window or a smaller limit must answer first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrreducibleShortfall {
    /// What the hard limits and her reply's measured need sum to, charged.
    pub needed: u32,
    pub window: u32,
    pub binding: String,
    pub binding_limit: u32,
}

/// What the one allocation decided, and the arithmetic every consumer reads from it.
#[derive(Debug, Clone, PartialEq)]
pub struct ActPlan {
    pub window: u32,
    pub plan: FlexAllocation,
    claims: Vec<FlexClaim>,
    /// The reply's floor before the cap: her measured need.
    reply_need: u32,
    /// Which of her newest result's levels the allocation chose (an index into its rungs).
    newest_level: usize,
    /// Each part's hard limit as the search saw it, the reply at its uncapped need among them.
    limits: Vec<Group>,
    /// Each leveled part's own hard limit (her newest result at its coarsest level): what a
    /// receipt names as the part's floor.
    hard_limits: Vec<(&'static str, u32)>,
    /// The levels chosen for each leveled part, by claim id.
    chosen: Vec<(&'static str, Choice)>,
    /// Head parts whose held levels the window could no longer hold: re-chosen this act, loudly.
    lost: Vec<&'static str>,
    /// Head parts held below what this act's search would show them at unheld: the levels each
    /// rises to at the next start of her head's epoch (both ways: no collapse is permanent).
    rise: Vec<(&'static str, Choice)>,
    /// What the search did this act; `None` when every hard limit together did not fit.
    pub searched: Option<Searched>,
    /// What the time price of context decided: the window this plan holds within the served one.
    pub priced: Priced,
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

/// One leveled part as the search sees it: its options (every distinct reduction, or only its
/// held levels).
struct Part<'a> {
    id: &'static str,
    priority: u8,
    leveled: &'a Leveled,
    options: Vec<Choice>,
}

impl Part<'_> {
    fn group(&self) -> Group {
        Group { id: self.id.to_owned(), priority: self.priority, options: self.options.iter().map(|o| o.cost).collect() }
    }
}

/// The first guess: the flexbox over every claim, each group's grant rounded down to its largest
/// option that fits it.
pub fn first_guess(window: u32, groups: &[Group], continuous: &[FlexClaim]) -> Vec<usize> {
    let mut claims: Vec<FlexClaim> = groups
        .iter()
        .map(|g| FlexClaim {
            source_id: g.id.clone(),
            priority: g.priority,
            floor_tokens: g.limit(),
            min_tokens: g.limit(),
            max_tokens: g.want(),
            required: true,
        })
        .collect();
    claims.extend(continuous.iter().cloned());
    let plan = flex_allocate(window, &claims);
    groups
        .iter()
        .map(|g| {
            let grant = plan.granted(&g.id);
            g.options.iter().position(|&cost| cost <= grant).unwrap_or(g.options.len() - 1) // unwrap_or: below every option is its hard limit
        })
        .collect()
}

/// THE allocation of one act: the window her act holds ([`priced::hold`]: where her value per
/// second peaks, never past `window`), shared across `claims`.
pub fn allocate(window: u32, claims: ActClaims) -> ActPlan {
    priced::hold(window, claims)
}

/// Every hard limit of `claims` that the search never reduces: framing, the required input, her
/// tools and her last move at their hard limits, and her view's required floors.
fn hard_limits(claims: &ActClaims) -> u32 {
    let view_floors = turn_need(&claims.view.iter().filter(|claim| claim.required).cloned().collect::<Vec<_>>());
    claims
        .framing
        .leveled
        .hard_limit()
        .saturating_add(claims.required.hard_limit())
        .saturating_add(claims.tools.leveled.hard_limit())
        .saturating_add(claims.last_move.hard_limit())
        .saturating_add(view_floors)
}

/// The plan of one act at `window` tokens shared across `claims`, once. Every leveled part
/// answers "the most you can show within this" ([`Leveled::reduce`]); their levels are searched
/// together ([`search`]), and the continuous parts share what the levels leave through the
/// flexbox. The turn is short only when every hard limit together does not fit
/// ([`ActPlan::irreducible_shortfall`], naming the binding one).
fn plan_at(window: u32, claims: ActClaims) -> ActPlan {
    let hard = hard_limits(&claims);
    let ActClaims { framing, required, newest, tools, grounding, last_move, history, reply, view, price: _ } = claims;
    // Her newest result is chosen before the search, among its own levels: whole whenever the
    // window holds it beside every HARD LIMIT (the reply's measured need included), else the
    // largest level that does, else its coarsest. The detail above any part's hard limit and every
    // optional part reduce before it (goal 7: the newest end is never cut; Cormac on #5779:
    // counting every floor showed it at a summary on a window that held it whole).
    let left = window.saturating_sub(hard.saturating_add(reply.floor_tokens));
    let newest_options = newest.options();
    let newest_choice = newest_options.iter().find(|option| option.cost <= left).or(newest_options.last()).cloned().unwrap_or_default(); // unwrap_or_default: no result, no choice
    let newest_level = newest_choice.rungs.first().copied().unwrap_or(0); // unwrap_or: no result has only level 0 of nothing
    // The reply never takes from a hard limit: its floor is what the window leaves after them,
    // her newest result at its chosen level among them (the required input once refused at 275 of
    // 0 when the reply's floor filled an 8,192 window to the token).
    let mut reply = reply;
    let reply_need = reply.floor_tokens;
    reply.floor_tokens = reply_need.min(window.saturating_sub(hard.saturating_add(newest_choice.cost)));
    reply.min_tokens = reply.floor_tokens;
    reply.source_id = id::REPLY.to_owned();
    let mut continuous = vec![span(id::HISTORY, HISTORY_PRIORITY, history, false)];
    continuous.extend(view);
    continuous.push(reply);
    let whole = |id: &'static str, leveled: &Leveled| Group { id: id.to_owned(), priority: FIXED_PRIORITY, options: vec![leveled.want()] };
    let singles = [whole(id::REQUIRED, &required), whole(id::LAST_MOVE, &last_move), Group { id: id::NEWEST.to_owned(), priority: FIXED_PRIORITY, options: vec![newest_choice.cost] }];
    // A head part offers only its held levels while they fit beside every other hard limit (a level
    // change at the head re-prefills the whole prompt); else every reduction, and the loss is said.
    let parts_with = |hold: bool| -> (Vec<Part<'_>>, Vec<&'static str>) {
        let mut lost = Vec::new();
        let mut parts = Vec::new();
        for (id, priority, head) in [(id::FRAMING, FIXED_PRIORITY, &framing), (id::TOOLS, TOOLS_PRIORITY, &tools)] {
            let others = hard.saturating_sub(head.leveled.hard_limit()).saturating_add(newest_choice.cost).saturating_add(reply_need.min(window));
            let options = match head.holding().filter(|_| hold) {
                Some(held) if held.cost.saturating_add(others) <= window => vec![held],
                Some(_) => {
                    lost.push(id);
                    head.leveled.options()
                }
                None => head.leveled.options(),
            };
            parts.push(Part { id, priority, leveled: &head.leveled, options });
        }
        // Grounding is enrichment: below its handles it leaves the turn (the empty choice), never
        // a reason to refuse one.
        let mut options = grounding.options();
        if grounding.hard_limit() > 0 {
            options.push(Choice::default());
        }
        parts.push(Part { id: id::GROUNDING, priority: GROUNDING_PRIORITY, leveled: &grounding, options });
        (parts, lost)
    };
    let groups_of = |parts: &[Part<'_>]| -> Vec<Group> { parts.iter().map(Part::group).chain(singles.iter().cloned()).collect() };
    // The search sees the reply at its measured need only: its growth above it takes what the
    // levels leave and never collapses another part, so a pass and the act after it (whose replies
    // differ in priority and want) choose the same levels and start her view alike.
    let searched_claims: Vec<FlexClaim> = continuous
        .iter()
        .map(|claim| if claim.source_id == id::REPLY { FlexClaim { max_tokens: claim.floor_tokens, ..claim.clone() } } else { claim.clone() })
        .collect();
    let (parts, lost) = parts_with(true);
    let groups = groups_of(&parts);
    let searched = search(window, &groups, &searched_claims, first_guess(window, &groups, &searched_claims)).ok();
    // Every hard limit together does not fit: each part at its hard limit and the shortfall typed
    // (`irreducible_shortfall`); never a part dropped or budget minted.
    let picks: Vec<usize> = searched.as_ref().map_or_else(|| groups.iter().map(|g| g.options.len() - 1).collect(), |found| found.picks.clone()); // map_or_else: no search answer is every part at its hard limit
    let mut chosen: Vec<(&'static str, Choice)> = parts.iter().zip(&picks).map(|(part, &pick)| (part.id, part.options[pick].clone())).collect();
    // The grounding rides the tail after her view, so it may trade with her reply's real priority
    // and want: searched again with the head fixed at the levels just chosen.
    if searched.is_some() {
        let head_fixed: Vec<Group> = groups
            .iter()
            .zip(&picks)
            .map(|(g, &pick)| if g.id == id::GROUNDING { g.clone() } else { Group { options: vec![g.options[pick]], ..g.clone() } })
            .collect();
        if let (Ok(found), Some(at)) = (search(window, &head_fixed, &continuous, first_guess(window, &head_fixed, &continuous)), parts.iter().position(|part| part.id == id::GROUNDING)) {
            chosen[at] = (id::GROUNDING, parts[at].options[found.picks[at]].clone());
        }
    }
    // Both ways: what the search would show a held head part at unheld. It rises at the next start
    // of her head's epoch, never mid-epoch.
    let held_any = parts.iter().any(|part| part.options.len() == 1 && part.leveled.options().len() > 1);
    let rise = if held_any {
        let (free, _) = parts_with(false);
        { let free_groups = groups_of(&free); search(window, &free_groups, &searched_claims, first_guess(window, &free_groups, &searched_claims)) }.map_or_else(|_| Vec::new(), |found| { // map_or_else: unheld fits no better, nothing rises
            free.iter()
                .zip(&found.picks)
                .filter(|(part, _)| part.id != id::GROUNDING)
                .filter_map(|(part, &pick)| {
                    let option = &part.options[pick];
                    let held = chosen.iter().find(|(id, _)| *id == part.id).map(|(_, c)| c.cost)?;
                    (option.cost > held).then(|| (part.id, option.clone()))
                })
                .collect()
        })
    } else {
        Vec::new()
    };
    let cost_of = |id: &str| chosen.iter().find(|(c, _)| *c == id).map_or(0, |(_, choice)| choice.cost); // map_or: every leveled part is chosen
    let mut all = vec![fixed(id::FRAMING, cost_of(id::FRAMING)), fixed(id::REQUIRED, required.want())];
    // An act with no result makes no newest claim.
    if !newest.is_empty() {
        all.push(fixed(id::NEWEST, newest_choice.cost));
    }
    all.push(fixed(id::TOOLS, cost_of(id::TOOLS)));
    all.push(fixed(id::LAST_MOVE, last_move.want()));
    // Grounding is enrichment: its chosen levels are its claim, never a reason to refuse a turn.
    all.push(FlexClaim { required: false, priority: GROUNDING_PRIORITY, ..fixed(id::GROUNDING, cost_of(id::GROUNDING)) });
    all.extend(continuous.iter().cloned());
    let plan = flex_allocate(window, &all);
    // Each hard limit as it binds: the leveled parts at their limits, the required continuous
    // floors, the reply at its uncapped need.
    let mut limits: Vec<Group> = groups.iter().map(|g| Group { options: vec![g.options.last().copied().unwrap_or(0)], ..g.clone() }).collect(); // unwrap_or: a group always has an option
    for (part, group) in parts.iter().zip(limits.iter_mut()) {
        group.options = vec![if part.id == id::GROUNDING { 0 } else { part.leveled.hard_limit() }];
    }
    limits.extend(continuous.iter().filter(|c| c.required).map(|c| Group {
        id: c.source_id.clone(),
        priority: c.priority,
        options: vec![if c.source_id == id::REPLY { reply_need } else { c.floor_tokens }],
    }));
    let hard_limits = vec![
        (id::FRAMING, framing.leveled.hard_limit()),
        (id::REQUIRED, required.hard_limit()),
        (id::NEWEST, newest.hard_limit()),
        (id::TOOLS, tools.leveled.hard_limit()),
        (id::LAST_MOVE, last_move.hard_limit()),
        (id::GROUNDING, 0),
    ];
    chosen.push((id::NEWEST, newest_choice));
    ActPlan { window, plan, claims: all, reply_need, newest_level, limits, hard_limits, chosen, lost, rise, searched, priced: Priced::Unmeasured }
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

    /// Which of her newest result's levels this act shows (an index into its levels: 0 is
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
    ///
    /// Never below her view's REQUIRED floors: the reply's want beyond its grant is taken from what
    /// her view could grow into, never from the collapsed minimum she acts on (Codex on #5779: an
    /// 8,192 window with a 9,000 reply need left a start room of 0 beside a required 600).
    ///
    /// Never MORE than the plan has (Codex: required means refuse if underfunded, never mint
    /// budget): the protected part is what the plan actually granted her required view sources, up
    /// to their floors; only the rest is reduced by the reply's want.
    pub fn start_room(&self) -> u32 {
        let reply_short = self
            .reply()
            .map_or(0, |reply| reply.max_tokens.saturating_sub(self.granted(id::REPLY))); // map_or: no reply claimed wants nothing more
        let protected = self.granted_required_view_floors();
        let growth = self.view_granted().saturating_add(self.room()).saturating_sub(protected);
        protected.saturating_add(growth.saturating_sub(reply_short))
    }

    /// What the plan granted her REQUIRED view sources, each counted up to its floor.
    fn granted_required_view_floors(&self) -> u32 {
        self.plan
            .allocations
            .iter()
            .filter(|a| a.required && a.source_id.starts_with(id::VIEW))
            .map(|a| a.allocated_tokens.min(a.requested_floor))
            .fold(0, u32::saturating_add)
    }

    /// The view sources the allocation could not grant their floor (dropped, never a required
    /// one): a start sheds them, and their items wait for a later start that funds them
    /// (`View::withhold`).
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

    /// Every hard limit (framing's irreducible blocks, the required input, her newest result at its
    /// chosen level, each verb at its pointer, her last move, her view's required floors), charged:
    /// what no reduction goes below.
    pub fn irreducible(&self) -> u32 {
        self.limits.iter().filter(|g| g.id != id::REPLY).flat_map(|g| g.options.first().copied()).fold(0, u32::saturating_add)
    }

    /// `Some` when every hard limit with the reply's MEASURED need (uncapped) exceeds the window:
    /// irreducible growth, after every part has reduced to its hard limit, never answered by
    /// collapsing history or by shedding her view. It names the binding hard limit. This act still
    /// proceeds, the reply at what the hard limits leave (a cut answer is continued and visible in
    /// its receipt); the consumer of the report is admission (ONE-ACT stage 3/4: her need grows,
    /// admission places her on a larger window, or she waits).
    pub fn irreducible_shortfall(&self) -> Option<IrreducibleShortfall> {
        overflow(self.window, &self.limits, &[]).map(|over| IrreducibleShortfall {
            needed: over.needed,
            window: over.window,
            binding: over.binding,
            binding_limit: over.binding_limit,
        })
    }

    /// The levels the allocation chose for the leveled part `id` (framing, tools, grounding,
    /// newest): one rung per item, or no rungs when it left the turn.
    pub fn chosen(&self, id: &str) -> Option<&Choice> {
        self.chosen.iter().find(|(part, _)| *part == id).map(|(_, choice)| choice)
    }

    /// The leveled part `id`'s hard limit: what it shows at its least (grounding leaves the turn).
    /// `None` for a part that is not leveled (history, the reply, a view source).
    pub fn hard_limit(&self, id: &str) -> Option<u32> {
        self.hard_limits.iter().find(|(part, _)| *part == id).map(|(_, limit)| *limit)
    }

    /// Whether the head part `id`'s held levels were lost this act (the window could not hold them).
    pub fn lost(&self, id: &str) -> bool {
        self.lost.contains(&id)
    }

    /// The levels the held head part `id` rises to at the next start of her head's epoch.
    pub fn rise(&self, id: &str) -> Option<&Choice> {
        self.rise.iter().find(|(part, _)| *part == id).map(|(_, choice)| choice)
    }
}

/// One view source at a start: its floor, whether that floor is required, and the grant the one
/// allocation gave it. A `fixed` source (any priority above the lowest: identity, policy, standing
/// blocks) starts whole at its grant; the lowest-priority sources (her conversation, results,
/// thoughts) grow by appends and share what the start leaves them. A `required` floor (her view's
/// collapsed minimum on an act, or a source that is required in itself) is never shared away.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartSource {
    pub priority: u8,
    pub fixed: bool,
    pub required: bool,
    pub floor: u32,
    pub grant: u32,
    /// What its newest item costs whole: a growing source holds it before the rest is shared, so
    /// her newest end never depends on how the shares divide (goal 7).
    pub newest: u32,
}

/// A view start whose REQUIRED floors the plan did not fund: a typed refusal, never a silent drop
/// of a floor she acts on and never budget minted to cover it (Codex on #5779: "required means
/// refuse if underfunded, not mint budget"). It refuses the turn for capacity; it is distinct from
/// the reply-only [`IrreducibleShortfall`], with which an act still proceeds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartRefused {
    /// A required source the plan granted less than its floor.
    Underfunded { floor: u32, grant: u32 },
    /// Required floors that together exceed the room the start has.
    OverRoom { required: u32, start: u32 },
}

impl StartRefused {
    /// What the start needed and what it had, for the turn's capacity refusal.
    pub fn needed_and_had(self) -> (u32, u32) {
        match self {
            Self::Underfunded { floor, grant } => (floor, grant),
            Self::OverRoom { required, start } => (required, start),
        }
    }
}

/// Each view source's share of a start of `start` tokens, arithmetic on the one plan's grants
/// (never a second allocation): every REQUIRED floor first (each funded by its grant and all within
/// the start, or the typed [`StartRefused`]); then every optional floor, in priority order, while
/// the start holds it (an optional floor that does not fit starts that source empty); then the
/// fixed sources up to their grant, in priority order; then what is left shared among the growing
/// sources: first each one's newest item whole, then the rest in proportion to what each lacks of
/// its grant. No source ever exceeds its grant.
pub fn start_shares(start: u32, sources: &[StartSource]) -> Result<Vec<u32>, StartRefused> {
    if let Some(s) = sources.iter().find(|s| s.required && s.grant < s.floor) {
        return Err(StartRefused::Underfunded { floor: s.floor, grant: s.grant });
    }
    let required: u32 = sources.iter().filter(|s| s.required).map(|s| s.floor).fold(0, u32::saturating_add);
    if required > start {
        return Err(StartRefused::OverRoom { required, start });
    }
    let mut order: Vec<usize> = (0..sources.len()).collect();
    order.sort_by(|&a, &b| sources[b].priority.cmp(&sources[a].priority));
    let mut shares = vec![0u32; sources.len()];
    let mut left = start - required;
    for &i in order.iter().filter(|&&i| sources[i].required) {
        shares[i] = sources[i].floor;
    }
    for &i in order.iter().filter(|&&i| !sources[i].required) {
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
    // Each growing source's newest item, whole, before the proportional share: shared in
    // proportion to what each lacks of its grant, a grant the reply's priority moved left her
    // conversation 1 char short of her newest turn on an act and not on the pass before it
    // (`reply_priority_grows_the_reply_not_at_history_or_required_input`, 2026-11-06).
    for &i in order.iter().filter(|&&i| !sources[i].fixed) {
        let more = sources[i].newest.min(sources[i].grant).saturating_sub(shares[i]).min(left);
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
    Ok(shares)
}

#[cfg(test)]
mod tests;
