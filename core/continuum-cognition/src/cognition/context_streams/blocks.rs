//! A grounding block (the workspace map, the board, the wall, the roster) as a source of her view.
//!
//! A block whose source delivered keyed units (`RagItem`s: one card, one wall post, one peer) is
//! STATE, one keyed item per unit: a unit that changes appends its own new revision and nothing
//! else, and a unit the source stops offering is ended once ([`Source::ended`]). Before this the
//! block was ONE item, so any change re-appended the whole block: Benchy's prompt on the M5
//! (2026-11-07, build 19426f454) held 7 whole `[room-kanban]` snapshots of ~30k chars each, 47% of
//! a 439k-char prompt, because each claim anywhere on the board appended the board again.
//!
//! It is collapsed navigation: each unit carries its own levels from its source (whole, summary,
//! pointer), the view's one allocation spends newest first, so the units a source orders last (her
//! focus) keep detail longest, and the oldest run folds into ONE navigation node
//! ([`continuum_context::nav::Node`]) naming its count and the command that opens it, never a
//! pointer line per unit. A unit marked as a tally (`RagItem::tally`, a count that moves as its
//! subject does) is ordered after the body, so a moving count never sits in front of the units.
//!
//! A block with no units is one subject shown whole, or at its truthful minimum (a contract in
//! force, a checkout she works from).

use continuum_context::nav::{Expand, Node};
use continuum_context::source::{Item, Source};

use crate::cognition::rag_budget::RagItem;

use super::{priority, ContextMessage};

#[derive(Clone)]
pub(crate) struct Grounding {
    /// Whose block this is (its id, `super::standing`).
    pub persona: uuid::Uuid,
    pub faculty: String,
    pub body: Body,
}

/// What a block is made of, which decides how it renders as items.
#[derive(Clone)]
pub(crate) enum Body {
    /// One subject: the whole text, and what it guarantees when the window is short (a long list's
    /// first units with a line naming what is omitted; an atomic block whole).
    Atomic { text: String, minimum: String },
    /// The source's keyed, levelled units, one item each.
    Units {
        units: std::sync::Arc<[RagItem]>,
        /// The verb that yields the source's whole (`work/list`): what its navigation nodes open.
        /// `None` when no verb does: its units then never fold, they leave when collapsed.
        expand: Option<&'static str>,
        /// The source's own notice that it holds more than it delivered, as its last item.
        more: Option<String>,
    },
}

impl Grounding {
    /// A block with no units that is shown whole or not at all (a contract in force).
    pub fn atomic(persona: uuid::Uuid, faculty: impl Into<String>, text: impl Into<String>) -> Self {
        let text = text.into();
        Self { persona, faculty: faculty.into(), body: Body::Atomic { minimum: text.clone(), text } }
    }

    fn render(&self, body: &str) -> ContextMessage {
        ContextMessage::user(format!("[{}]\n{body}", self.faculty))
    }

    /// `count` of this block's units folded into one node that opens them.
    fn node(&self, label: &str, count: usize) -> Option<ContextMessage> {
        let Body::Units { expand: Some(verb), .. } = &self.body else { return None };
        Some(self.render(&Node::new(label, count, Expand::verb(verb)).to_string()))
    }

