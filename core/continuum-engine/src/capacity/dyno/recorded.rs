//! What the M5 recorded on 2026-11-04: the dyno's only inputs. Every value is a measurement with
//! its receipt beside it; a value that is not one says so (`shape, not a receipt`) and states
//! the one property a scenario needs from it.

/// Bytes in a decimal gigabyte, as the day's readings were reported.
pub const GB: u64 = 1_000_000_000;

/// The M5 Pro's memory: 64 GiB. Receipt: `sysctl hw.memsize` (ONE-ACT table: "68.7 GB, not 128").
pub const M5_DEVICE_BYTES: u64 = 68_719_476_736;

/// Qwen3.6-35B-A3B's weights, wired. Receipt: the lane footprint, 2026-11-04.
pub const QWEN36_WEIGHTS_BYTES: u64 = 20_419_565_568;

/// The model's trained window. Receipt: the M5 footprint the launch-pool test pins
/// (`serving_daemon/tests/one_launch.rs`, `m5_trained`).
pub const QWEN36_TRAINED_WINDOW: u32 = 262_144;

/// KV bytes per token on the M5 (q4_0 KV, flash attention). Receipt: the M5 footprint the
/// launch-pool test pins (`serving_daemon/tests/one_launch.rs`, `m5.kv_per_token = 8_317`).
pub const M5_KV_BYTES_PER_TOKEN: u64 = 8_317;

/// The served model's footprint on the M5, as the planner sizes with it: the recorded weights,
/// trained window and KV rate over the planner's own constructor. `None` only for zero weights.
pub fn qwen36_on_m5() -> Option<crate::inference::planning::serving_plan::ModelFootprint> {
    let mut footprint = crate::inference::planning::serving_plan::footprint::footprint_from_parts(
        "ggml-org/Qwen3.6-35B-A3B-GGUF",
        QWEN36_WEIGHTS_BYTES,
        QWEN36_TRAINED_WINDOW,
        true,
        None,
    )?;
    footprint.kv_per_token = M5_KV_BYTES_PER_TOKEN;
    Some(footprint)
}

/// The pools one build launched with that day, by launch timing. Receipt: engine `-c` (ONE-ACT table).
pub const LAUNCH_POOLS_SAME_BUILD: [u32; 6] = [33_040, 50_275, 88_425, 125_640, 413_625, 654_336];

/// Whose bytes a memory-board reading still counts as held when it was taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviousEngine {
    /// The previous engine's attribution stands: its memory is wired and not yet observed released.
    StillAttributed { wired_bytes: u64 },
    /// The resource authority observed the previous engine's release (or verified adoption).
    ReleaseObserved,
}

/// One memory-board reading a launch could size from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoardReading {
    pub usable_bytes: u64,
    /// What the board credited back to serving at that reading.
    pub credited_bytes: u64,
    pub previous_engine: PreviousEngine,
}

/// Scenario 1: the cold boot read the board with the old engine still wired.
/// Receipt: board reading usable 22,785,477,836 B, credited 0 (2026-11-04; vm_stat 20:56Z showed
/// 21-24 GB reclaimable with the old engine wired, 45.6 GB after its release at 21:00Z).
pub const COLD_BOOT_OLD_ENGINE_WIRED: BoardReading = BoardReading {
    usable_bytes: 22_785_477_836,
    credited_bytes: 0,
    previous_engine: PreviousEngine::StillAttributed { wired_bytes: QWEN36_WEIGHTS_BYTES },
};

/// Scenario 1: what the engine launched with from that reading. Receipt: engine `-c 125640`.
pub const COLD_BOOT_LAUNCHED_POOL: u32 = 125_640;

/// A mind's demand as the daemon measured it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordedNeed {
    Exact(u32),
    /// A censored observation (`Observed::AtLeast`, #5773).
    AtLeast(u32),
}

/// One resident on the pool, by the dyno's small id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Resident {
    pub name: &'static str,
    pub mind: u32,
    pub need: RecordedNeed,
}

pub const AIKO: u32 = 1;
pub const ARIS: u32 = 2;
pub const ATLAS: u32 = 3;
pub const BENCHY: u32 = 4;

/// Scenario 2: the shared pool when one bound took it. Receipt: `serving.grants` pool_tokens
/// 125,696, grants {Aiko: 114,592}, 20:15-20:35Z.
pub const WHOLE_POOL_GRANT_POOL: u32 = 125_696;

