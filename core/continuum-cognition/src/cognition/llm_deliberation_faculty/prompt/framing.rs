//! Her framing as a budget consumer: what it is made of and how each part reduces.
//!
//! The system message holds her identity, `[Taking your turn]`, `[Your tools]` with its menu, and
//! `[Acting]`. All of it is one item that cannot reduce (its hard limit is its whole: who she is
//! and how a turn works), EXCEPT the menu's categories: each is an item whose whole lists its
//! verbs and whose pointer is its count and the handle that opens it
//! (`persona_tools::menu_lines`, `commands/list --filter <category>`). A category whose every
//! verb her native surface already sends gives up its detail first (the schemas name them
//! already); the rest keep theirs longer. Every cost is measured from the rendered text at the
//! guard rate, each part rounded up, so the parts together never under-count the whole.
//!
//! The menu sits in the system message, at the prompt's head: its levels are held with her hands'
//! for the epoch of her head (`tools_epoch.rs`), and change only where her hands' do.

use std::collections::BTreeSet;

use continuum_context::ladder::{Leveled, Rungs};

use super::*;
use crate::cognition::persona_tools::{menu_lines, MenuLine};

/// This act's framing as the allocation's consumer.
pub(in super::super) struct FramingOffer {
    lines: Vec<MenuLine>,
    /// Indices into `lines`, least important first: the items' order after the whole item.
    importance: Vec<usize>,
    pub(in super::super) leveled: Leveled,
    /// The held levels, one rung per item (the whole item's first), when her head holds them.
    pub(in super::super) held: Option<Vec<usize>>,
    /// The categories her head holds expanded. A category whose two levels charge the same at this
    /// act's density has one rung, and that rung renders as she holds it: her choice is semantic, never
    /// decided by integer costs that happen to round together (Codex on #6118: at density 1/10 the
    /// plasticity collapse and expansion both charge 2).
    held_menu: Option<BTreeSet<String>>,
}

impl FramingOffer {
    /// Which categories the levels `rungs` (one per item, the whole item first) expand.
    pub(in super::super) fn expanded(&self, rungs: &[usize]) -> BTreeSet<String> {
        self.importance
            .iter()
            .zip(rungs.iter().skip(1))
            .zip(self.leveled.items().iter().skip(1))
            .filter(|((&i, &rung), item)| rung == 0 && (item.costs().len() > 1 || self.holds_expanded(&self.lines[i].category)))
            .map(|((&i, _), _)| self.lines[i].category.clone())
            .collect()
    }

    /// Whether her head holds `category` expanded; with no held menu (an epoch's start) every
    /// category is.
    fn holds_expanded(&self, category: &str) -> bool {
        self.held_menu.as_ref().is_none_or(|held| held.contains(category))
    }

    /// The rungs `expanded` is, one per item: what her head holds.
    pub(in super::super) fn rungs_of(&self, expanded: &BTreeSet<String>) -> Vec<usize> {
        // The category items follow the whole item, in importance order; a category whose collapse
        // is no smaller than its expansion has one rung, its expansion.
        std::iter::once(0)
            .chain(self.importance.iter().zip(self.leveled.items().iter().skip(1)).map(|(&i, rungs)| {
                usize::from(!expanded.contains(&self.lines[i].category) && rungs.costs().len() > 1)
            }))
            .collect()
    }
}

