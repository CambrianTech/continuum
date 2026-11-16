//! What the design says must happen where the code has no type for it yet. Test-side only: each
//! fixing slice replaces one of these with its production type and the tests follow by rename.

/// ONE-ACT §5b.1's one typed restore outcome at the engine/core seam. Codex's slice implements it
/// in the engine adapter (`turn_admission::PageOutcome` today has only Completed / Rejected /
/// Uncertain); when it lands, this enum is deleted and the reproductions import his, variant for
/// variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreOutcome {
    /// A verified successful receipt for this page and destination incarnation.
    Restored,
    /// The page is good; the destination cannot hold it now. Confirmation is kept.
    DeferredCapacity(CapacityShortfall),
    /// The page is good for some destination, not this one. Artifact and provenance are kept.
    Incompatible,
    /// Positive evidence the artifact is missing, corrupt or incomplete: only then is its
    /// confirmation removed.
    InvalidPage,
    /// Transport failure, unknown reason or timeout: provenance and custody stay until a terminal
    /// receipt or a fence proves the outcome.
    Unresolved,
}

/// Which capacity the destination lacked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapacityShortfall {
    /// Cells allocatable right now: retry after verified reclamation, no geometry change.
    FreeCells { requested: u64 },
    /// The page exceeds the whole compatible pool: needs a capacity or placement change, never a
    /// repeated restore.
    ExceedsPool { tokens: u64, pool: u64 },
}

impl RestoreOutcome {
    /// Whether the page stays confirmed (restorable on a compatible destination) after this outcome.
    pub fn keeps_confirmation(self) -> bool {
        !matches!(self, Self::InvalidPage)
    }
}

/// A reproduction that cannot drive the real code yet, named by the seam it lacks. A test holding
/// one fails with [`MissingSeam::describe`] when run with `--ignored`, so the red set lists what is
/// missing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissingSeam {
    /// Scenario 7: the view's turnover / collapse policy lives in the persona's prompt assembly
    /// (`continuum-cognition` prompt/view.rs) and `continuum-context` depends on `continuum-base`
    /// only, so neither can read these recorded items. Needed: the one allocation as a
    /// `continuum-context` policy taking (view chars, appended per act, grant, working set) and
    /// returning which items collapse to handles, drivable from recorded items.
    ViewTurnoverPolicy,
    /// Scenario 5's launch half: `launch_pool` takes no resident page sizes, and the saved ledger
    /// records no page token count (ONE-ACT §5b.1 has Codex persist it with the save receipt).
    LaunchKnowsResidentPages,
    /// §5b.1 "timeout and late receipt retain custody": after an `Uncertain` page switch the
    /// adapter quarantines the endpoint and nothing receives the engine's later terminal receipt,
    /// so no test can deliver one. Needed: an operation custody record the late receipt resolves.
    LateReceiptCustody,
    /// The IntelMac hour (2026-11-05 05:41-06:22Z): seven of eight admitted residents never
    /// completed in 2,462 s. The capture records only the one completed turn's prompt split; the
    /// other seven's prompt sizes, slot assignment and the engine's per-batch order are not in it,
    /// so no service model can be checked against the hour's starvation.
    LiveHourPrompts,
    /// Scenario 10 (card 8569b03d step 3): a mind this seat serves for another node reaches it as a
    /// prompt (`resource_admission::leased_in_sent_samples`, a size a window bounds), never as her
    /// `LaneRequirement`: the generate wire carries none. Needed: the typed requirement on the grid
    /// seam (Codex owns grid), read by the seat's demand beside its residents'.
    LeasedInRequirement,
}

impl MissingSeam {
    pub fn describe(self) -> &'static str {
        match self {
            Self::ViewTurnoverPolicy => "the view's collapse policy is not drivable from recorded items (continuum-context has no turnover policy; it lives in cognition prompt/view.rs)",
            Self::LaunchKnowsResidentPages => "the launch pool is sized without the residents' page sizes (launch_pool has no such input; the ledger has no page token count)",
            Self::LateReceiptCustody => "no custody record receives a late terminal receipt after an Uncertain page switch",
            Self::LiveHourPrompts => "the IntelMac hour's capture lacks the seven unserved residents' prompt sizes, slots and the engine's batch order (per-request-rows.log carries only the completed turn's split)",
            Self::LeasedInRequirement => "a leased-in mind reaches this seat as a prompt size, never her LaneRequirement: the grid's generate wire carries none (Codex owns the seam)",
        }
    }
}
