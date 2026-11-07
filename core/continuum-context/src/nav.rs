//! Collapsed navigation: the ONE contract every view a mind perceives adopts by declaring its shape.
//!
//! Joel, 2026-11-07: "think positron collapsed navigation and expansion"; "so much like user
//! navigation and tabs"; "all positronic controls collapse easily without having to independently
//! manage this at each consumer". A view (a board, a wall, a roster, any activity's control)
//! declares only its SHAPE, one [`Entry`] per subject:
//! - a typed key (what the subject is; a change to it appends that entry's new revision),
//! - its renderings: whole, an optional summary (else derived from its shape by
//!   [`crate::levels::summarize`]), and a pointer (its expand handle),
//! - what it is to her ([`Need`]): her FOCUS, or a member of a [`Group`] with its recency.
//!
//! Everything else comes from [`collapse`], never from the view: her focus stands whole, last
//! (newest, so the view's allocation keeps it whole longest); each group shows its most recently
//! changed [`standing`] entries on their own and folds the rest into ONE [`Node`] (its label, its
//! count, the typed command that opens it, the same command a person's click maps to), or folds
//! all of it when it is history ([`Fold::All`]). A node's count moves as its group does, so it is
//! a tally (ordered after the body). The context view then supersedes on key, ends what left,
//! levels each item by the act's one allocation, and folds an oldest run into a [`Node`]
//! (`Source::fold`). Expanding a node is an ordinary act whose result enters her view like any
//! result. Before this, each source decided its own collapse (Benchy, the M5, 2026-11-07: 7 whole
//! kanban snapshots, 47% of a 439k-char prompt; 664 thought pointers, 18-24% of Aiko's and Aris's).

use std::fmt;

/// The command that expands a node: a verb from her command surface and its arguments, typed, so
/// a node can only name a command by its parts (`work/list state=merged`), never by hand-built text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Expand {
    pub verb: &'static str,
    pub args: Vec<(&'static str, String)>,
}

impl Expand {
    pub fn verb(verb: &'static str) -> Self {
        Self { verb, args: Vec::new() }
    }

    pub fn with(mut self, name: &'static str, value: impl Into<String>) -> Self {
        self.args.push((name, value.into()));
        self
    }
}

impl fmt::Display for Expand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.verb)?;
        for (name, value) in &self.args {
            write!(f, " {name}={value}")?;
        }
        Ok(())
    }
}

/// A folded group: what it holds, how many, and how to open it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    pub label: String,
    pub count: usize,
    pub expand: Expand,
}

impl Node {
    pub fn new(label: impl Into<String>, count: usize, expand: Expand) -> Self {
        Self { label: label.into(), count, expand }
    }
}

impl fmt::Display for Node {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "▸ {} ({}) → `{}`", self.label, self.count, self.expand)
    }
}

/// How many of a group of `count` entries off her focus stand on their own, the most recent first;
/// the rest fold into the group's node. The rung between the node alone (one line) and every entry
/// (`count` lines): their geometric mean, the ballpark rule `crate::levels` builds every summary
/// with, never a fixed number. 200 open cards show 15 and one node; 4 show 2.
pub fn standing(count: usize) -> usize {
    (count as f64).sqrt().ceil() as usize
}

/// How a group folds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fold {
    /// Its most recently changed [`standing`] entries stand, the rest fold: live work off her focus.
    Recent,
    /// It folds whole: history (merged cards), present only as its count.
    All,
}

/// A group of entries off her focus, as a view declares it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    pub label: &'static str,
    pub expand: Expand,
    pub fold: Fold,
}

/// What an entry is to her.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Need {
    /// Her focus: whole, in the order the view declares, after every group.
    Focus,
    /// A member of a group, with when it last changed.
    In { group: Group, recency: u64 },
}

/// One subject of a view, as the view declares it: its shape, nothing about how it collapses.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry<M> {
    pub key: String,
    pub full: String,
    pub summary: Option<String>,
    pub pointer: Option<String>,
    pub need: Need,
    /// A count or time that moves as its subject does: ordered after the body.
    pub tally: bool,
    /// The view's provenance for it (a card's state, a post's author).
    pub meta: M,
}

impl<M> Entry<M> {
    /// Her focus, whole, keyed by its typed id.
    pub fn new(key: impl fmt::Display, full: impl Into<String>, meta: M) -> Self {
        Self { key: key.to_string(), full: full.into(), summary: None, pointer: None, need: Need::Focus, tally: false, meta }
    }

    pub fn summary(mut self, summary: impl Into<String>) -> Self {
        self.summary = Some(summary.into());
        self
    }

    pub fn pointer(mut self, pointer: impl Into<String>) -> Self {
        self.pointer = Some(pointer.into());
        self
    }

    /// A member of `group`, last changed at `recency`.
    pub fn in_group(mut self, group: Group, recency: u64) -> Self {
        self.need = Need::In { group, recency };
        self
    }

    pub fn tally(mut self) -> Self {
        self.tally = true;
        self
    }
}

