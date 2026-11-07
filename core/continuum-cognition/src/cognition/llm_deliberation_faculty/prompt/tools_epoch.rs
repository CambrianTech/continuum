//! Her hands as one consumer in the window's one allocation, and one SET per epoch of her head
//! (plan goal 10: tools are working sets).
//!
//! Each verb is an item with levels generated from its schema (`tool_surface::levels`: whole,
//! summary, pointer), so a window too small for her verbs whole shows them smaller, never refuses
//! the turn: her floor is every verb at its pointer (its name, its parameters' names and the
//! `commands/help` handle that opens it), and she is never handless (Aiko on the M5, 2026-11-07:
//! her core hands whole, 11,377 tokens, refused her turns on a 30,566 window). Which verbs are her
//! hands stays the surface's decision; how much of each she sees is the allocation's, searched with
//! every other part of her prompt (`continuum_context::search`).
//!
//! # The set outlives her view's turnovers
//!
//! Keeping the level alone still re-selected the set every act from that act's affordances (a
//! dirty worktree offers `git_diff`, a held card `get_task`): Aiko on the M5, 2026-10-29, 19 of
//! 89 requests changed the head, every one in the tool list. Holding the set per epoch of her
//! VIEW still re-selected it at every turnover, from that act's grant, and a turnover starts the
//! claim at her core hands: on the M5, 2026-10-31 on 526bbe418, Aris's set flipped between her
//! 38-verb hands and her 23-verb core within one room, and every cold prefill in two hours (13 of
//! 13, Aris and Benchy, ~40k tokens at 100+ s each) followed a head change, while every restore
//! behind an unchanged head came back warm. A view turnover re-renders only what follows the
//! head; re-selecting the head there turned a view re-prefill into a whole one.
//!
//! So the set AND its levels (her verbs' and her menu's, `framing.rs`) are held across her view's
//! turnovers ([`ToolsEpoch`]) and change only:
//! - when the window cannot hold them (`delib.tools.epoch_level_lost`): re-chosen within it, and
//!   held;
//! - when the moves her held cards give her change (`delib.tools.epoch_moves_changed`), or the
//!   window now holds them at more detail (the allocation's rise: both ways, no collapse is
//!   permanent), at the next start of her view that funds what the new head adds, never mid-epoch:
//!   a card's transition is a new epoch of her hands, one cached prefix (STATE-DRIVEN-SURFACE.md);
//! - when her surface is rebuilt (`rebuild_tool_surface` clears it).
//!
//! # The template puts her hands at the head
//!
//! The served model's chat template (Qwen3.6-35B-A3B's, read from its GGUF on the M5) renders the
//! tool schemas first of all (`<|im_start|>system` then `# Tools` and `<tools>…</tools>`), before
//! her system text: a set that changes rewrites the head and every token behind it. A held card's
//! transition re-selected the set at once, mid-epoch, and the whole prompt went cold. So a changed
//! set waits for her view's next start, where everything after the system text is re-rendered
//! anyway; until then the epoch keeps its set, and the verbs the new moves add ride the tail
//! (`tail_tools`, after her messages, outside the prefix), callable natively at once.
//!
//! The trade, stated: a verb whose affordance appears later, or that her learned head would add,
//! is not in her head until one of those changes. On a server that takes `tail_tools` her learned
//! head's loaded verbs still ride the tail every act (after the messages, never in the prefix),
//! so a verb she loads stays callable at once; on one that does not, `commands/help` plus the
//! executor's dispatch by name still reach it. A held verb whose affordance vanished stays in the
//! set: its command refuses a call that no longer applies.

use std::collections::BTreeSet;

use continuum_context::allocation::{id, ActPlan, Head};

use super::framing::FramingOffer;
use super::*;
use crate::cognition::tool_surface::{SelectedSurface, SurfaceReason, ToolSet, ToolsLeveled};

/// What her head holds for its epoch: her verbs as canonical names in the order sent, each at its
/// level, why, the held-card moves they were selected for, and her menu's expanded categories.
#[derive(Clone, Debug)]
pub(in super::super) struct ToolsEpoch {
    names: Vec<String>,
    /// Each verb's level, in sent order (`tool_surface::VerbLevels`: 0 whole).
    levels: Vec<usize>,
    reason: SurfaceReason,
    /// The held-card moves it was selected for (`HeldWork::move_commands`).
    moves: Vec<String>,
    /// Moves that changed since and whose set waits for her head's next epoch: said once.
    waiting: Vec<String>,
    /// The menu categories her framing shows expanded (`framing.rs`).
    menu: BTreeSet<String>,
}

