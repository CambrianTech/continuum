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
//! turnovers ([`HeldHands`]) and change only:
//! - when the window cannot hold them (`delib.tools.epoch_level_lost`): re-chosen within it, and
//!   held;
//! - when the moves her held cards give her change (`delib.tools.epoch_moves_changed`), or the
//!   window now holds them at more detail (the allocation's rise: both ways, no collapse is
//!   permanent), at the next start of her view that funds what the new head adds, never mid-epoch:
//!   a card's transition is a new epoch of her hands, one cached prefix (STATE-DRIVEN-SURFACE.md);
//! - when her surface is rebuilt (`rebuild_tool_surface` clears it).
//!
//! # A held head sends the bytes it froze
//!
//! Each epoch of her head freezes its exact schemas (`hands_custody`, card 9cf06ade): its verbs
//! whole, to price levels from, and its payload at its levels, sent by every act of the epoch
//! without being rendered again, and checkpointed with her view, so a restart or a deploy that
//! changed a schema sends the bytes her engine holds. A verb whose live contract moved is told
//! to her as a notice instead (`context_streams::tool_contracts`).
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

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use continuum_context::allocation::{id, ActPlan, Head};

use super::framing::FramingOffer;
use super::*;
use crate::cognition::hands_custody::{HeldHands, LiveContract};
use crate::cognition::tool_surface::{SelectedSurface, ToolSet, ToolsLeveled};

/// This act's hands before the allocation: her surface whole, its verbs as a consumer, and the
/// levels her head holds.
pub(in super::super) struct ToolsOffer {
    surface: SelectedSurface,
    set: ToolSet,
    leveled: ToolsLeveled,
    /// The held levels as rungs of `leveled`, when her head holds this set.
    held: Option<Vec<usize>>,
    kept: bool,
    moves: Vec<String>,
    /// The set her changed moves select, adopted at her head's next epoch.
    adopt: Option<SelectedSurface>,
    /// Her held head as it froze, when she holds one.
    frozen: Option<HeldHands>,
}

impl ToolsOffer {
    /// Her hands as the allocation's head part: what she acts through, so a price choice never shows
    /// a verb below its summary (`continuum_context::ladder::Consumer::Actionable`).
    pub(in super::super) fn head(&self) -> Head {
        Head { leveled: self.leveled.leveled.clone(), held: self.held.clone(), consumer: continuum_context::ladder::Consumer::Actionable }
    }

    /// The canonical names her surface sends.
    pub(in super::super) fn names(&self) -> &[String] {
        &self.set.names
    }
}

/// The head's next epoch: the surface and menu it adopts, and what it adds to the head (in the
/// allocation's unit, measured from the rendered heads), which a start of her view must fund before
/// it is adopted.
pub(in super::super) struct NextHead {
    surface: SelectedSurface,
    levels: Vec<usize>,
    menu: BTreeSet<String>,
    pub(in super::super) growth: usize,
}

/// This act's hands after the allocation.
pub(in super::super) struct ToolsChoice {
    /// What the budget charges and the request sends.
    pub(in super::super) sent: SelectedSurface,
    levels: Vec<usize>,
    kept: bool,
    moves: Vec<String>,
    /// Her menu's expanded categories this act.
    pub(in super::super) menu: BTreeSet<String>,
    /// What her head adopts at its next epoch: her changed moves' set, or her held levels risen.
    pub(in super::super) next: Option<NextHead>,
}

