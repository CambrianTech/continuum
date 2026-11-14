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

use crate::cognition::review_verdict::MindOf;
use continuum_context::nav::{Expand, Fold, Group};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

/// A group of the board off her focus: what a node folds, and what `work/list group=` opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BoardGroup {
    // A teammate's newest submission awaiting a reviewer, its review card hers to take (a plain
    // comment: a variant doc becomes a described schema entry in every act's work/list tool).
    ReviewRequests,
    Claimable,
    HeldByOthers,
    // The review cards that judge her own submissions: another member takes them.
    YoursInReview,
    Merged,
    Closed,
}

impl BoardGroup {
    pub const ALL: [BoardGroup; 6] = [
        BoardGroup::ReviewRequests,
        BoardGroup::Claimable,
        BoardGroup::HeldByOthers,
        BoardGroup::YoursInReview,
        BoardGroup::Merged,
        BoardGroup::Closed,
    ];

    /// Its wire name: what `work/list group=` takes.
    pub fn name(self) -> &'static str {
        match self {
            BoardGroup::ReviewRequests => "review_requests",
            BoardGroup::Claimable => "claimable",
            BoardGroup::HeldByOthers => "held_by_others",
            BoardGroup::YoursInReview => "yours_in_review",
            BoardGroup::Merged => "merged",
            BoardGroup::Closed => "closed",
        }
    }

    /// What a node of it reads as.
    fn label(self) -> &'static str {
        match self {
            BoardGroup::ReviewRequests => "review requests",
            BoardGroup::HeldByOthers => "held by others",
            BoardGroup::YoursInReview => "your submissions in review",
            other => other.name(),
        }
    }

    /// Its node's shape: history folds whole, live work keeps its most recent cards standing.
    pub fn nav(self) -> Group {
        // Her own work in review is a count she waits on, never a move of hers: it folds whole.
        let fold = match self {
            BoardGroup::Merged | BoardGroup::Closed | BoardGroup::YoursInReview => Fold::All,
            BoardGroup::ReviewRequests | BoardGroup::Claimable | BoardGroup::HeldByOthers => Fold::Recent,
        };
        Group { label: self.label(), expand: Expand::verb("work/list").with("group", self.name()), fold }
    }

    /// The group whose node reads `label`.
    pub fn of_label(label: &str) -> Option<BoardGroup> {
        BoardGroup::ALL.into_iter().find(|group| group.label() == label)
    }
}

/// Who reads the board, and what she reads it against: the facts [`group_of`] needs beyond one card.
pub(crate) struct Reader<'b, M: MindOf> {
    /// The board's cards: a review card's parent and its submissions are among them.
    pub cards: &'b [airc_work::WorkCard],
    /// The reader's board id.
    pub me: Uuid,
    /// The identity owner: whether a submission's publisher is the reader's own mind.
    pub minds: &'b M,
    pub now_ms: u64,
}

/// The group `card` sits in off her focus, or `None` when it is hers and live (her focus).
///
/// A review card is a teammate's request for a verdict, so it leads her board as one
/// (`ReviewRequests`), never one undistinguished card among the claimable: on the M5, 2026-11-14,
/// 46 submissions awaited a reviewer while each review card stood among 667 claimable cards, the
/// group standing only its 26 most recently changed, and no resident filed a review. The review
/// of her own submission is not hers to take (`work/claim` refuses it, card 0174211f), so it is
/// never offered as claimable: it is counted as hers in review.
pub(crate) fn group_of<M: MindOf>(card: &airc_work::WorkCard, reader: &Reader<'_, M>) -> Option<BoardGroup> {
    match card.state {
        airc_work::CardState::Merged => return Some(BoardGroup::Merged),
        airc_work::CardState::Closed => return Some(BoardGroup::Closed),
        _ => {}
    }
    if card.owner.map(|owner| owner.as_uuid()) == Some(reader.me) {
        return None;
    }
    use crate::cognition::review_verdict::{judged_submission, review_request, same_mind};
    // Hers by the same rule `work/claim` refuses by: any review card whose parent's newest is hers.
    let judges_hers = judged_submission(card, reader.cards)
        .is_some_and(|judged| same_mind(reader.minds, judged.publisher, airc_core::PeerId::from_uuid(reader.me)));
    if judges_hers {
        return Some(BoardGroup::YoursInReview);
    }
    let claimable = crate::cognition::card_holder::claimable_now(card, reader.now_ms);
    Some(match (claimable, review_request(card, reader.cards).is_some()) {
        (true, true) => BoardGroup::ReviewRequests,
        (true, false) => BoardGroup::Claimable,
        (false, _) => BoardGroup::HeldByOthers,
    })
}
