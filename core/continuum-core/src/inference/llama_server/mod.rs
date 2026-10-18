//! llama_server — Rust owns the llama-server process.
//!
//! This is the unsloth-Studio replacement for LOCAL serving control. Where
//! `unsloth_control` drove unsloth-Studio's management API (`/api/inference/load`
//! etc.) over HTTP, llama.cpp's `llama-server` has NO such management surface — it
//! serves exactly ONE model passed at launch and exposes only `/v1/*`, `/props`,
//! and `/health`. So "switch model" is not an API call; it is a process relaunch.
//! The clean consequence: the core OWNS the C++ launch. No second Python Studio
//! racing us for the GPU, no dual-server confusion — one supervised child, one
//! source of truth for "what is being served right now."
//!
//! Shape: this module is the *leaf* of the serving stack. The
//! [`crate::modules::serving_daemon::ServingDaemonModule`] is the canonical
//! ServiceModule (own tick + `watch` publish, per CONCURRENCY-STYLE-GUIDE.md); it
//! DECIDES which model to serve and how many lanes, then asks this process-owner
//! to reconcile the running `llama-server` to that decision. Degrade-never-panic:
//! a spawn or readiness failure surfaces as [`EnsureOutcome::Degraded`] with a
//! named reason, never an `unwrap`/panic on the daemon's tick.
//!
//! The published [`ServingSnapshot`] (which model is live, is it ready, on what
//! base url) is also the grid seam: a node going down makes its snapshot go
//! stale/empty; a node coming online publishes a fresh one. A grid allocator
//! contracts inference leases against these snapshots. That layer is deferred
//! (single-machine first), but the snapshot is the rail it slots onto.
//!
//! One directory, one file per owner (a pure move of the former single file):
//!
//! | file | owns |
//! |---|---|
//! | `endpoint` | host/port, serving root + `/v1` url, port scan, per-port glass-box paths |
//! | `target` | `ServingTarget`, `LanePlacement`, `AdapterEntry`, the `-ot` cold-expert value |
//! | `engine` | which `llama-server` binary runs, its `--help`/`--version` probes, rollback/proven marks, stale-engine facts |
//! | `snapshot` | `ServingSnapshot` and the process-wide serving state + its readers and settle waits |
//! | `control` | the `LlamaServerControl` trait, its errors/outcomes, the owned-launch vocabulary |
//! | `reconcile` | `ensure_model_serving*`: adopt or relaunch, and the grow-ladder predicates |
//! | `liveness` | the lane's delivery record and the pure wedge/starvation classifiers |
//! | `probe` | bounded `/props` + `/slots` reads, `EngineProbe`, the checkpointed wait, in-flight slots |
//! | `smoke` | the decode smoke probe and its verdict |
//! | `readiness` | bring-up budgets, load-progress allowance, `wait_ready`, the stderr-log tail |
//! | `page_dirs` | KV page-dir geometry, the stale-generation sweep, THE PAGE-DIR GUARD |
//! | `genome` | in-place LoRA load/unload/stage on a running engine, adapter dormancy |
//! | `sight` | mmproj placement decision and the vision-readiness verdict |
//! | `external` | an operator-pinned external endpoint, probed and adopted |
//! | `process` | `OwnedEngine`, `LlamaServerProcess`, constructors, child install/retire/exit |
//! | `process_control` | `impl LlamaServerControl for LlamaServerProcess` |
//! | `launch` | `PreparedLocalLaunch`, `prepare_local_launch`, `launch_prepared` |
//! | `ephemeral` | the cold-spawn gate, device-budget admission, `EphemeralServingLane` |

use crate::model_registry::Model;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::watch;
use ts_rs::TS;

mod control;
mod endpoint;
mod engine;
mod ephemeral;
mod external;
mod genome;
mod launch;
mod liveness;
mod page_dirs;
mod probe;
mod process;
mod process_control;
mod readiness;
mod reconcile;
mod sight;
mod smoke;
mod snapshot;
mod target;