impl ToolsEpoch {
    /// The menu categories her head holds expanded.
    pub(in super::super) fn menu(&self) -> &BTreeSet<String> {
        &self.menu
    }
}

/// This act's hands before the allocation: her surface whole, its verbs as a consumer, and the
/// levels her head holds.
pub(in super::super) struct ToolsOffer<'a> {
    surface: SelectedSurface<'a>,
    set: ToolSet,
    leveled: ToolsLeveled,
    /// The held levels as rungs of `leveled`, when her head holds this set.
    held: Option<Vec<usize>>,
    kept: bool,
    moves: Vec<String>,
    /// The set her changed moves select, adopted at her head's next epoch.
    adopt: Option<SelectedSurface<'a>>,
}

impl ToolsOffer<'_> {
    /// Her hands as the allocation's head part.
    pub(in super::super) fn head(&self) -> Head {
        Head { leveled: self.leveled.leveled.clone(), held: self.held.clone() }
    }

    /// The canonical names her surface sends.
    pub(in super::super) fn names(&self) -> &[String] {
        &self.set.names
    }
}

/// The head's next epoch: the surface and menu it adopts, and what it adds to the head (in the
/// allocation's unit, measured from the rendered heads), which a start of her view must fund before
/// it is adopted.
pub(in super::super) struct NextHead<'a> {
    surface: SelectedSurface<'a>,
    levels: Vec<usize>,
    menu: BTreeSet<String>,
    pub(in super::super) growth: usize,
}

/// This act's hands after the allocation.
pub(in super::super) struct ToolsChoice<'a> {
    /// What the budget charges and the request sends.
    pub(in super::super) sent: SelectedSurface<'a>,
    levels: Vec<usize>,
    kept: bool,
    moves: Vec<String>,
    /// Her menu's expanded categories this act.
    pub(in super::super) menu: BTreeSet<String>,
    /// What her head adopts at its next epoch: her changed moves' set, or her held levels risen.
    pub(in super::super) next: Option<NextHead<'a>>,
}

