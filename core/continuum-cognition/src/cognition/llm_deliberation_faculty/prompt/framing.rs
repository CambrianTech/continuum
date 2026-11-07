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
}

impl FramingOffer {
    /// Which categories the levels `rungs` (one per item, the whole item first) expand.
    pub(in super::super) fn expanded(&self, rungs: &[usize]) -> BTreeSet<String> {
        self.importance
            .iter()
            .zip(rungs.iter().skip(1))
            .filter(|(_, &rung)| rung == 0)
            .map(|(&i, _)| self.lines[i].category.clone())
            .collect()
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
            let line = &lines[i];
            let expanded = charge(line_cost(&line.expanded));
            let (rungs, _) = Rungs::kept(expanded, (), [(charge(line_cost(&line.collapsed) + 1), ())]);
            items.push(rungs);
        }
        let mut offer = FramingOffer { lines, importance, leveled: Leveled::new(items), held: None };
        offer.held = held.map(|expanded| offer.rungs_of(expanded));
        offer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: framing treated as one irreducible block (Aiko on the M5, 2026-11-07:
    // framing 2,795 tokens among the "irreducible floors" that refused her turn), or a menu whose
    // chosen levels do not round-trip to the categories rendered. Its menu categories reduce to
    // their handles, the parts never under-count the system message they render, and the
    // categories her native surface already covers give up their detail first.
    #[test]
    fn the_menu_reduces_by_category_and_its_parts_cover_the_system_message() {
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
        let faculty = LlmDeliberationFaculty::new(uuid::Uuid::new_v4(), "Asha", "You are Asha.", adapter).with_tools(tools);
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
}
