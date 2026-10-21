//! serving_plan — honest hardware → persona-serving decision.
//!
//! A deterministic, **classification-based** planner (NOT an LLM — per Joel:
//! "I even considered an LLM cpu-only just to make these decisions… that's
//! probably silly because we can do better with just classification"). Given
//! THIS host's honest memory budget and the candidate model footprints, it
//! answers — with **no grid assumed** — three questions:
//!
//!   1. which base model do we serve? (the most capable one that fits on GPU)
//!   2. how many continuous-batching lanes? (`n_seq_max`)
//!   3. how many distinct models do we keep resident (warm)?
//!
//! It degrades gracefully across the whole hardware range the substrate must
//! run on: an 8 GB M2 Air (Joel's wife's laptop) figures out *something* to
//! run; a 64 GB M5 Pro runs the most capable model it can at up to
//! [`MAX_LANES`] lanes with several models warm.
//!
//! **Footprint-aware.** A rich coding model (a sentinel, a full persona)
//! costs more memory, so fewer fit and fewer lanes run — the plan reflects
//! that, it doesn't pretend every model is the same size.
//!
//! **GPU-residency-first.** A model that cannot fit even ONE lane on the
//! GPU/UMA budget is not silently CPU-served here; the plan reports
//! `fits_on_gpu = false` and names the smallest option, leaving the
//! CPU-exception (a lone Intel Mac) or grid-routing decision to the caller.
//! This honors the "no silent CPU fallback" bar.
//!
//! ## Composition seam (this is a DECISION, not a scheduler)
//!
//! It loads nothing and schedules nothing. Its output feeds two existing
//! primitives: the spawner's base-model pick, and
//! [`crate::inference::planning::adaptive_throughput`] lane budgets
//! (`lanes` → `ThroughputLaneBudget::max_concurrency`). Re-run it whenever
//! the host budget changes — a model evicted under pressure, a LoRA paged in,
//! GPU pressure shifting. Pure function, same shape as `adaptive_throughput`
//! and `model_resolver`.

pub mod footprint;

use crate::inference::planning::MIN_SERVE_CTX;

mod max_lanes;
pub use max_lanes::*;
mod plan_serving;
pub use plan_serving::*;

#[cfg(test)]
mod tests;
