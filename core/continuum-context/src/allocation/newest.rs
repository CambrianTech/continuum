//! Her NEWEST result in the act's one allocation: the step that ends her recursion (plan goal 7:
//! "Keep the newest interaction whole and the full result reachable; shrink oldest-first").
//!
//! It is irreducible at its WHOLE. It is chosen before the search, against every other part at its
//! hard limit (framing's irreducible blocks, the required input, each verb at its pointer, her last
//! move, her view's required floors, her reply's measured need): so whenever the window holds it
//! whole beside those, it is whole, and her tools, grounding, view and older history collapse
//! around it in the search that follows. Before, it was a ladder of summaries chosen like any
//! other part: on the M5 (2026-11-07) her newest result came in as a summary and a handle 159
//! times in a day, about 105 follow-on acts re-read it, and 52 of those re-reads were themselves
//! too large, so she paged a handle that answered with another handle.
//!
//! Only a result larger than what the window can hold beside every other hard limit is shown
//! smaller, and then as its FIRST PAGE: its own content cut to exactly the room left (never a
//! summary, never a bare handle), with a note naming the call that reads on from where the page
//! ends. The page's least ([`Newest::Pageable::least`]) is its producer's measure: the note plus
//! at least as much content as the note costs. Its producer saves a result that has no other copy
//! before it must page (a refusal on every act while it stayed newest: the IntelMac, 2026-11-11), so
//! `Whole` is only a result whose save the store refused.

/// Her newest result as the allocation sees it, in the allocation's unit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Newest {
    /// No result this act.
    #[default]
    Absent,
    /// A result with no other copy: whole, or the turn is short.
    Whole(u32),
    /// A saved result: whole whenever the window holds it beside every other hard limit, else its
    /// first page cut to the room left, never below `least`.
    Pageable { whole: u32, least: u32 },
}

/// What the allocation showed of her newest result.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum NewestShown {
    #[default]
    Absent,
    Whole,
    /// Its first page, `tokens` long: what the window left beside every other hard limit.
    Page { tokens: u32 },
}

impl Newest {
    /// What it costs whole: what it wants.
    pub fn whole(self) -> u32 {
        match self {
            Self::Absent => 0,
            Self::Whole(whole) | Self::Pageable { whole, .. } => whole,
        }
    }

    /// The least it is ever shown at: its whole when it has no page, else its least page.
    pub fn least(self) -> u32 {
        match self {
            Self::Absent => 0,
            Self::Whole(whole) => whole,
            Self::Pageable { whole, least } => least.min(whole),
        }
    }

    pub fn is_absent(self) -> bool {
        self == Self::Absent
    }

    /// The same result with every cost through `charge` (a producer's estimate into the
    /// allocation's unit).
    pub fn charged(self, charge: impl Fn(usize) -> u32) -> Self {
        let at = |tokens: u32| charge(tokens as usize);
        match self {
            Self::Absent => Self::Absent,
            Self::Whole(whole) => Self::Whole(at(whole)),
            Self::Pageable { whole, least } => Self::Pageable { whole: at(whole), least: at(least) },
        }
    }

    /// What it shows within `left` (the window less every other hard limit and her reply's
    /// measured need), and what that costs: whole when it fits; else, when it has a page, a page
    /// of `page_room` (the same less her reply's whole want: a page never takes her answer's
    /// room), never below its least; a result with no page stays whole.
    pub fn shown(self, left: u32, page_room: u32) -> (NewestShown, u32) {
        match self {
            Self::Absent => (NewestShown::Absent, 0),
            Self::Whole(whole) => (NewestShown::Whole, whole),
            Self::Pageable { whole, .. } if whole <= left => (NewestShown::Whole, whole),
            Self::Pageable { .. } => {
                let tokens = page_room.min(left).max(self.least());
                (NewestShown::Page { tokens }, tokens)
            }
        }
    }
}