    /// One unit as a keyed item of this block, at its source's levels.
    fn unit(&self, key: String, unit: &RagItem, pointer: Option<ContextMessage>) -> Item<ContextMessage> {
        let full = self.render(&unit.content);
        let summary = unit
            .summary
            .as_deref()
            .filter(|summary| continuum_context::levels::is_a_level(summary, &unit.content))
            .map(|summary| self.render(summary));
        super::standing(self.persona, &self.faculty, &key, full).with_summary(summary).with_pointer(pointer)
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
        let (units, more) = match &self.body {
            Body::Atomic { text, minimum } => {
                // A block is what she works from now (her board, her checkout) with no handle to
                // reopen it by, so collapsed it is still its truthful minimum, never a one-liner.
                return vec![super::standing(self.persona, &self.faculty, "block", self.render(text)).with_pointer(self.render(minimum))];
            }
            Body::Units { units, more, .. } => (units, more),
        };
        // The body first, then its tallies: a count that moves never sits in front of the units.
        let ordered: Vec<(usize, &RagItem)> =
            units.iter().enumerate().filter(|(_, unit)| !unit.tally()).chain(units.iter().enumerate().filter(|(_, unit)| unit.tally())).collect();
        let keeps_a_pointer = units.iter().any(|unit| unit.pointer.is_some());
        let newest = ordered.len().saturating_sub(1);
        let mut items: Vec<Item<ContextMessage>> = ordered
            .into_iter()
            .enumerate()
            .map(|(at, (index, unit))| {
                // A unit keyed before keys existed (an old capture) is keyed by its place.
                let key = if unit.key.is_empty() { format!("unit:{index}") } else { unit.key.clone() };
                // A block none of whose units keeps a pointer still says, collapsed, what it holds
                // and how to open it: its newest unit collapses to the block's node.
                let pointer = match (&unit.pointer, keeps_a_pointer) {
                    (Some(pointer), _) => Some(self.render(pointer)),
                    (None, false) if at == newest => self.node(&format!("{} items", self.faculty), units.len()),
                    (None, _) => None,
                };
                self.unit(key, unit, pointer)
            })
            .collect();
        if let Some(more) = more {
            items.push(super::standing(self.persona, &self.faculty, "(more)", self.render(more)));
        }
        items
    }

    /// A block of units is state: a unit the source no longer offers (a card that left the open
    /// board, an unpinned post) is ended once, never left standing as if it still held.
    fn states(&self) -> bool {
        matches!(self.body, Body::Units { .. })
    }

    fn ended(&self, key: &str) -> Option<ContextMessage> {
        Some(self.render(&format!("{key}: no longer shown")))
    }