// The siblings share one namespace, as the single file did: a file that reads a
// sibling's module-private item imports `super::*`, and these globs (with the
// re-exports below) are what `super` holds.
use self::endpoint::*;
use self::engine::*;
use self::launch::*;
use self::page_dirs::*;
use self::probe::*;
use self::process::*;
use self::readiness::*;
use self::sight::*;
use self::smoke::*;
use self::snapshot::*;

// The public surface, unchanged: every path that resolved on the single file resolves here.
pub use self::control::{
    EngineRetirementError, EngineRetirementReceipt, EngineRetirementStatus, EnsureOutcome,
    LlamaServerControl, LlamaServerError, OwnedEngineIdentity, OwnedRestoreSession,
    OwnedServingTarget, ServingLifecycleCheck,
};
pub use self::endpoint::{
    moe_glass_box_paths, port_of_base_url, serving_root, serving_v1_url, MoeGlassBoxPaths,
    DEFAULT_BASE_URL,
};
pub use self::engine::{
    engine_accepts_train_dir, engine_stale, engine_train_dir, warm_build_jobs_for_memory,
    WARM_BUILD_FIRST_JOB_BYTES, WARM_BUILD_JOB_BYTES, WARM_BUILD_MIN_FREE_BYTES,
    WARM_BUILD_RESERVE_BYTES, WARM_BUILD_UNCAPPED_FREE_BYTES,
};
pub use self::ephemeral::{
    install_lane_admission, lane_admission_installed, EphemeralServingLane, LaneAdmission,
    LaneAdmissionAuthority,
};
pub use self::external::{external_serving_pin, probe_external_serving};
pub use self::genome::GenomeInPlace;
pub use self::liveness::{
    classify_never_started_timeout, classify_real_decode, consecutive_real_decode_failures,
    judge_smoke_miss, ms_since_real_decode, ms_since_real_work, note_real_decode,
    note_real_decode_failure, note_real_prefill_progress, reset_real_decode_failures,
    slots_activity_fingerprint_of, slots_max_inflight_prompt_tokens_of, wedge_miss_threshold,
    NeverStartedClass, RealDecodeOutcome, SmokeMissVerdict,
};
pub use self::page_dirs::{
    kv_page_dir, kv_pages_root, page_dirs_of, page_geometry_key, sweep_stale_page_generations,
    PAGE_GEOMETRY_STEP,
};
pub use self::process::LlamaServerProcess;
pub use self::readiness::READY_TIMEOUT;
pub use self::reconcile::{ensure_model_serving, ensure_model_serving_if_current};
pub use self::sight::{vision_lane_ready, MultimodalSupport};
pub use self::snapshot::{
    await_ready_serving, await_snapshot_resettle, current_serving, has_reconciled,
    install_serving_state, mark_first_reconcile, serving_state_receiver,
    wait_for_serving_window_settle, ServingSnapshot, SnapshotSettle, DEFAULT_SERVING_WAIT,
    PROVIDER_ID,
};
pub use self::target::{
    cold_expert_offload_ot, main_lane_placement, placement_from_config, AdapterEntry,
    LanePlacement, ServingTarget,
};

pub(crate) use self::engine::{installed_engine_commit, lane_runs_installed_engine};
// The engine-slot capture is its only caller outside this module, and it is Windows-only.
#[cfg(windows)]
pub(crate) use self::engine::engine_override;
pub(crate) use self::genome::gene_stage_name;
pub(crate) use self::page_dirs::collect_retired_engines;
pub(crate) use self::probe::{
    engine_condemned, engine_probe, engine_props, in_flight_slot, restore_probe, slot_advanced,
    slot_work, wait_while_engine_progresses, EngineProbe, InFlightSlotGuard,
};

// Read from outside this module only by `turn_admission`'s tests.
#[cfg(test)]
pub(crate) use self::probe::{slots_progress, EngineProgress};
#[cfg(test)]
mod tests;
