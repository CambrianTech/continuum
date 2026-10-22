//! llama_server.rs — Rust owns the llama-server process.
//!
//! This is LOCAL serving control. llama.cpp's `llama-server` has NO management API — it
//! serves exactly ONE model passed at launch and exposes only `/v1/*`, `/props`,
//! and `/health`. So "switch model" is not an API call; it is a process relaunch.
//! The clean consequence: the core OWNS the C++ launch. No second server
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

mod configuration;
use configuration::{
    first_free_port, is_debug_build, split_host_port, DECODE_SMOKE_TIMEOUT,
    EPHEMERAL_READY_TIMEOUT, PROBE_TIMEOUT,
};
pub use configuration::{
    moe_glass_box_paths, port_of_base_url, serving_root, serving_v1_url, MoeGlassBoxPaths,
    DEFAULT_BASE_URL, READY_TIMEOUT,
};
mod liveness;
mod readiness;
pub use liveness::{
    classify_never_started_timeout, classify_real_decode, consecutive_real_decode_failures,
    judge_smoke_miss, ms_since_real_decode, ms_since_real_work, note_real_decode,
    note_real_decode_failure, note_real_prefill_progress, reset_real_decode_failures,
    slots_activity_fingerprint_of, slots_max_inflight_prompt_tokens_of, wedge_miss_threshold,
    NeverStartedClass, RealDecodeOutcome, SmokeMissVerdict,
};

mod reconcile;
pub use reconcile::{ensure_model_serving, ensure_model_serving_if_current};
pub(crate) use reconcile::{grow_refused_by_memory_gate, lane_grow_would_narrow_the_window};

mod smoke;
use smoke::{
    smoke_attempt, smoke_verdict, SmokeVerdict, QUICK_SMOKE_BUDGET, THINKING_SMOKE_BUDGET,
};
mod target;
pub use target::{
    cold_expert_offload_ot, main_lane_placement, placement_from_config, AdapterEntry,
    LanePlacement, ServingTarget,
};
mod kv_support;
use kv_support::ensure_engine_kv_support_recorded;
mod pages;
pub(crate) use pages::collect_retired_engines;
pub use pages::{
    kv_page_dir, kv_pages_root, page_dirs_of, page_geometry_key, sweep_stale_page_generations,
    PAGE_GEOMETRY_STEP,
};
use pages::{
    retire_page_owner, revision_evidence, sweep_stale_page_generations_guarded, PageDirReservation,
    PAGE_DIR_GUARD,
};
mod genome;
pub(crate) use genome::gene_stage_name;
pub use genome::{engine_accepts_train_dir, engine_train_dir, GenomeInPlace};
use genome::{engine_help_lists_train_dir, ENGINE_TRAIN_DIR};
mod engine_install;
#[cfg(windows)]
pub(crate) use engine_install::engine_override;
pub(crate) use engine_install::{installed_engine_commit, lane_runs_installed_engine};
pub use engine_install::{
    engine_stale, warm_build_jobs_for_memory, WARM_BUILD_FIRST_JOB_BYTES, WARM_BUILD_JOB_BYTES,
    WARM_BUILD_MIN_FREE_BYTES, WARM_BUILD_RESERVE_BYTES, WARM_BUILD_UNCAPPED_FREE_BYTES,
};
use engine_install::{
    mark_engine_proven, roll_back_engine_after_failed_launch, server_bin, ENGINE_CONVERGE_TRIED,
    ENGINE_STALE,
};
mod snapshot;
use snapshot::SERVING_STATE;
pub use snapshot::{
    await_ready_serving, await_snapshot_resettle, current_serving, has_reconciled,
    install_serving_state, mark_first_reconcile, serving_state_receiver, MultimodalSupport,
    ServingSnapshot, SnapshotSettle, DEFAULT_SERVING_WAIT, PROVIDER_ID,
};
mod vision;
pub use vision::vision_lane_ready;
use vision::{main_lane_mmproj_decision_for, MainLaneMmproj};
mod external;
pub use external::{external_serving_pin, probe_external_serving, wait_for_serving_window_settle};

mod progress;
pub(crate) use progress::{
    engine_condemned, engine_probe, engine_probe_within, engine_props, restore_probe,
    slots_progress, wait_while_engine_progresses, EngineProbe, EngineProgress,
};

mod slot_work;
pub(crate) use slot_work::{
    in_flight_slot, slot_advanced, slot_work, slot_work_of, InFlightSlotGuard,
    SlotWork,
};

mod control;
pub use control::{
    EngineRetirementError, EngineRetirementReceipt, EngineRetirementStatus, EnsureOutcome,
    LlamaServerControl, LlamaServerError, OwnedEngineIdentity, OwnedRestoreSession,
    OwnedServingTarget, ServingLifecycleCheck,
};
mod prepared_launch;
use prepared_launch::{unsupported_launch_environment, PreparedLocalLaunch};
mod process;
pub use process::LlamaServerProcess;
use process::OwnedEngine;
mod ephemeral;
pub use ephemeral::{
    install_lane_admission, lane_admission_installed, EphemeralServingLane, LaneAdmission,
    LaneAdmissionAuthority,
};

mod control_impl;
mod launch;
mod placement_stamp;
mod prepare;

#[cfg(test)]
mod tests;