/// Scenario 2: the four residents. Receipt: Aiko's `Observed::AtLeast` 114,592 (the grant she was
/// given); the other three were exact needs of about 40k each (ONE-ACT table: "three dark 20
/// min"). The 40,000 is the rounded receipt; the property the scenario needs is only that any
/// one of them fits beside the others but not beside Aiko's bound (125,696 - 114,592 = 11,104).
pub const WHOLE_POOL_GRANT_RESIDENTS: [Resident; 4] = [
    Resident { name: "Aiko", mind: AIKO, need: RecordedNeed::AtLeast(114_592) },
    Resident { name: "Aris", mind: ARIS, need: RecordedNeed::Exact(40_000) },
    Resident { name: "Atlas", mind: ATLAS, need: RecordedNeed::Exact(40_000) },
    Resident { name: "Benchy", mind: BENCHY, need: RecordedNeed::Exact(40_000) },
];

/// The serving daemon's grant tick. Receipt: the 5-second tick `planning.rs` names.
pub const GRANT_TICK_MS: u64 = 5_000;

/// Scenario 2 and 3: the recorded window: 20:15-20:35Z, 20 minutes.
pub const WHOLE_POOL_GRANT_WINDOW_MS: u64 = 20 * 60 * 1_000;

/// Scenario 3: how often Aiko's turns ended. Receipt: Aiko ~20k prompts, 28 acts in 20 min
/// (20:15-20:35Z), so one act about every 43 s: each completes and the next starts between two
/// grant ticks, and a polled `in_flight` reads true on every tick.
pub const AIKO_ACTS_IN_WINDOW: u64 = 28;

/// Scenario 4: the engine's unified pool when the restore was refused. Receipt: 125,696 cells
/// (the same pool as scenario 2), restore 400 body "failed to find 59144 available cells".
pub const UNIFIED_POOL_CELLS: u64 = 125_696;

/// Scenario 4: the refused page. Receipt: the 400 body names 59,144 cells.
pub const REFUSED_PAGE_CELLS: u64 = 59_144;

/// Scenario 4: what the other residents held beside it. Shape, not a receipt ("others
/// resident"): the property is only that they held more than the pool left the page
/// (125,696 - 59,144 = 66,552).
pub const OTHERS_RESIDENT_CELLS: u64 = 70_000;

/// Scenario 5: the small launch. Receipt: engine `-c 33040`, reported pool 33,280 (the restore
/// body's `size = 33280`).
pub const SMALL_LAUNCH_CTX: u32 = 33_040;
pub const SMALL_POOL_CELLS: u64 = 33_280;

/// Scenario 5: the residents' pages saved under the previous, larger engine. Receipt: page
/// tokens 35,842 and 33,542; restore 400 body "n_tokens = 35838 > size = 33280".
pub const OVERSIZE_PAGES: [u64; 2] = [35_842, 33_542];

/// Scenario 6: the admission hold nobody released. Receipt: `admission.quiesce` held
/// 18:00:46Z-18:16Z, zero generations, growth due and the launch planner refusing the relaunch.
pub const UNRELEASED_HOLD_MS: u64 = (15 * 60 + 14) * 1_000;

/// Scenario 6: the running pool and the pool the ensure's target carried. Shape, not a receipt
/// (the hold's receipt names no pools): the property is only that the daemon asked for growth and
/// the launch planner's target was not larger than what the engine held, so the relaunch the hold
/// waits for was refused.
pub const UNRELEASED_HOLD_POOLS: (u32, u64) = (88_425, 88_425);

/// Scenario 7: Aris's overflowing view. Receipt: per-act appends 17-43k chars on a ~44k-token
/// view; Aris ~89k prompts, 9 acts in 30 min (18:49-19:22Z), against Aiko's ~20k prompts, 28
/// acts in 20 min.
pub const ARIS_VIEW_TOKENS: u64 = 44_000;
pub const ARIS_APPEND_CHARS_PER_ACT: (u64, u64) = (17_000, 43_000);
pub const ARIS_PROMPT_TOKENS: u64 = 89_000;
pub const AIKO_PROMPT_TOKENS: u64 = 20_000;

/// Prefill on the M5 at a large prompt. Receipt: llama-server prompt eval, 141,739 tokens at
/// 148 tok/s; a cold act: Atlas 76,114 tokens, 0 cached, 318 s prefill (21:37Z).
pub const M5_PREFILL_TOK_PER_S_LARGE: u64 = 148;
pub const ATLAS_COLD_PROMPT_TOKENS: u64 = 76_114;
pub const ATLAS_COLD_PREFILL_S: u64 = 318;

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
