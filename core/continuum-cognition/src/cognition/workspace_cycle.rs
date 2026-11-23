//! The mind that runs the workspace: it assembles a persona's faculties, ticks them over a
//! consolidated burst, and drives what the tick decides into her hands and memory.
//!
//! It sits ABOVE the [`workspace`](super::workspace) floor in the lineage. The floor holds the
//! vocabulary every faculty reads and writes (the [`Workspace`], its [`Burst`], the [`Faculty`]
//! contract, [`Contribution`], [`Decision`], the [`Arbiter`]); the concrete faculties stand on that
//! floor, and this cycle, which holds her working memory, admission state and tool executor, stands
//! on them. A faculty never reaches the cycle: it is handed the workspace, never the mind.

use std::sync::Arc;

use futures_util::future::join_all;
use uuid::Uuid;

use super::llm_deliberation_faculty::{
    empty_genome, relaxed_decoding, DecodingHandle, GenomeHandle, ModelBinding, ModelBindingHandle,
};
use super::workspace::*;
use crate::ai::types::ActiveAdapterRequest;

mod assembly;
mod body;
pub use body::ActingBody;
mod eval_isolation;
pub use eval_isolation::EvalIsolation;
mod tick;

#[cfg(test)]
mod tests;


/// One service-tick of cognition over a CONSOLIDATED burst (never per-event):
/// every faculty bids in parallel over the same world-state, the arbiter
/// integrates the bids into the bounded workspace, the workspace broadcasts.
/// The participation [`Decision`] is then read from the broadcast.
pub struct WorkspaceCycle {
    faculties: Vec<Arc<dyn Faculty>>,
    arbiter: Arc<dyn Arbiter>,
    /// Replay/logging seam — every tick is recorded here. Default `Noop`
    /// (zero hot-path cost); swap in a recording sink to make the mind a
    /// glass box for debugging, tuning, and test benches.
    capture: Arc<dyn WorkspaceCaptureSink>,
    /// The persona's hands + hippocampus + identity for the act→observe driver.
    /// `None` → no hands (pure cognition). See [`ActingBody`].
    acting: Option<Arc<ActingBody>>,
    /// The persona's paged-in genome — the LoRA layers active for generation. The
    /// deliberation faculty shares this exact handle and reads it wait-free on
    /// every generation; [`page_in`](Self::page_in)/[`page_out`](Self::page_out)
    /// swap which gene is active (virtual memory for skill). Empty → base model.
    /// This is the seam `cognition/eval` A/Bs base vs a candidate gene over.
    genome: GenomeHandle,
    /// Shared decoding-temperature override ([`DecodingHandle`]) — the same
    /// `ArcSwap` the deliberation faculty reads wait-free on every generation.
    /// `None` (live cognition) → her configured warmth; the eval window
    /// ([`isolate_for_eval`]) flips it to `Some(0.0)` (greedy) so the reward metric
    /// is reproducible, and restores it on the guard's drop. Mirrors `genome`: one
    /// handle, two holders (cycle + faculty).
    decoding: DecodingHandle,
    /// The persona's live model binding — the served adapter + requested model +
    /// context window the deliberation faculty reads wait-free each generation. The
    /// faculty shares this EXACT handle; [`rebind_model`](Self::rebind_model) swaps
    /// it atomically when the served model changes (`serving/pin` or grid failover),
    /// carrying the genome + memory across untouched (no cycle rebuild). `None` →
    /// a pure-cognition / test cycle with no deliberation faculty to re-home (the
    /// re-home is then a benign no-op). Mirrors `genome`/`decoding`: one handle, two
    /// holders. See [`ModelBinding`] + [[seamless-persona-failover-model-and-genome]].
    model_binding: Option<ModelBindingHandle>,
    /// Where a room's authored manifest comes from (S1). Read once per tick to stamp
    /// [`Workspace::room_affordances`]. `None` = no recipe registry was shared in (a
    /// pure-cognition / test cycle): nothing is stamped and the surface is pre-S1.
    experience: Option<crate::experience::source::SharedExperienceSource>,
    /// Monotonic service-tick counter — the source of each [`Workspace::cycle`]
    /// frame index. Interior-mutable because `run` takes `&self` (the living
    /// mind is shared, not owned per tick); bumped once per `run_in_room`.
    /// Starts at 0 so the first live cycle is `CycleId(1)` and `CycleId(0)`
    /// stays the UNSTAMPED sentinel.
    cycle_counter: std::sync::atomic::AtomicU64,
    /// #169 per-turn STREAMING sink — the output channel for the CURRENT turn's
    /// answer tokens, or `None` when this turn is not streamed (the default; every
    /// non-live caller and test). Interior-mutable (like `cycle_counter`) because
    /// `run_in_room_inner` takes `&self`: the live caller (service_loop) sets it
    /// `Some` just before a streamed turn and clears it after, and
    /// `run_in_room_inner` reads it into the per-turn [`Workspace::token_sink`]. A
    /// std `Mutex` — set/read is a cheap clone of a refcounted `Sender`, never held
    /// across an await. Safe: a persona's live turns are sequential, and eval runs
    /// on a SEPARATE forked cycle, so no cross-turn contention.
    token_sink: parking_lot::Mutex<
        Option<tokio::sync::mpsc::UnboundedSender<crate::ai::adapter::GenerationChunk>>,
    >,
    /// #186 glass-box: the decaying per-axis "which faculty is firing" accumulator the
    /// vitals radiator samples (Focus/Reason/Recall/Act → the tile's live compass). The
    /// tick seam bumps it from the faculties this cycle already runs + times; the acting
    /// seam bumps Act. PURE OBSERVABILITY — no decision path ever reads it. `Arc` so the
    /// radiator can hold a cheap clone without the cycle lock. See [`FacultyPulse`].
    faculty_pulse: Arc<super::faculty_pulse::FacultyPulse>,
    /// #266 KV-PREFILL ECONOMY: lifetime `cache_n` / `prompt_n` for THIS persona,
    /// folded once per generation at the settle seam. Interior-mutable (like
    /// `cycle_counter`) because the settle path holds `&self`.
    ///
    /// WHY A LIFETIME ACCUMULATOR AND NOT THE LAST TICK: [`Workspace::metrics`]
    /// carries the winning verdict of ONE tick, and one sample of a live system is
    /// not a fact about it — a single cold turn reads 0% and a single warm one
    /// reads 90%. The rate that answers "is this citizen's prefix staying resident"
    /// is over her turns, not her latest. Same reasoning FacultyPulse applies to
    /// axis levels; this is its prefill sibling.
    ///
    /// SEPARATE COUNTERS, RATIO DERIVED ON READ — never a stored rate. Averaging
    /// rates lies (the same rule [`TurnMetrics::accumulate`] states for tok/s), so
    /// the meter divides totals, and a persona whose turns are mostly-warm cannot
    /// have one cold turn drag the mean.
    ///
    /// PURE OBSERVABILITY — no decision path reads these; nothing routes, budgets,
    /// or gates on them. They exist so live citizens' cache behaviour is visible at
    /// all: before this, `cache_n` was parsed, typed, threaded through
    /// [`TurnMetrics`] and then dropped at turn end, and the ONLY consumer that ever
    /// turned it into a number was the eval harness. Components green, wiring to the
    /// place it matters absent — the shape that produced `[no reporters]`.
    kv_cached_tokens: std::sync::atomic::AtomicU64,
    kv_prefill_tokens: std::sync::atomic::AtomicU64,
}