impl LlmDeliberationFaculty {
    /// Her framing this act as a consumer: its irreducible blocks whole, then each menu category
    /// at its levels. `sent` is what her native surface sends (canonical names); `held` the menu
    /// categories her head's epoch holds expanded.
    pub(in super::super) fn framing_offer(
        &self,
        ws: &Workspace,
        holds_live_work: bool,
        sent: &[String],
        held: Option<&BTreeSet<String>>,
        charge: &dyn Fn(usize) -> u32,
    ) -> FramingOffer {
        let lines = if self.tools.is_empty() { Vec::new() } else { menu_lines(&self.tools) };
        let whole = self.expanded_categories(ws);
        let system = self.compose_system_split(&whole, ws.directed_at_self(), ws.self_initiated, ws.now_ms, holds_live_work).stable;
        // The whole is exact: what the menu's expanded lines cost apart is taken from the whole's
        // own measured cost, so every category expanded prices the system message exactly. A
        // collapsed line is priced one token above its own rounding, so a reduction never
        // under-counts what renders (the whole's cost rounds down, each line's up).
        let line_cost = |text: &str| text.len().div_ceil(GUARD_CHARS_PER_TOKEN);
        let rest = Self::framing_cost(&system).saturating_sub(lines.iter().map(|line| line_cost(&line.expanded)).sum());
        let mut importance: Vec<usize> = (0..lines.len()).collect();
        let covered = |line: &MenuLine| line.verbs.iter().all(|verb| sent.contains(verb));
        importance.sort_by_key(|&i| (u8::from(!covered(&lines[i])), i));
        let mut items = vec![Rungs::whole(charge(rest))];
        for &i in &importance {
            items.push(lines[i].levels(charge));
        }
        let mut offer = FramingOffer { lines, importance, leveled: Leveled::new(items), held: None, held_menu: held.cloned() };
        offer.held = held.map(|expanded| offer.rungs_of(expanded));
        offer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A faculty whose menu has three categories of eight verbs each.
    fn faculty() -> LlmDeliberationFaculty {
        let names: Vec<String> = ["code", "work", "chat"].iter().flat_map(|cat| ["read", "edit", "write", "search", "status", "list", "send", "submit"].iter().map(move |verb| format!("{cat}/{verb}"))).collect();
        let tools: Vec<crate::ai::types::NativeToolSpec> = names
            .iter()
            .map(|name| crate::ai::types::NativeToolSpec {
                name: name.clone(),
                description: format!("{name}: does what its name says"),
                input_schema: crate::ai::types::ToolInputSchema { schema_type: "object".into(), properties: serde_json::json!({}), required: None, definitions: None },
            })
            .collect();
        let adapter: std::sync::Arc<dyn crate::ai::adapter::AIProviderAdapter> = std::sync::Arc::new(crate::ai::heuristic_adapter::HeuristicInferenceAdapter::new());
        LlmDeliberationFaculty::new(uuid::Uuid::new_v4(), "Asha", "You are Asha.", adapter).with_tools(tools)
    }

    // what this catches: framing treated as one irreducible block (Aiko on the M5, 2026-11-07:
    // framing 2,795 tokens among the "irreducible floors" that refused her turn), or a menu whose
    // chosen levels do not round-trip to the categories rendered. Its menu categories reduce to
    // their handles, the parts never under-count the system message they render, and the
    // categories her native surface already covers give up their detail first.
    #[test]
    fn the_menu_reduces_by_category_and_its_parts_cover_the_system_message() {
        let faculty = faculty();
        let ws = Workspace::new("what changed on the board?");
        let charge = |tokens: usize| tokens as u32;
        let offer = faculty.framing_offer(&ws, false, &[], None, &charge);
        assert!(offer.leveled.items().len() > 1, "a menu with categories");
        assert!(offer.leveled.hard_limit() < offer.leveled.want(), "categories collapse below the whole");
        let all = offer.leveled.reduce(offer.leveled.want());
        let expanded = offer.expanded(&all.rungs);
        assert_eq!(expanded, faculty.expanded_categories(&ws), "the whole is every category expanded");
        assert_eq!(offer.rungs_of(&expanded), all.rungs);
        let floor = offer.leveled.at_limit();
        let collapsed = offer.expanded(&floor.rungs);
        let system = faculty.compose_system_split(&collapsed, false, false, None, false).stable;
        assert!(LlmDeliberationFaculty::framing_cost(&system) as u32 <= floor.cost, "the parts never under-count what renders");
        let whole_system = faculty.compose_system_split(&expanded, false, false, None, false).stable;
        assert!(LlmDeliberationFaculty::framing_cost(&whole_system) as u32 <= all.cost);
    }

    // what this catches (Codex on #6118): a held collapsed category rewritten to expanded because its
    // two levels charge the same integer at this act's density (1/10 rounds both to the same cost),
    // which changes her system prompt mid-epoch. Her held menu renders the same at density 1 and 1/10.
    #[test]
    fn a_held_menu_survives_levels_that_charge_the_same() {
        let faculty = faculty();
        let ws = Workspace::new("what changed on the board?");
        let held: BTreeSet<String> = BTreeSet::new(); // her head holds every category collapsed
        for (measured, estimated) in [(1u64, 1u64), (1, 10)] {
            let charge = move |tokens: usize| (tokens as u64 * measured).div_ceil(estimated) as u32;
            let offer = faculty.framing_offer(&ws, false, &[], Some(&held), &charge);
            let rungs = offer.held.clone().expect("test: her head holds a menu");
            assert_eq!(offer.expanded(&rungs), held, "density {measured}/{estimated}: the held menu renders as held");
        }
    }

    // what this catches (Benchy on the M5, 2026-11-17 22:12Z): a menu category whose collapsed level
    // exists at one calibration and not at the next, so her held menu renders differently mid-epoch
    // and her whole KV re-prefills. At density 1 and at the measured 0.899 this category keeps its collapse (lower densities may still
    // coalesce it; a_held_menu_survives_levels_that_charge_the_same covers that case).
    #[test]
    fn a_held_menu_renders_the_same_whatever_the_calibration() {
        let line = crate::cognition::persona_tools::menu_lines(&[crate::ai::types::NativeToolSpec {
            name: "plasticity/analyze".into(),
            description: String::new(),
            input_schema: crate::ai::types::ToolInputSchema { schema_type: "object".into(), properties: serde_json::json!({}), required: None, definitions: None },
        }])
        .remove(0);
        let line = MenuLine {
            expanded: "plasticity: analyze, compact, compress, pipeline, topology\n".into(),
            collapsed: "plasticity (5 \u{2014} commands/list --filter plasticity)\n".into(),
            ..line
        };
        let at = |measured: u64, estimated: u64| move |tokens: usize| (tokens as u64 * measured).div_ceil(estimated) as u32;
        let (raw, measured) = (at(1, 1), at(14_659, 16_305));
        let before = line.levels(&raw);
        let after = line.levels(&measured);
        assert_eq!(before.costs().len(), 2, "its collapse is smaller text: a level");
        assert_eq!(after.costs().len(), before.costs().len(), "at the measured 0.899 its collapse is still a level");
        let collapsed_text = line.collapsed.len().div_ceil(GUARD_CHARS_PER_TOKEN);
        assert!(after.costs()[1] >= measured(collapsed_text), "a collapse never prices below its own text");
    }
}
