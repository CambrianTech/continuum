//! A standing grounding block (the workspace map, the board, the wall, the roster) as a
//! source of one item: what it says now. A block that changes appends its new version.
//!
//! Its levels come from the source's own units (FLOW-IMPLEMENTATION-PLAN §2d): whole is
//! every unit whole, summary every unit at its summary, pointer every unit at the cheapest
//! level it offers. The block stays one item because each item renders as its own message;
//! choosing a level per unit (keep the best card whole, collapse the rest) waits on merging
//! adjacent items at render.

use continuum_context::source::{Item, Source};

use crate::cognition::rag_budget::RagItem;

use super::{priority, ContextMessage};

pub(crate) struct Grounding {
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
    pub fn atomic(faculty: impl Into<String>, text: impl Into<String>) -> Self {
        let text = text.into();
        Self { faculty: faculty.into(), minimum: text.clone(), text, units: std::sync::Arc::from(Vec::new()) }
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
            return vec![Item { key: "block".to_owned(), full, summary: None, pointer: Some(pointer), stored: None }];
        }
        let whole = self.at(|unit| &unit.content);
        let summary = self.units.iter().any(|unit| unit.summary.is_some()).then(|| {
            self.at(|unit| unit.summary.as_deref().unwrap_or(&unit.content)) // a unit with no summary level is its own summary
        });
        let pointer = self.at(|unit| unit.pointer.as_deref().or(unit.summary.as_deref()).unwrap_or(&unit.content)); // the cheapest level each unit offers
        let summary = summary.filter(|summary| *summary != whole);
        let pointer = (pointer != whole).then_some(pointer);
        vec![Item {
            key: "block".to_owned(),
            full: self.render(whole),
            summary: summary.map(|summary| self.render(summary)),
            pointer: Some(pointer.map_or_else(|| self.render(self.minimum.clone()), |pointer| self.render(pointer))), // map_or_else: units with no coarser level keep the block's truthful minimum,
            stored: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grounding(units: Vec<RagItem>) -> Grounding {
        let text = units.iter().map(|u| u.content.clone()).collect::<Vec<_>>().join("\n");
        Grounding { faculty: "room-board".into(), minimum: text.clone(), text, units: units.into() }
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
        assert_eq!(text(&item.full), "[room-board]\ncard a: a long description of the work\ncard b: a long description of the work");
        assert_eq!(item.summary.as_ref().map(text).as_deref(), Some("[room-board]\ncard a: short\ncard b: short"));
        assert_eq!(item.pointer.as_ref().map(text).as_deref(), Some("[room-board]\n[card a]\n[card b]"));
    }
}
