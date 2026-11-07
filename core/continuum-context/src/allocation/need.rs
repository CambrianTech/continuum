//! Her need: the window her act's allocation CHOSE, never a sum of floors and never her view's
//! uncollapsed overflow (Joel, 2026-11-07: "the mind needs are also hyperinflated unless you've
//! already implemented the positronic rag budgeting and focus reductions").
//!
//! Before this, a held turn recorded every claim's floor (the sizes each part requested), and a
//! turn whose view overflowed its grant recorded the window plus everything her view lacked,
//! uncollapsed: whole files and folded navigation she reaches by a pointer. Aiko recorded a need of
//! 902,158 tokens on a 262,144-token model (the M5, `grid.mind.unservable` 16:21Z), and the grid's
//! demand read 23.6M. What her view lacks past the chosen window is collapsed (levels, navigation
//! pointers): it is never need.
//!
//! The need is the priced optimum ([`super::priced`]): where her context's value per second
//! peaks, bounded below by every hard limit with her reply's measured need ([`super::priced`]'s
//! `least`) and above by the model's trained context ([`TrainedContext`]). When that optimum lies
//! past the window she was served, her grant censored it, and the optimum is recorded as a lower
//! bound ([`ActNeed::Censored`]). A need above the trained context cannot be built
//! ([`NeedWindow::within`]): it is a measurement defect ([`BeyondTrained`], card bf3b6d68).

use std::num::NonZeroU32;

/// The model's trained context, as the model's own row reports it (its artifact's header, or the
/// catalog's deliberate correction of it). Unknown is a state, never a number: an unreported model
/// bounds her need by the window she was served.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrainedContext {
    Reported(NonZeroU32),
    Unreported,
}

/// A window no larger than the model's trained context: the only form a need takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NeedWindow(u32);

/// A window past the model's trained context: no lane of this model can ever hold it, so it is a
/// measurement defect to name, never a need to record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BeyondTrained {
    pub needed: u32,
    pub trained: NonZeroU32,
}

impl NeedWindow {
    /// `tokens` as a need on a model whose trained context is `trained`.
    pub fn within(tokens: u32, trained: TrainedContext) -> Result<Self, BeyondTrained> {
        match trained {
            TrainedContext::Reported(limit) if tokens > limit.get() => Err(BeyondTrained { needed: tokens, trained: limit }),
            _ => Ok(Self(tokens)),
        }
    }

    pub fn tokens(self) -> u32 {
        self.0
    }
}

/// What one act says of her need.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ActNeed {
    /// The priced optimum, inside her grant: the window her act held.
    Chosen(NeedWindow),
    /// Her grant censored it: the priced optimum lies past the window she was served, or what she
    /// must hold already fills that window. Her need is at least this point.
    Censored(NeedWindow),
    /// No priced optimum: her act time is unmeasured, or no window lies inside the contexts it was
    /// measured over. Her need is unknown above her irreducible window (recorded apart).
    #[default]
    Unpriced,
    /// What she must hold, or the optimum, lies past the model's trained context.
    BeyondTrained(BeyondTrained),
}

impl ActNeed {
    /// `tokens` as an exact need (`censored` false) or a lower bound, on a model whose trained
    /// context is `trained`.
    pub(super) fn at(tokens: u32, censored: bool, trained: TrainedContext) -> Self {
        match NeedWindow::within(tokens, trained) {
            Ok(need) if censored => Self::Censored(need),
            Ok(need) => Self::Chosen(need),
            Err(beyond) => Self::BeyondTrained(beyond),
        }
    }
}
