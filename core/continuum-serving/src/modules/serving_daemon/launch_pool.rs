//! The engine's one KV pool: sized once, at launch, from a settled budget reading only.
//!
//! `--kv-unified` allocates the whole pool up front and one launch keeps it for the engine's life
//! (no growth relaunch), so the reading it is sized from must be one that would not change. The
//! budget owner says which readings those are ([`BudgetReading`], `own_credit`); this module turns
//! one into the `-c` / `--parallel` a launch commits, and refuses a transient one.

use super::*;
use crate::inference::planning::serving_plan::ModelFootprint;

/// The pool a launch commits, decided from the budget owner's reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LaunchPool {
    /// A settled reading sized it: `-c` is `per_sequence x sequences`, `--parallel` `sequences`.
    Sized { per_sequence: u32, sequences: u32 },
    /// The reading is transient: a running engine of the target model keeps its own pool (an
    /// adoption, or a same-size relaunch for its genome, sight, KV type or binary), and no engine
    /// launches until the reading settles.
    AwaitSettled { sequences: u32 },
}

impl LaunchPool {
    /// PURE: the pool `reading` allows `sequences` of `footprint`'s model, beside the plan's own
    /// window x lanes ([`launch_pool`]). A transient reading sizes nothing.
    pub(super) fn from_reading(
        reading: BudgetReading,
        footprint: Option<&ModelFootprint>,
        plan_window: u32,
        lanes: u32,
        sequences: u32,
        trained_window: u32,
    ) -> Self {
        let sequences = sequences.max(1);
        match reading {
            BudgetReading::Settled(budget_bytes) => {
                let pool = launch_pool(footprint, budget_bytes, plan_window, lanes, sequences, trained_window);
                // `-c` is the CLI's `slots x per-slot`: the pool carried in that shape, not anyone's window.
                let per_sequence = u32::try_from(pool / u64::from(sequences)).unwrap_or(u32::MAX); // unwrap_or: a pool over u32 x sequences is no real engine; the CLI caps it
                Self::Sized { per_sequence, sequences }
            }
            BudgetReading::Transient => Self::AwaitSettled { sequences },
        }
    }

    /// PURE: the `(per_sequence, sequences)` this launch may commit, given the pool the engine
    /// already serving the target model reports (`None` when no engine answers, or it serves
    /// another model). `None` back: nothing launches on this reading.
    pub(super) fn commit(self, running_same_model_pool: Option<u32>) -> Option<(u32, u32)> {
        match self {
            Self::Sized { per_sequence, sequences } => Some((per_sequence, sequences)),
            Self::AwaitSettled { sequences } => running_same_model_pool
                .filter(|pool| *pool > 0)
                .map(|pool| (pool / sequences, sequences)),
        }
    }
}

/// The pool the engine serving `model_id` reports holding, read from the engine itself; `None`
/// when nothing answers, it serves another model, or its pool is unreadable.
pub(super) async fn running_pool_of<C: LlamaServerControl + ?Sized>(server: &C, model_id: &str) -> Option<u32> {
    match server.active_model().await {
        Ok(Some(active)) if active == model_id => server.served_pool_tokens().await.ok(),
        _ => None,
    }
}

/// The pool an engine launches with: the most memory the host's measured capacity proves fits.
/// That is the fit at `sequences` (a window within `budget_bytes` for each, capped at the trained
/// window) or the plan's own window x its lanes, whichever is larger.
///
/// `budget_bytes` is a SETTLED reading ([`BudgetReading::Settled`]): what memory holds once the
/// view's transient has passed, beside everything else on the node. Never the host's physical
/// total (590df4bc3: 5 x 262,144 on the M5, memory pressure 91%, rolled back), and never a cold
/// boot's transient (b4be7bc59: 5 x 2,048; 87eccb3a7 at 20:15Z: 125,640 from a 22.8 GB first
/// reading on a 128 GB machine). On the M5 (plan 2 x 128,768) the result is 257,536 for 5 at a
/// 23 GB budget; on the IntelMac (plan 2 x 32,768, fit 9 x 32,768) 294,912 for 9.
pub(super) fn launch_pool(
    footprint: Option<&ModelFootprint>,
    budget_bytes: u64,
    plan_window: u32,
    lanes: u32,
    sequences: u32,
    trained_window: u32,
) -> u64 {
    let plan_pool = u64::from(plan_window) * u64::from(lanes);
    let fit_pool = footprint.map_or(0, |fp| {
        u64::from(fp.window_within(budget_bytes, sequences).min(trained_window)) * u64::from(sequences)
    }); // map_or: no footprint proves no fit beyond the plan's own pool
    plan_pool.max(fit_pool)
}
