//! `BenchRound` — the round LIFECYCLE spine (#371): a dispatched benchmark round is an
//! ENTITY with a stage, not a pile of cards whose fate is probe archaeology.
//!
//! Before this, `benchmark/dispatch` posted cards + kickoffs and then NOTHING owned the
//! round: no stage announcements, no END — "random and directed by agent, not an
//! ecosystem" (Joel 2026-08-16). This module is the smallest true fix: dispatch registers
//! the round (its id IS the run room's id — dispatch already mints one per run, #329a;
//! never a second id), and the EXISTING `work.card.state_changed` bus event settles cards
//! one by one until the round is Done.
//!
//! Event-driven, never polling ([[the-whole-system-is-event-based-not-polling]]): there is
//! no tick here and no new bus receiver either — the one live consumer of
//! `WORK_CARD_STATE_CHANGED` (`modules::benchmark_grade`'s receiver task, the #450
//! subscriber) forwards every payload to [`observe_card_event`]. One subscription, two
//! reactions (grade the card, advance the round).
//!
//! Probes fire at TRANSITIONS ONLY:
//! - `bench.round.staged`   — at dispatch: round id, benchmark, card count.
//! - `bench.round.card_settled` — a card in the round reached a terminal state.
//! - `bench.round.done`     — the END, exactly once: every card settled.
//!
//! ## The round also owns WHO DRIVES its work ([`WorkDriver`])
//!
//! A benchmark card can be worked two ways, and the difference decides whether the round
//! teaches anybody anything. The round is the only thing that knows which, because the
//! decision is made once at dispatch and read later at claim time — in a different verb,
//! on a different task, with nothing threaded between them.

use continuum_base::time::EpochMs;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{LazyLock, Mutex};

use serde_json::Value;
use uuid::Uuid;

mod work_driver;
pub use work_driver::*;
mod review_parent;
pub use review_parent::*;
mod round_card_snapshot;
pub use round_card_snapshot::*;

#[cfg(test)]
mod tests;