impl LlmDeliberationFaculty {
    /// This act's hands as a consumer: her held set while her head holds one (the verbs her new
    /// moves add ride the tail until her head's next epoch), else this act's selection. Demand
    /// never depends on a grant.
    pub(in super::super) fn tools_offer(&self, ws: &Workspace, charge: &dyn Fn(usize) -> u32) -> ToolsOffer {
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
                // Her held head as it froze: its own schemas, never re-read from the live surface,
                // so a restart or a changed registry sends the bytes her engine holds.
                let mut sent = SelectedSurface::frozen_whole(&held.whole, held.reason);
                sent.tail = fresh.tail.clone(); // the verbs she loaded ride the tail, outside the prefix
                let mut adopt = None;
                if changed {
                    // The verbs her new moves add that the held head lacks ride the tail now.
                    let added = self.surface.names_of(&fresh).into_iter().filter(|name| !held.names.contains(name)).collect::<Vec<_>>();
                    let riding = self.surface.retained(&added, fresh.reason, 0).specs.as_deref().map(<[_]>::to_vec).unwrap_or_default(); // unwrap_or_default: a surface with no tools adds no verbs
                    let riding: Vec<_> = riding.into_iter().filter(|spec| !sent.tail.iter().any(|loaded| loaded.name == spec.name)).collect();
                    sent.tail.extend(riding);
                    adopt = Some(fresh);
                }
                (sent, true, adopt)
            }
            None => (fresh, false, None),
        };
        // A held head is named as it froze, so a verb the live surface no longer offers keeps its place.
        let set = match held.as_ref() {
            Some(held) => self.tool_set_named(&held.names, &surface, &moves),
            None => self.tool_set_of(&surface, &moves),
        };
        let leveled = set.leveled(charge);
        let held_rungs = held.as_ref().and_then(|held| set.rungs_of(&leveled, &held.levels));
        ToolsOffer { surface, set, leveled, held: held_rungs, kept, moves, adopt, frozen: held }
    }

    /// `surface` as a consumer, its verbs ranked by her held cards' moves and her learned head.
    fn tool_set_of(&self, surface: &SelectedSurface, moves: &[String]) -> ToolSet {
        self.tool_set_named(&self.surface.names_of(surface), surface, moves)
    }

    /// [`Self::tool_set_of`] with `surface`'s canonical names given.
    fn tool_set_named(&self, names: &[String], surface: &SelectedSurface, moves: &[String]) -> ToolSet {
        let head = self.working_memory.as_ref().and_then(|wm| wm.tool_head().held()).unwrap_or_default(); // unwrap_or_default: no learned head ranks no verb by use
        self.surface.tool_set_named(names, surface, moves, &head)
    }

    /// The surface the allocation's levels buy, and what her head adopts at its next epoch. A
    /// window that could no longer hold her held levels re-chose them, and says so.
    pub(in super::super) fn granted_tools(&self, ws: &Workspace, offer: ToolsOffer, framing: &FramingOffer, act: &ActPlan, charge: &dyn Fn(usize) -> u32) -> ToolsChoice {
        let ToolsOffer { surface, set, leveled, held, kept, moves, adopt, frozen } = offer;
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
        // A held head at its held levels sends the payload it froze, shared, never rendered again;
        // its whole was priced at the freeze. Anything else renders once, and is frozen when held.
        let held_at_levels = frozen.as_ref().filter(|held| kept && held.levels == levels);
        let demand = held_at_levels.map_or_else(|| set.render(&set.whole()).1, |held| held.demand_tokens); // map_or_else: a head not held at these levels is priced from its schemas
        let sent = match held_at_levels {
            Some(held) => {
                let mut sent = SelectedSurface::frozen_sent(&held.sent, held.sent_tokens, held.reason, demand);
                sent.tail = surface.tail.clone();
                sent
            }
            None => surface.clone().at_levels(&set, &levels).with_demand(demand),
        };
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
    fn head_tokens(&self, ws: &Workspace, menu: &BTreeSet<String>, surface: &SelectedSurface) -> usize {
        let system = self.compose_system_split(menu, ws.directed_at_self(), ws.self_initiated, ws.now_ms, Self::holds_live_work(ws)).stable;
        Self::framing_cost(&system).saturating_add(surface.tokens)
    }

    /// After her view is fitted: the surface this act sends. Her held set goes out at its held
    /// levels unless this act started an epoch of her view that funded her head's next epoch
    /// (`head_funded`): then the next head (her changed moves' set, or her levels risen) is adopted
    /// and held. An act with no held set (the first, or one whose levels were re-chosen) holds what
    /// it sends.
    pub(in super::super) fn retain_tools(&self, choice: ToolsChoice, refused: bool, head_funded: bool) -> (SelectedSurface, BTreeSet<String>) {
        let ToolsChoice { sent, levels, kept, moves, menu, next } = choice;
        if refused {
            return (sent, menu); // a refused turn sent no surface
        }
        let held = self.tools_epoch.lock().clone();
        let (sent, menu, hands) = match (next, held) {
            // This act started an epoch of her view that funded her head's next epoch: adopted.
            (Some(next), held) if head_funded => {
                let hands = self.freeze(&next.surface, next.levels, next.menu.clone(), moves, held.as_ref());
                (next.surface, next.menu, hands)
            }
            // Her head holds its set: a changed set waits for her head's next epoch (said once);
            // levels the window re-chose are the same frozen verbs at new levels, a new epoch.
            (next, Some(mut held)) if kept => {
                held.waiting = if next.is_some() { moves } else { Vec::new() };
                if held.levels != levels {
                    held.levels = levels;
                    held.sent = sent.specs.clone().unwrap_or_else(|| Arc::from(Vec::new())); // unwrap_or_else: a surface with no tools freezes none
                    held.sent_tokens = sent.tokens;
                    held.generation += 1;
                }
                held.menu = menu.clone();
                (sent, menu, held)
            }
            // No head held (her first act, a rebuilt surface, levels lost): what this act sends.
            (_, held) => {
                let hands = self.freeze(&sent, levels, menu.clone(), moves, held.as_ref());
                (sent, menu, hands)
            }
        };
        *self.tools_epoch.lock() = Some(hands);
        self.keep_custody();
        (sent, menu)
    }

    /// A new epoch of her head from what `sent` carries: its verbs whole and its payload as sent,
    /// each verb's contract as the live surface has it now, nothing told yet.
    fn freeze(&self, sent: &SelectedSurface, levels: Vec<usize>, menu: BTreeSet<String>, moves: Vec<String>, previous: Option<&HeldHands>) -> HeldHands {
        let names = self.surface.names_of(sent);
        let none = || Arc::from(Vec::new());
        let whole = self.surface.retained(&names, sent.reason, 0).specs.unwrap_or_else(none); // unwrap_or_else: a surface with no tools freezes none
        let frozen: BTreeMap<_, _> = names
            .iter()
            .filter_map(|name| match self.surface.live_contract(name) {
                LiveContract::Offered { id, .. } => Some((name.clone(), id)),
                LiveContract::Removed => None,
            })
            .collect();
        let hands = HeldHands {
            generation: previous.map_or(1, |previous| previous.generation + 1), // map_or: her first head is generation 1
            names,
            levels,
            reason: sent.reason,
            moves,
            waiting: Vec::new(),
            menu,
            whole,
            sent: sent.specs.clone().unwrap_or_else(none), // unwrap_or_else: a surface with no tools freezes none
            sent_tokens: sent.tokens,
            demand_tokens: sent.demand_tokens(),
            frozen,
            told: BTreeMap::new(),
        };
        continuum_base::probe!(
            class = "tools.custody.frozen",
            persona = %self.persona_name,
            generation = hands.generation,
            verbs = hands.names.len(),
            sent_tokens = hands.sent_tokens,
            "a new epoch of her head: its exact schemas are frozen and sent by every act until the next"
        );
        hands
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
            framing: Head { held: Some(floor.rungs), ..Head::free(framing.leveled.clone()) },
            tools: offer.head(), required: Leveled::default(), newest: continuum_context::allocation::Newest::Absent,
            grounding: Leveled::default(), last_move: Leveled::default(), history: Span::default(),
            reply: FlexClaim { source_id: id::REPLY.into(), priority: 1, floor_tokens: 0, min_tokens: 0, max_tokens: 0, required: false },
            view: Vec::new(),
            price: continuum_context::price::ContextPrice::Unmeasured,
            trained: continuum_context::allocation::TrainedContext::Unreported,
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
