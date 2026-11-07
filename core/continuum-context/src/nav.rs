//! A navigation node: a group of a source's items folded under one label, with their count and the
//! typed command that expands them. Her view is collapsed navigation (Joel, 2026-11-07: "think
//! positron collapsed navigation and expansion"; "so much like user navigation and tabs"): her
//! focus whole, what is near it at a summary, and everything else folded into nodes she opens with
//! the same command a person's click maps to. Expanding is an ordinary act whose result enters her
//! view like any result; nothing here renders the folded items.
//!
//! One construct for every source: a source's [`crate::source::Source::fold`] block and a group a
//! source folds itself (a board's merged cards) are both a `Node`, so no source words its own fold
//! (thoughts, results and the board each had or wanted one). A node costs one line whatever its
//! count, never one pointer per folded item (Aiko and Aris on the M5, 2026-11-07: 664 thought
//! pointers were 18-24% of their prompts).

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

/// How many of a group of `count` items off her focus stand on their own, the most recent first;
/// the rest fold into the group's node. The rung between the node alone (one line) and every item
/// (`count` lines): their geometric mean, the ballpark rule `crate::levels` builds every summary
/// with, never a fixed number. 200 open cards show 15 and one node; 4 show 2.
pub fn standing(count: usize) -> usize {
    (count as f64).sqrt().ceil() as usize
}

impl fmt::Display for Node {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "▸ {} ({}) → `{}`", self.label, self.count, self.expand)
    }
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
}
