//! A standing grounding block (the workspace map, the board, the wall, the roster) as a
//! source of one item: what it says now. A block that changes appends its new version.
//!
//! Its levels come from the source's own units (FLOW-IMPLEMENTATION-PLAN §2d): whole is
//! every unit whole, summary every unit at its summary, pointer every unit's pointer. A unit
//! with no pointer leaves the collapsed block, as `RagItem::pointer` says (`None`: a collapsed
//! item leaves her view entirely); a block none of whose units keep a pointer collapses to its
//! truthful minimum. The block stays one item because each item renders as its own message;
//! choosing a level per unit (keep the best card whole, collapse the rest) waits on merging
//! adjacent items at render.

use continuum_context::source::{Item, Source};

use crate::cognition::rag_budget::RagItem;

use super::{priority, ContextMessage};

pub(crate) struct Grounding {
    /// Whose block this is (its id, `super::standing`).
    pub persona: uuid::Uuid,
    pub faculty: String,
    /// The whole block.
    pub text: String,
    /// What it guarantees when the window is short, for a block with no units: a long
    /// list's first units with a line naming what is omitted; an atomic block whole.
    pub minimum: String,
    /// The source's keyed, levelled units, when it delivered them.
    pub units: std::sync::Arc<[RagItem]>,
}

impl Grounding {
    /// A block with no units that is shown whole or not at all (a contract in force).
    pub fn atomic(persona: uuid::Uuid, faculty: impl Into<String>, text: impl Into<String>) -> Self {
        let text = text.into();
        Self { persona, faculty: faculty.into(), minimum: text.clone(), text, units: std::sync::Arc::from(Vec::new()) }
    }

    /// The block as a standing item: the root of its activity; a board update is new content, not a
    /// new identity.
    fn block(&self, full: ContextMessage, summary: Option<ContextMessage>, pointer: ContextMessage) -> Item<ContextMessage> {
        super::standing(self.persona, &self.faculty, "block", full).with_summary(summary).with_pointer(pointer)
    }

    fn render(&self, body: String) -> ContextMessage {
        ContextMessage::user(format!("[{}]\n{body}", self.faculty))
    }

    /// The block with every unit at the level `pick` chooses.
    fn at(&self, pick: impl Fn(&RagItem) -> &str) -> String {
        self.units.iter().map(pick).collect::<Vec<_>>().join("\n")
    }
}

impl Source<ContextMessage> for Grounding {
    fn name(&self) -> &str {
        &self.faculty
    }

    fn priority(&self) -> u8 {
        priority::GROUNDING
    }

    fn items(&self) -> Vec<Item<ContextMessage>> {
        if self.units.is_empty() {
            // A block is what she works from now (her board, her checkout) with no handle to
            // reopen it by, so collapsed it is still its truthful minimum, never a one-liner.
            let full = self.render(self.text.clone());
            let pointer = self.render(self.minimum.clone());
            return vec![self.block(full, None, pointer)];
        }
        let whole = self.at(|unit| &unit.content);
        let summary = self.units.iter().any(|unit| unit.summary.is_some()).then(|| {
            self.at(|unit| unit.summary.as_deref().unwrap_or(&unit.content)) // a unit with no summary level is its own summary
        });
        let pointer = self.units.iter().filter_map(|unit| unit.pointer.as_deref()).collect::<Vec<_>>().join("\n");
        let summary = summary.filter(|summary| *summary != whole);
        // The units' own pointers stand even when they equal the whole (a strip whose lines are their
        // own pointers stays as it is); only a block that keeps nothing collapses to its minimum
        // (Codex and Cormac on #5681).
        let pointer = (!pointer.is_empty()).then_some(pointer);
        let pointer = pointer.map_or_else(|| self.render(self.minimum.clone()), |pointer| self.render(pointer)); // map_or_else: a block that keeps no unit collapses to its truthful minimum
        vec![self.block(self.render(whole), summary.map(|summary| self.render(summary)), pointer)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grounding(units: Vec<RagItem>) -> Grounding {
        let text = units.iter().map(|u| u.content.clone()).collect::<Vec<_>>().join("\n");
        Grounding { persona: uuid::Uuid::nil(), faculty: "room-board".into(), minimum: text.clone(), text, units: units.into() }
    }

    // what this catches: a source's levels never reaching her view (the block had only
    // whole-or-first-units). Whole is every unit whole, summary every unit's summary, pointer
    // every unit at its cheapest level, each smaller than the last.
    #[test]
    fn a_block_takes_its_levels_from_its_units() {
        let card = |id: &str| {
            RagItem::whole(id, format!("card {id}: a long description of the work"), serde_json::Value::Null)
                .with_summary(format!("card {id}: short"))
                .with_pointer(format!("[card {id}]"))
        };
        let item = grounding(vec![card("a"), card("b")]).items().remove(0);
        let text = |m: &ContextMessage| m.text.clone();
        assert_eq!(text(item.full()), "[room-board]\ncard a: a long description of the work\ncard b: a long description of the work");
        assert_eq!(item.summary().map(text).as_deref(), Some("[room-board]\ncard a: short\ncard b: short"));
        assert_eq!(item.pointer().map(text).as_deref(), Some("[room-board]\n[card a]\n[card b]"));
    }

    // what this catches: a collapsed block that kept every unit, a unit with no pointer falling
    // back to its whole content: 592 one-line card pointers were Benchy's collapsed board (the M5,
    // 2026-11-01). A unit with no pointer leaves the collapsed block; a block that would keep
    // nothing collapses to its minimum.
    #[test]
    fn a_unit_without_a_pointer_leaves_the_collapsed_block() {
        let held = RagItem::whole("a", "card a: hers, long", serde_json::Value::Null).with_pointer("[card a]");
        let other = RagItem::whole("b", "card b: someone else's, long", serde_json::Value::Null);
        let block = Grounding { persona: uuid::Uuid::nil(), faculty: "room-board".into(), minimum: "minimum".into(), text: String::new(), units: vec![held, other.clone()].into() };
        let item = block.items().remove(0);
        assert_eq!(item.pointer().map(|m| m.text.clone()).as_deref(), Some("[room-board]\n[card a]"));
        let own = RagItem::whole("c", "continue the migration", serde_json::Value::Null).with_pointer("continue the migration");
        let strip = Grounding { persona: uuid::Uuid::nil(), faculty: "awareness".into(), minimum: "a different minimum".into(), text: String::new(), units: vec![own].into() };
        assert_eq!(
            strip.items().remove(0).pointer().map(|m| m.text.clone()).as_deref(),
            Some("[awareness]\ncontinue the migration"),
            "units that are their own pointers stay as they are, never swapped for the minimum"
        );
        let none = Grounding { persona: uuid::Uuid::nil(), faculty: "room-board".into(), minimum: "minimum".into(), text: String::new(), units: vec![other].into() };
        assert_eq!(none.items().remove(0).pointer().map(|m| m.text.clone()).as_deref(), Some("[room-board]\nminimum"));
    }
}
