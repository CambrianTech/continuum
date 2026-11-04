//! Her held work this tick, typed: the cards she holds with the moves each one's state gives her,
//! the claims that ended since her last good read, and her live rejected-claim facts. Read once
//! per tick by the cycle (`extensions::held_work`) and carried on the [`Workspace`](super::Workspace),
//! so her tool surface and her held-work item read the same snapshot
//! (docs/architecture/STATE-DRIVEN-SURFACE.md). Nothing here parses rendered text.

use airc_work::{CardState, WorkCardId};

use crate::cognition::card_moves::{Held, Move};

/// What her board says she holds now.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HeldWork {
    /// Every card she holds, in the board's order.
    pub holds: Vec<Held>,
    /// The claim her work turns are bound to when she holds more than one
    /// (`work_focus::focus_workspace_card`); `None` with one card or none.
    pub selected: Option<WorkCardId>,
    /// Claims that were hers at the last successful read and are not now: a lease that lapsed
    /// or a card released. Delivered once, the read it is seen; a degraded read never adds one.
    pub ended: Vec<EndedClaim>,
    /// Her rejected claim attempts still worth knowing, worded by the ring that records them
    /// (`persona::claim_rejections`).
    pub rejections: Vec<String>,
}

/// A claim that ended: which card, and what it was called, so she can tell which one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EndedClaim {
    pub card: WorkCardId,
    pub title: String,
}

impl HeldWork {
    /// The command names (and subtrees) of every move her held cards put in her hands, each once,
    /// in canonical order: what her tool surface adds to the room's declared set. Stable while her
    /// cards' states are.
    pub fn move_commands(&self) -> Vec<String> {
        let moves: std::collections::BTreeSet<Move> = self.holds.iter().flat_map(|held| held.moves.iter()).collect();
        moves.into_iter().map(|mv| mv.command().to_owned()).collect()
    }

    /// She holds a card she is working on now (InProgress): the working-presence contract's key.
    pub fn holds_in_progress(&self) -> bool {
        self.holds.iter().any(|held| held.card.state == CardState::InProgress)
    }

    /// She holds a card in either working state (Claimed or InProgress): the lane gate's key.
    pub fn holds_a_working_card(&self) -> bool {
        self.holds.iter().any(|held| matches!(held.card.state, CardState::Claimed | CardState::InProgress))
    }
}
