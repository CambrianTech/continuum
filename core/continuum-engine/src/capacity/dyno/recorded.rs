//! What the M5 recorded on 2026-11-04: the dyno's only inputs. The values are DATA
//! (`fixtures/dyno/2026-11-04.json`), each beside its receipt; this module is their typed shape and the
//! engine's refusal wording. A value that is not a measurement says so in its receipt ("SHAPE,
//! not a receipt") and names the one property a scenario uses from it.

use crate::inference::planning::serving_plan::ModelFootprint;

/// The day's data file, compiled in so a run needs no file system.
const RECORDED_JSON: &str = include_str!("../../../fixtures/dyno/2026-11-04.json");

/// A recorded value and where it was measured.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct Receipted<T> {
    pub value: T,
    pub receipt: String,
}

/// Everything the M5 recorded that the dyno replays.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct Recorded {
    pub m5_device_bytes: Receipted<u64>,
    pub qwen36_weights_bytes: Receipted<u64>,
    pub qwen36_trained_window: Receipted<u32>,
    pub m5_kv_bytes_per_token: Receipted<u64>,
    /// The engine's sequences: the residents and the scratch traffic.
    pub m5_sequences: Receipted<u32>,
    pub launch_pools_same_build: Receipted<Vec<u32>>,
    /// Dyno 1.
    pub cold_boot: Receipted<ColdBoot>,
    /// Dyno 2 and 3.
    pub whole_pool_grant: Receipted<WholePoolGrant>,
    pub grant_tick_ms: Receipted<u64>,
    pub aiko_turns: Receipted<TurnCadence>,
    /// Dyno 4.
    pub restore_free_cells: Receipted<FreeCellRefusal>,
    /// Dyno 5.
    pub oversize: Receipted<Oversize>,
    /// Dyno 6.
    pub unreleased_hold: Receipted<UnreleasedHold>,
    /// Dyno 7.
    pub aris_view: Receipted<OverflowingView>,
    pub m5_prefill_tok_per_s_large: Receipted<u64>,
}

/// Whose bytes a memory-board reading still counts as held when it was taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreviousEngine {
    /// The previous engine's attribution stands: its memory is wired and not yet observed released.
    StillAttributed { wired_bytes: u64 },
    /// The resource authority observed the previous engine's release (or verified adoption).
    ReleaseObserved,
}

/// One memory-board reading a launch could size from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
pub struct BoardReading {
    pub usable_bytes: u64,
    /// What the board credited back to serving at that reading.
    pub credited_bytes: u64,
    pub previous_engine: PreviousEngine,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
pub struct ColdBoot {
    pub reading: BoardReading,
    /// The pool the engine launched with from that reading.
    pub launched_pool: u32,
}

/// A mind's demand as the daemon measured it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordedNeed {
    Exact(u32),
    /// A censored observation (`Observed::AtLeast`, #5773).
    AtLeast(u32),
}

/// One resident on the pool, by the dyno's small id.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct Resident {
    pub name: String,
    pub mind: u32,
    pub need: RecordedNeed,
}

/// The shared pool when one bound took it, and the grant the holder held.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct WholePoolGrant {
    pub pool: u32,
    pub holder: u32,
    pub held: u32,
    pub residents: Vec<Resident>,
}

/// How often a mind's turns ended over a window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
pub struct TurnCadence {
    pub window_ms: u64,
    pub acts: u64,
}

impl TurnCadence {
    /// The time between two of her turn ends.
    pub fn act_ms(&self) -> u64 {
        self.window_ms / self.acts.max(1)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
pub struct FreeCellRefusal {
    pub pool_cells: u64,
    pub page_cells: u64,
    pub others_resident_cells: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
pub struct Oversize {
    pub launch_ctx: u32,
    pub pool_cells: u64,
    pub pages: [u64; 2],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
pub struct UnreleasedHold {
    pub held_ms: u64,
    pub running_pool: u32,
    pub target_pool: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
pub struct OverflowingView {
    pub view_tokens: u64,
    pub append_chars_per_act: [u64; 2],
    pub prompt_tokens: u64,
    pub aiko_prompt_tokens: u64,
}

impl Recorded {
    /// The day's data, parsed. An error names the field that does not match this shape.
    pub fn load() -> Result<Self, serde_json::Error> {
        serde_json::from_str(RECORDED_JSON) // file format: the day's recorded data file, 2026-11-04.json
    }

    /// The served model's footprint on the M5, as the planner sizes with it: the recorded
    /// weights, trained window and KV rate over the planner's own constructor. `None` only for
    /// zero weights.
    pub fn qwen36_on_m5(&self) -> Option<ModelFootprint> {
        let mut footprint = crate::inference::planning::serving_plan::footprint::footprint_from_parts(
            "ggml-org/Qwen3.6-35B-A3B-GGUF",
            self.qwen36_weights_bytes.value,
            self.qwen36_trained_window.value,
            true,
            None,
        )?;
        footprint.kv_per_token = self.m5_kv_bytes_per_token.value;
        Some(footprint)
    }
}

/// The engine's restore-refusal bodies, in the engine's own words (one owner of their shape, so
/// the fake engine and every assertion speak them identically). Receipt: the 2026-11-04 bodies
/// "failed to find 59144 available cells" and "n_tokens = 35838 > size = 33280".
pub fn free_cells_refusal(cells: u64) -> String {
    format!("failed to find {cells} available cells")
}

/// See [`free_cells_refusal`].
pub fn exceeds_pool_refusal(tokens: u64, size: u64) -> String {
    format!("n_tokens = {tokens} > size = {size}")
}

/// A refusal for a page saved under another format. No receipt: the fork has no typed reason for
/// it yet (ONE-ACT §5b.1 has Codex define it); the fake engine speaks this placeholder until then.
pub const INCOMPATIBLE_REFUSAL: &str = "dyno: saved state was written under another layout";

/// A refusal on positive evidence the page itself is bad. No receipt, as above.
pub const CORRUPT_REFUSAL: &str = "dyno: saved state is truncated";
