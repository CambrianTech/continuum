//! The work board's navigation shape, declared ONCE (`continuum_context::nav`): which group a card
//! belongs to off her focus, and the `work/list` that opens exactly that group. Her board view
//! (`room_board_source`) and the `work/list` tool both declare their entries through it, so what
//! she perceives in her view and what her tool returns collapse the same way: her own cards whole
//! (her focus), each live group's most recently changed cards standing and the rest one node with
//! its count, history only its node. Before, `work/list` returned every card (241,460 chars, about
//! 80,884 tokens, on the M5, 2026-11-07) while the same board in her view was a few lines (card
//! e0b2b5b1).
//!
//! A node opens with `work/list group=<its group>`, and that selector filters through
//! [`group_of`] itself, so opening a node returns exactly the cards it folded (Cormac on #5843: a
//! `claimable=false` call behind the "held by others" node also returned her cards and every
//! merged and closed card).

use continuum_context::nav::{Expand, Fold, Group};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

/// A group of the board off her focus: what a node folds, and what `work/list group=` opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BoardGroup {
    Claimable,
    HeldByOthers,
    Merged,
    Closed,
}

impl BoardGroup {
    pub const ALL: [BoardGroup; 4] = [BoardGroup::Claimable, BoardGroup::HeldByOthers, BoardGroup::Merged, BoardGroup::Closed];

    /// Its wire name: what `work/list group=` takes.
    pub fn name(self) -> &'static str {
        match self {
            BoardGroup::Claimable => "claimable",
            BoardGroup::HeldByOthers => "held_by_others",
            BoardGroup::Merged => "merged",
            BoardGroup::Closed => "closed",
        }
    }

    /// What a node of it reads as.
    fn label(self) -> &'static str {
        match self {
            BoardGroup::HeldByOthers => "held by others",
            other => other.name(),
        }
    }

    /// Its node's shape: history folds whole, live work keeps its most recent cards standing.
    pub fn nav(self) -> Group {
        let fold = match self {
            BoardGroup::Merged | BoardGroup::Closed => Fold::All,
            BoardGroup::Claimable | BoardGroup::HeldByOthers => Fold::Recent,
        };
        Group { label: self.label(), expand: Expand::verb("work/list").with("group", self.name()), fold }
    }

    /// The group whose node reads `label`.
    pub fn of_label(label: &str) -> Option<BoardGroup> {
        BoardGroup::ALL.into_iter().find(|group| group.label() == label)
    }
}

/// The group `card` sits in off her focus, or `None` when it is hers and live (her focus).
pub(crate) fn group_of(card: &airc_work::WorkCard, me: Uuid, now_ms: u64) -> Option<BoardGroup> {
    match card.state {
        airc_work::CardState::Merged => return Some(BoardGroup::Merged),
        airc_work::CardState::Closed => return Some(BoardGroup::Closed),
        _ => {}
    }
    if card.owner.map(|owner| owner.as_uuid()) == Some(me) {
        return None;
    }
    Some(if crate::cognition::card_holder::claimable_now(card, now_ms) { BoardGroup::Claimable } else { BoardGroup::HeldByOthers })
}