impl LlmDeliberationFaculty {
    /// This act's hands as a consumer: her held set while her head holds one (the verbs her new
    /// moves add ride the tail until her head's next epoch), else this act's selection. Demand
    /// never depends on a grant.
    pub(in super::super) fn tools_offer(&self, ws: &Workspace, charge: &dyn Fn(usize) -> u32) -> ToolsOffer<'_> {
        let held = self.tools_epoch.lock().clone();
        let moves = ws.held.move_commands();
        let fresh = self.select_tool_surface(ws);
        let changed = held.as_ref().is_some_and(|held| held.moves != moves);
        let (surface, kept, adopt) = match held.as_ref() {
            Some(held) => {
                if changed && held.waiting != moves {
                    continuum_base::probe!(
                        class = "delib.tools.epoch_moves_changed",
                        persona = %self.persona_name,
                        held_moves = %held.moves.join(","),
                        moves = %moves.join(","),
                        "a held card's transition changed the moves in her hands: her tool set changes at her head's next epoch; its new verbs ride the tail until then"
                    );
                }
                let mut sent = self.surface.retained(&held.names, held.reason, 0);
                sent.tail = fresh.tail.clone(); // the verbs she loaded ride the tail, outside the prefix
                let mut adopt = None;
                if changed {
                    // The verbs her new moves add that the held head lacks ride the tail now.
                    let head = self.surface.names_of(&sent);
                    let added = self.surface.names_of(&fresh).into_iter().filter(|name| !head.contains(name)).collect::<Vec<_>>();
                    let riding = self.surface.retained(&added, fresh.reason, 0).specs.map(std::borrow::Cow::into_owned).unwrap_or_default(); // unwrap_or_default: a surface with no tools adds no verbs
                    let riding: Vec<_> = riding.into_iter().filter(|spec| !sent.tail.iter().any(|loaded| loaded.name == spec.name)).collect();
                    sent.tail.extend(riding);
                    adopt = Some(fresh);
                }
                (sent, true, adopt)
            }
            None => (fresh, false, None),
        };
        let set = self.tool_set_of(&surface, &moves);
        let leveled = set.leveled(charge);
        let held = held.as_ref().and_then(|held| set.rungs_of(&leveled, &held.levels));
        ToolsOffer { surface, set, leveled, held, kept, moves, adopt }
    }

    /// `surface` as a consumer, its verbs ranked by her held cards' moves and her learned head.
    fn tool_set_of(&self, surface: &SelectedSurface<'_>, moves: &[String]) -> ToolSet {
        let head = self.working_memory.as_ref().and_then(|wm| wm.tool_head().held()).unwrap_or_default(); // unwrap_or_default: no learned head ranks no verb by use
        self.surface.tool_set(surface, moves, &head)
    }

    /// The surface the allocation's levels buy, and what her head adopts at its next epoch. A
    /// window that could no longer hold her held levels re-chose them, and says so.
    pub(in super::super) fn granted_tools<'a>(&'a self, ws: &Workspace, offer: ToolsOffer<'a>, framing: &FramingOffer, act: &ActPlan, charge: &dyn Fn(usize) -> u32) -> ToolsChoice<'a> {
        let ToolsOffer { surface, set, leveled, held, kept, moves, adopt } = offer;
        let rungs = act.chosen(id::TOOLS).map_or_else(|| leveled.leveled.at_limit().rungs, |choice| choice.rungs.clone()); // map_or_else: an unchosen part sits at its hard limit
        let levels = set.levels_of(&leveled, &rungs);
        let menu_rungs = act.chosen(id::FRAMING).map_or_else(|| framing.leveled.at_limit().rungs, |choice| choice.rungs.clone()); // map_or_else: as above
        let menu = framing.expanded(&menu_rungs);
        if act.lost(id::TOOLS) || act.lost(id::FRAMING) {
            continuum_base::probe!(
                class = "delib.tools.epoch_level_lost",
                persona = %self.persona_name,
                held = held.as_ref().map_or(0, |rungs| leveled.leveled.cost_of(rungs)), // map_or: no held set held nothing
                chosen = leveled.leveled.cost_of(&rungs),
                "the window could not hold her head's levels: they were re-chosen within it, and the whole prompt re-prefills"
            );
        }
        let demand = set.render(&set.whole()).1;
        let sent = surface.clone().at_levels(&set, &levels).with_demand(demand);
        // Her learned head reserves the head her next epoch would hold, within this act's grant.
        self.reserve_next_head(ws, act.granted(id::TOOLS));
        let rise_menu = act.rise(id::FRAMING).map(|choice| framing.expanded(&choice.rungs));
        // The next head's parts: her changed moves' set (at the levels this act's tools claim, or
        // its rise, buys) or her held set risen, and her menu risen or as it is.
        let next = match adopt {
            Some(fresh) => {
                let fresh_set = self.tool_set_of(&fresh, &moves);
                let fresh_leveled = fresh_set.leveled(charge);
                let target = act.rise(id::TOOLS).map_or(leveled.leveled.cost_of(&rungs), |rise| rise.cost); // map_or: no rise keeps this act's cost
                let fresh_levels = fresh_set.levels_of(&fresh_leveled, &fresh_leveled.leveled.reduce(target).rungs);
                let fresh_whole = fresh_set.render(&fresh_set.whole()).1;
                Some((fresh.at_levels(&fresh_set, &fresh_levels).with_demand(fresh_whole), fresh_levels, rise_menu.unwrap_or_else(|| menu.clone()))) // unwrap_or_else: no menu rise keeps this act's menu
            }
            None => {
                let rise_levels = act.rise(id::TOOLS).map(|choice| set.levels_of(&leveled, &choice.rungs));
                (rise_levels.is_some() || rise_menu.is_some()).then(|| {
                    let levels = rise_levels.unwrap_or_else(|| levels.clone()); // unwrap_or_else: only the menu rises
                    (surface.at_levels(&set, &levels).with_demand(demand), levels, rise_menu.unwrap_or_else(|| menu.clone())) // unwrap_or_else: only the verbs rise
                })
            }
        };
        // What the next head adds, measured in ONE place for every head part: the head it renders
        // (system message at its menu, plus its tool surface) against this act's, in the
        // allocation's unit, so a rise of any head consumer is funded before it is adopted.
        let now = charge(self.head_tokens(ws, &menu, &sent));
        let next = next.map(|(surface, levels, next_menu)| {
            let growth = charge(self.head_tokens(ws, &next_menu, &surface)).saturating_sub(now) as usize;
            NextHead { surface, levels, menu: next_menu, growth }
        });
        ToolsChoice { sent, levels, kept, moves, menu, next }
    }

    /// What her head costs as it renders: the system message at `menu` and the tool `surface`.
    fn head_tokens(&self, ws: &Workspace, menu: &BTreeSet<String>, surface: &SelectedSurface<'_>) -> usize {
        let system = self.compose_system_split(menu, ws.directed_at_self(), ws.self_initiated, ws.now_ms, Self::holds_live_work(ws)).stable;
        Self::framing_cost(&system).saturating_add(surface.tokens)
    }

    /// After her view is fitted: the surface this act sends. Her held set goes out at its held
    /// levels unless this act started an epoch of her view that funded her head's next epoch
    /// (`head_funded`): then the next head (her changed moves' set, or her levels risen) is adopted
    /// and held. An act with no held set (the first, or one whose levels were re-chosen) holds what
    /// it sends.
    pub(in super::super) fn retain_tools<'a>(&'a self, choice: ToolsChoice<'a>, refused: bool, head_funded: bool) -> (SelectedSurface<'a>, BTreeSet<String>) {
        let ToolsChoice { sent, levels, kept, moves, menu, next } = choice;
        if refused {
            return (sent, menu); // a refused turn sent no surface
        }
        let (sent, levels, menu, moves) = match next {
            Some(next) if head_funded => (next.surface, next.levels, next.menu, moves),
            Some(_) if kept => {
                // Her set waits for her head's next epoch; the change is said once.
                if let Some(held) = self.tools_epoch.lock().as_mut() {
                    held.waiting = moves.clone();
                    held.levels = levels;
                    held.menu = menu.clone();
                }
                return (sent, menu);
            }
            _ => (sent, levels, menu, moves),
        };
        *self.tools_epoch.lock() = Some(ToolsEpoch { names: self.surface.names_of(&sent), levels, reason: sent.reason, moves, waiting: Vec::new(), menu: menu.clone() });
        (sent, menu)
    }
}