    /// The oldest run as ONE navigation node: its count and the command that opens it. The view
    /// folds the run only when the node is smaller than the run's pointers.
    fn fold(&self, folded: &[Item<ContextMessage>]) -> Option<ContextMessage> {
        self.node("older", folded.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use continuum_context::epoch::Block;
    use continuum_context::view::{Claim, GrantMismatch, View};

    fn grounding(units: Vec<RagItem>, expand: Option<&'static str>) -> Grounding {
        Grounding { persona: uuid::Uuid::nil(), faculty: "room-kanban".into(), body: Body::Units { units: units.into(), expand, more: None } }
    }

    fn whole(claims: &[Claim]) -> Vec<usize> {
        claims.iter().map(|claim| claim.full_chars).collect()
    }

    fn texts(view: &View<ContextMessage>) -> Vec<String> {
        view.render().map(|m| m.text().to_owned()).collect()
    }

    // what this catches: a block rendered as ONE item, so any change to one unit re-appended every
    // unit (Benchy, the M5, 2026-11-07: 7 whole kanban snapshots, 47% of her prompt). Each unit is
    // its own keyed item at its own levels, and a unit with no pointer leaves when collapsed.
    #[test]
    fn each_unit_is_its_own_keyed_item_at_its_own_levels() {
        let card = |id: &str| {
            RagItem::whole(format!("card:{id}"), format!("card {id}: a long description of the work"), serde_json::Value::Null)
                .with_summary(format!("card {id}: short"))
        };
        let items = grounding(vec![card("a").with_pointer("[card a]"), card("b")], None).items();
        assert_eq!(items.iter().map(|i| i.key().to_owned()).collect::<Vec<_>>(), ["card:a", "card:b"]);
        assert_eq!(items[0].full().text(), "[room-kanban]\ncard a: a long description of the work");
        assert_eq!(items[0].summary().map(|m| m.text().to_owned()).as_deref(), Some("[room-kanban]\ncard a: short"));
        assert_eq!(items[0].pointer().map(|m| m.text().to_owned()).as_deref(), Some("[room-kanban]\n[card a]"));
        assert!(items[1].pointer().is_none(), "a unit with no pointer leaves a collapsed block");
    }

    // what this catches: a collapsed block that says nothing at all. When no unit keeps a pointer the
    // newest unit collapses to the block's navigation node, which names the verb that opens it; a
    // tally is ordered after the body so its moving count never leads.
    #[test]
    fn a_block_whose_units_keep_no_pointer_collapses_to_its_node_and_tallies_follow_the_body() {
        let count = RagItem::whole("headline", "[board] 3 claimable here.", serde_json::Value::Null).as_tally();
        let card = RagItem::whole("card:a", "card a [Open]", serde_json::Value::Null);
        let items = grounding(vec![count, card], Some("work/list")).items();
        assert_eq!(items.iter().map(|i| i.key().to_owned()).collect::<Vec<_>>(), ["card:a", "headline"]);
        assert!(items[0].pointer().is_none());
        assert_eq!(items[1].pointer().map(|p| p.text().to_owned()).as_deref(), Some("[room-kanban]\n▸ room-kanban items (2) → `work/list`"));
    }

    // what this catches: the whole board re-appended on every claim. Through the real view: a change
    // to one card appends that card's revision alone, an unchanged board appends nothing, and a card
    // that leaves is ended once instead of standing as if it were still open.
    #[test]
    fn a_change_to_one_unit_appends_only_its_revision_and_a_unit_that_leaves_is_ended_once() -> Result<(), GrantMismatch> {
        let board = |states: &[(&str, &str)]| {
            grounding(
                states.iter().map(|(id, state)| RagItem::whole(format!("card:{id}"), format!("card {id} [{state}] \"a card with a title\" (P2, unclaimed)"), serde_json::Value::Null)).collect(),
                None,
            )
        };
        let first = board(&[("a", "Open"), ("b", "Open"), ("c", "Open")]);
        let (mut view, _) = View::start(Vec::new(), &[&first as &dyn Source<_>], whole)?;
        assert_eq!(view.advance(&[&first as &dyn Source<_>]).items, 0, "unchanged: nothing appended");
        let claimed = board(&[("a", "Open"), ("b", "Claimed"), ("c", "Open")]);
        assert_eq!(view.advance(&[&claimed as &dyn Source<_>]).items, 1, "one card changed, one revision");
        assert!(texts(&view).last().is_some_and(|t| t.contains("card b [Claimed]")));
        let left = board(&[("a", "Open"), ("b", "Claimed")]);
        assert_eq!(view.advance(&[&left as &dyn Source<_>]).items, 1, "the card that left is ended");
        assert!(texts(&view).last().is_some_and(|t| t.contains("card:c: no longer shown")));
        assert_eq!(view.advance(&[&left as &dyn Source<_>]).items, 0, "and ended once");
        Ok(())
    }

    // what this catches: a collapsed board that is a pointer line per card (664 thought pointers were
    // 18-24% of Aiko's and Aris's prompts). Under a short grant the newest units keep detail and the
    // oldest run folds into ONE node naming its count and the command that opens it.
    #[test]
    fn under_a_short_grant_the_oldest_run_folds_into_one_navigation_node() -> Result<(), GrantMismatch> {
        let cards: Vec<RagItem> = (0..40)
            .map(|n| {
                RagItem::whole(format!("card:{n}"), format!("card {n:04} [Open] \"a card with a reasonably long title\" (P2, unclaimed)"), serde_json::Value::Null)
                    .with_pointer(format!("[card {n:04}] [Open]"))
            })
            .collect();
        let board = grounding(cards, Some("work/list"));
        let (view, _) = View::start(Vec::new(), &[&board as &dyn Source<_>], |claims: &[Claim]| vec![claims[0].full_chars / 8])?;
        let shown = texts(&view);
        assert_eq!(shown.iter().filter(|t| t.contains("▸ older (")).count(), 1, "one node: {shown:#?}");
        assert!(shown.first().is_some_and(|t| t.contains("→ `work/list`")), "the node leads, the oldest run: {shown:#?}");
        assert!(shown.last().is_some_and(|t| t.contains("card 0039 [Open] \"a card")), "the newest stays whole");
        assert!(view.chars() < board.items().iter().map(|i| i.full().chars()).sum::<usize>() / 4);
        Ok(())
    }

    /// A resident's real board, as her captured prompt carried it (`fixtures/`): every whole
    /// `[room-kanban]` snapshot one request held, its build and the request's size.
    #[derive(serde::Deserialize)]
    struct Captured {
        persona: String,
        build_sha: String,
        captured_at_ms: u64,
        prompt_chars: usize,
        board_chars: usize,
        snapshots: Vec<String>,
    }

    /// A captured snapshot declared as `RoomBoardSource` declares its shape, collapsed by the one
    /// contract (`nav::collapse`): a card is an entry from its `card <id>` line (a title's
    /// continuation and its submission guidance ride with it), a lead an entry from its `[...]`
    /// header; her cards and her lead are her focus, every other card is in its group (claimable,
    /// held by others). The capture's order, oldest card first, stands in for each card's last
    /// change, which the capture does not carry.
    fn units(snapshot: &str) -> Vec<RagItem> {
        use continuum_context::nav::{collapse, Entry, Expand, Fold, Group};
        let mut raw: Vec<String> = Vec::new();
        for line in snapshot.lines() {
            match raw.last_mut() {
                Some(unit) if !(line.starts_with("card ") || line.starts_with('[')) => {
                    unit.push('\n');
                    unit.push_str(line);
                }
                _ => raw.push(line.to_owned()),
            }
        }
        let group = |label: &'static str| Group { label, expand: Expand::verb("work/list"), fold: Fold::Recent };
        let (mut entries, mut hers): (Vec<Entry<bool>>, Vec<Entry<bool>>) = Default::default();
        for (at, unit) in raw.into_iter().enumerate() {
            let first = unit.lines().next().unwrap_or_default().to_owned(); // unwrap_or_default: a unit has at least its own line
            if let Some(rest) = first.strip_prefix("card ") {
                let id = rest.split(' ').next().unwrap_or_default(); // unwrap_or_default: the line names its card
                let tag = rest.split_once('[').and_then(|(_, t)| t.split_once(']')).map(|(t, _)| t).unwrap_or_default(); // unwrap_or_default: an untagged card keeps its id
                let entry = Entry::new(format!("card:{id}"), unit.clone(), false).summary(first.clone());
                if unit.contains(", owner YOU)") {
                    hers.push(entry.pointer(format!("[card {id}] [{tag}] — work/get {id}")));
                    continue;
                }
                let label = if tag == "Open" || tag.starts_with("CLAIMABLE") { "claimable" } else { "held by others" };
                entries.push(entry.pointer(format!("[card {id}] [{tag}]")).in_group(group(label), at as u64));
            } else {
                let header = first.split_once(']').map(|(h, _)| format!("{h}]")).unwrap_or_default(); // unwrap_or_default: every lead opens with its header
                let entry = Entry::new(header.clone(), unit.clone(), false).pointer(format!("{header} — work/list"));
                if header == "[your work]" { hers.insert(0, entry) } else { entries.push(entry.tally()) }
            }
        }
        entries.extend(hers);
        collapse(entries)
            .into_iter()
            .map(|unit| {
                let mut item = RagItem::whole(unit.key, unit.full, serde_json::Value::Null);
                item.summary = unit.summary;
                item.pointer = unit.pointer;
                if unit.tally { item.as_tally() } else { item }
            })
            .collect()
    }

    // what this catches: the whole board appended on every change, every card rendered whole (Benchy,
    // the M5, 2026-11-07, 19426f454: 7 whole `[room-kanban]` snapshots, 47% of a 439k-char prompt;
    // Atlas: 2 snapshots, 58% of 100k). Replayed through the real view from their captured boards:
    // the board is ONE set of keyed items (her focus whole, a few cards at a glance, navigation nodes
    // that count the rest), each change appends only what changed, and every card stays reachable
    // through a node's command.
    #[test]
    fn real_boards_are_collapsed_navigation_and_a_change_appends_only_its_cards() -> Result<(), GrantMismatch> {
        for raw in [include_str!("fixtures/benchy-board-2026-11-07.json"), include_str!("fixtures/atlas-board-2026-11-07.json")] {
            let captured: Captured = serde_json::from_str(raw).expect("the fixture is a capture"); // expect: checked-in fixture
            let board = |n: usize| grounding(units(&captured.snapshots[n]), Some("work/list"));
            let first = board(0);
            let (mut view, _) = View::start(Vec::new(), &[&first as &dyn Source<_>], whole)?;
            let mut previous = first.items();
            let mut appended = 0;
            for n in 1..captured.snapshots.len() {
                let next = board(n);
                let items = next.items();
                let changed = items.iter().filter(|i| !previous.iter().any(|p| p.key() == i.key() && p.full() == i.full())).count();
                let ended = previous.iter().filter(|p| !items.iter().any(|i| i.key() == p.key())).count();
                let advance = view.advance(&[&next as &dyn Source<_>]);
                assert_eq!(advance.items, changed + ended, "{}: snapshot {n} appends only what changed", captured.persona);
                assert!(advance.chars < str::chars(&captured.snapshots[n]).count() / 10, "{}: never the board again ({} chars)", captured.persona, advance.chars);
                appended += advance.chars;
                previous = items;
            }
            let shown = texts(&view);
            let holds = captured.snapshots.iter().any(|s| s.contains(", owner YOU)"));
            assert_eq!(shown.iter().any(|t| t.contains(", owner YOU)") && t.contains("(P")), holds, "{}: her held card whole", captured.persona);
            // Every live card of the last board is on its own or counted by a node.
            let last = captured.snapshots.last().map_or(0, |s| s.lines().filter(|l| l.starts_with("card ") && !l.contains("owner YOU")).count()); // map_or: a capture holds a board
            let standing = shown.iter().filter(|t| t.starts_with("[room-kanban]\ncard ")).count();
            let counted: usize = units(captured.snapshots.last().map_or("", String::as_str)) // map_or: a capture holds a board
                .iter()
                .filter(|u| u.key.starts_with("nav:"))
                .filter_map(|u| u.content.rsplit_once(" (").and_then(|(_, n)| n.split(')').next()?.parse::<usize>().ok()))
                .sum();
            assert!(standing + counted >= last, "{}: {standing} shown + {counted} counted covers {last} cards", captured.persona);
            let before = captured.board_chars as f64 / captured.prompt_chars as f64;
            let after = view.chars() as f64 / (captured.prompt_chars - captured.board_chars + view.chars()) as f64;
            eprintln!(
                "{} ({}, {}): board {:.1}% of {} chars as {} whole snapshots; after {:.1}% ({} chars: {} items shown, {} cards counted in nodes, {} chars appended by the {} changes)",
                captured.persona,
                captured.build_sha,
                captured.captured_at_ms,
                before * 100.0,
                captured.prompt_chars,
                captured.snapshots.len(),
                after * 100.0,
                view.chars(),
                shown.len(),
                counted,
                appended,
                captured.snapshots.len() - 1
            );
            assert!(after < before / 5.0, "{}: {after:.3} vs {before:.3}", captured.persona);
        }
        Ok(())
    }
}