/// What a collapsed unit stands for.
#[derive(Clone, Debug, PartialEq)]
pub enum Origin<M> {
    /// One entry of the view, with its provenance.
    Entry(M),
    /// A group's node, standing for `count` entries.
    Node { group: &'static str, count: usize },
}

/// One unit of a collapsed view: what the context view keys and levels.
#[derive(Clone, Debug, PartialEq)]
pub struct Unit<M> {
    pub key: String,
    pub full: String,
    pub summary: Option<String>,
    pub pointer: Option<String>,
    pub tally: bool,
    pub origin: Origin<M>,
}

impl<M> Unit<M> {
    fn of(entry: Entry<M>) -> Self {
        // A summary the view did not declare is derived from the entry's shape.
        let summary = entry.summary.or_else(|| crate::levels::summarize(&entry.full, crate::levels::Shape::Output));
        Self { key: entry.key, full: entry.full, summary, pointer: entry.pointer, tally: entry.tally, origin: Origin::Entry(entry.meta) }
    }

    fn node(group: &Group, count: usize) -> Self {
        let text = Node::new(group.label, count, group.expand.clone()).to_string();
        Self { key: format!("nav:{}", group.label), full: text.clone(), summary: None, pointer: Some(text), tally: true, origin: Origin::Node { group: group.label, count } }
    }
}

/// A view's entries as collapsed navigation: each group (in the order it first appears) as its
/// node and its standing entries, oldest change first; then her focus, in declared order.
pub fn collapse<M>(entries: Vec<Entry<M>>) -> Vec<Unit<M>> {
    let mut groups: Vec<(Group, Vec<(u64, Entry<M>)>)> = Vec::new();
    let mut focus = Vec::new();
    for entry in entries {
        match entry.need.clone() {
            Need::Focus => focus.push(entry),
            Need::In { group, recency } => match groups.iter_mut().find(|(known, _)| known.label == group.label) {
                Some((_, members)) => members.push((recency, entry)),
                None => groups.push((group, vec![(recency, entry)])),
            },
        }
    }
    let mut units = Vec::new();
    for (group, mut members) in groups {
        members.sort_by_key(|(recency, _)| *recency);
        let folded = match group.fold {
            Fold::Recent => members.len() - standing(members.len()),
            Fold::All => members.len(),
        };
        if folded > 0 {
            units.push(Unit::node(&group, folded));
        }
        units.extend(members.into_iter().skip(folded).map(|(_, entry)| Unit::of(entry)));
    }
    units.extend(focus.into_iter().map(Unit::of));
    units
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a fold worded by hand per source, or one that lists what it folds. A node
    // renders its label, its count and its typed command on one line, whatever the count.
    #[test]
    fn a_node_is_one_line_naming_its_count_and_the_command_that_opens_it() {
        let node = Node::new("merged", 840, Expand::verb("work/list").with("state", "merged"));
        assert_eq!(node.to_string(), "▸ merged (840) → `work/list state=merged`");
        assert_eq!(Node::new("merged", 8, node.expand.clone()).to_string().lines().count(), 1);
    }

    // what this catches: a group off her focus shown whole (200 cards) or cut to a fixed count. The
    // number standing on their own grows with the group, far slower than it, and never reaches zero.
    #[test]
    fn a_groups_standing_items_are_the_rung_between_its_node_and_every_item() {
        assert_eq!([0, 1, 4, 5, 200].map(standing), [0, 1, 2, 3, 15]);
    }

    // what this catches: a view deciding its own collapse. From a declared shape alone: her focus
    // last in its declared order, a live group's most recent entries standing (oldest change first)
    // after one node counting the rest, a history group only its node, every node a tally.
    #[test]
    fn collapse_turns_a_declared_shape_into_focus_standing_entries_and_nodes() {
        let live = Group { label: "open", expand: Expand::verb("work/list").with("state", "open"), fold: Fold::Recent };
        let history = Group { label: "merged", expand: Expand::verb("work/list").with("state", "merged"), fold: Fold::All };
        let mut entries: Vec<Entry<()>> = (0..9u64).map(|n| Entry::new(format!("open:{n}"), format!("open card {n}"), ()).in_group(live.clone(), 100 - n)).collect();
        entries.push(Entry::new("mine:a", "my card a", ()));
        entries.extend((0..5u64).map(|n| Entry::new(format!("merged:{n}"), "a merged card", ()).in_group(history.clone(), n)));
        entries.push(Entry::new("mine:b", "my card b", ()));
        let units = collapse(entries);
        let keys: Vec<&str> = units.iter().map(|u| u.key.as_str()).collect();
        assert_eq!(keys, ["nav:open", "open:2", "open:1", "open:0", "nav:merged", "mine:a", "mine:b"]);
        assert_eq!(units[0].full, "▸ open (6) → `work/list state=open`");
        assert_eq!(units[0].origin, Origin::Node { group: "open", count: 6 });
        assert!(units.iter().filter(|u| u.tally).all(|u| u.key.starts_with("nav:")));
        assert_eq!(units[4].full, "▸ merged (5) → `work/list state=merged`");
    }
}