#[cfg(test)]
mod review_growth_tests {
    use super::*;
    use continuum_context::allocation::{allocate, ActClaims, Span};
    use continuum_context::ladder::Leveled;
    use continuum_context::flex::FlexClaim;

    #[test]
    fn review_menu_only_rise_funds_the_rendered_head_growth() {
        let names: Vec<String> = ["code", "work", "chat"].iter().flat_map(|cat|
            ["read", "edit", "write", "search", "status", "list", "send", "submit"].iter()
                .map(move |verb| format!("{cat}/{verb}"))).collect();
        let tools = names.iter().map(|name| crate::ai::types::NativeToolSpec {
            name: name.clone(), description: format!("{name}: does what its name says"),
            input_schema: crate::ai::types::ToolInputSchema {
                schema_type: "object".into(), properties: serde_json::json!({}), required: None, definitions: None,
            },
        }).collect();
        let adapter: std::sync::Arc<dyn crate::ai::adapter::AIProviderAdapter> =
            std::sync::Arc::new(crate::ai::heuristic_adapter::HeuristicInferenceAdapter::new());
        let faculty = LlmDeliberationFaculty::new(uuid::Uuid::new_v4(), "Asha", "You are Asha.", adapter).with_tools(tools);
        let ws = Workspace::new("what changed on the board?");
        let charge = |tokens: usize| tokens as u32;
        let offer = faculty.tools_offer(&ws, &charge);
        let framing = faculty.framing_offer(&ws, false, offer.names(), None, &charge);
        let floor = framing.leveled.at_limit();
        assert!(floor.cost < framing.leveled.want(), "fixture: menu can rise");
        let window = (framing.leveled.want() + offer.head().leveled.want()) * 2;
        let act = allocate(window, ActClaims {
            framing: Head { leveled: framing.leveled.clone(), held: Some(floor.rungs) },
            tools: offer.head(), required: Leveled::default(), newest: Leveled::default(),
            grounding: Leveled::default(), last_move: Leveled::default(), history: Span::default(),
            reply: FlexClaim { source_id: id::REPLY.into(), priority: 1, floor_tokens: 0, min_tokens: 0, max_tokens: 0, required: false },
            view: Vec::new(),
            price: continuum_context::price::ContextPrice::Unmeasured,
        });
        assert!(act.rise(id::FRAMING).is_some(), "fixture: actual allocator offers menu rise");
        assert!(act.rise(id::TOOLS).is_none(), "fixture: tools stay at same level");
        let choice = faculty.granted_tools(&ws, offer, &framing, &act, &charge);
        let next = choice.next.as_ref().expect("test: menu rise has next head");
        let before = faculty.compose_system_split(&choice.menu, false, false, None, false).stable;
        let after = faculty.compose_system_split(&next.menu, false, false, None, false).stable;
        let delta = (LlmDeliberationFaculty::framing_cost(&after) + next.surface.tokens)
            .saturating_sub(LlmDeliberationFaculty::framing_cost(&before) + choice.sent.tokens);
        assert!(delta > 0, "fixture: rendered head actually grows");
        assert!(next.growth >= delta, "head funding {} must cover rendered growth {}", next.growth, delta);
    }
}
