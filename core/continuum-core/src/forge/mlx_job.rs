//! `forge::mlx_job` — the event-driven training JOB around the proven, synchronous
//! mlx_lm.lora path ([`crate::forge::mlx_train::run_mlx_train`], task #32's owned
//! loop, +5.1pts).
//!
//! ## Fire-and-EMIT, never poll (Joel: "not a fan of polling")
//! `run_mlx_train` blocks until the subprocess exits. This wraps it so a caller
//! gets a handle IMMEDIATELY: the blocking run is offloaded to `spawn_blocking`,
//! and lifecycle transitions are published two ways:
//!   1. a `watch` channel → [`current_train_status`] is a cheap READ of the latest
//!      (what `ai/inference`-style status commands project — never a poll loop),
//!   2. the airc bus (`forge.train.{started,done,failed}`) → consumers SUBSCRIBE
//!      (the L3 completion sentinel, any UI, and remote grid towers that offloaded
//!      the run all react to the SAME event — local and cross-grid, no polling).
//!
//! ## Single-resident
//! One native train at a time on a host (it saturates the GPU/UMA). A second
//! `train_start` while one runs REPLACES the observed handle — the caller is
//! expected to gate on [`current_train_status`]`.is_training_running` first.
//!
//! ## Why the trainer is injected
//! `spawn_train_job` takes the training work as a closure so the lifecycle
//! (started → completed/failed transitions + emits) is unit-tested WITHOUT a real
//! multi-minute mlx run. Production passes a closure over `run_mlx_train`; tests
//! pass a fake returning `Ok`/`Err`.

use std::sync::{Arc, Mutex, OnceLock};

use tokio::sync::watch;

use crate::forge::mlx_train::MlxTrainOutput;
use crate::forge::protocol::TrainStatus;
use crate::runtime::MessageBus;

/// Bus topic: a native training run has started (payload `{jobId}`).
pub const EV_TRAIN_STARTED: &str = "forge.train.started";
/// Bus topic: a run finished successfully (payload `{jobId, adapterDir, …}`) —
/// the L3 sentinel's cue to convert → eval → page-in.
pub const EV_TRAIN_DONE: &str = "forge.train.done";
/// Bus topic: a run failed (payload `{jobId, error}`).
pub const EV_TRAIN_FAILED: &str = "forge.train.failed";

/// The single-resident live-training observation seam. Holds the latest job's
/// watch receiver so [`current_train_status`] (and the `ForgeCustodian::train_status`
/// projection) READ it — never a poll loop. `None` = no native run yet this boot.
static ACTIVE_JOB: OnceLock<Mutex<Option<watch::Receiver<TrainStatus>>>> = OnceLock::new();

fn registry() -> &'static Mutex<Option<watch::Receiver<TrainStatus>>> {
    ACTIVE_JOB.get_or_init(|| Mutex::new(None))
}

/// The current native training status — a READ of the last published watch value.
/// Honest empty ([`TrainStatus::default`], `phase: ""`) when nothing has run.
pub fn current_train_status() -> TrainStatus {
    registry()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .map(|rx| rx.borrow().clone())
        .unwrap_or_default()
}

fn running(job_id: &str) -> TrainStatus {
    TrainStatus {
        job_id: job_id.to_string(),
        phase: "training".into(),
        is_training_running: true,
        message: "native mlx_lm.lora run in progress".into(),
        ..Default::default()
    }
}

/// Spawn the training work as a tracked, event-emitting job. Returns immediately
/// (the blocking `train` closure runs on a blocking thread). Publishes `training`
/// now, then `completed`/`failed` on the watch AND (if `bus` is `Some`) the airc
/// bus. The returned job_id is the handle a caller keeps.
pub fn spawn_train_job<F>(job_id: String, bus: Option<Arc<MessageBus>>, train: F) -> String
where
    F: FnOnce() -> Result<MlxTrainOutput, String> + Send + 'static,
{
    let (tx, rx) = watch::channel(running(&job_id));
    *registry().lock().unwrap_or_else(|e| e.into_inner()) = Some(rx);

    if let Some(bus) = &bus {
        bus.publish_async_only(EV_TRAIN_STARTED, serde_json::json!({ "jobId": job_id }));
    }

    let jid = job_id.clone();
    tokio::task::spawn_blocking(move || {
        // The blocking mlx subprocess wait lives HERE, off the async executor.
        match train() {
            Ok(output) => {
                let _ = tx.send(TrainStatus {
                    job_id: jid.clone(),
                    phase: "completed".into(),
                    is_training_running: false,
                    message: format!("adapter: {}", output.adapter_dir.display()),
                    ..Default::default()
                });
                if let Some(bus) = &bus {
                    bus.publish_async_only(
                        EV_TRAIN_DONE,
                        serde_json::json!({
                            "jobId": jid,
                            "adapterDir": output.adapter_dir.display().to_string(),
                            "adaptersSafetensors": output.adapters_safetensors.display().to_string(),
                            "adapterConfig": output.adapter_config.display().to_string(),
                        }),
                    );
                }
            }
            Err(e) => {
                let _ = tx.send(TrainStatus {
                    job_id: jid.clone(),
                    phase: "failed".into(),
                    is_training_running: false,
                    error: Some(e.clone()),
                    ..Default::default()
                });
                if let Some(bus) = &bus {
                    bus.publish_async_only(
                        EV_TRAIN_FAILED,
                        serde_json::json!({ "jobId": jid, "error": e }),
                    );
                }
            }
        }
    });

    job_id
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fake_output() -> MlxTrainOutput {
        MlxTrainOutput {
            adapter_dir: PathBuf::from("/tmp/adapter"),
            adapters_safetensors: PathBuf::from("/tmp/adapter/adapters.safetensors"),
            adapter_config: PathBuf::from("/tmp/adapter/adapter_config.json"),
        }
    }

    async fn drive_until(phase: &str) -> TrainStatus {
        for _ in 0..200 {
            if current_train_status().phase == phase {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        current_train_status()
    }

    // what this catches: the job is FIRE-AND-EMIT — spawn returns immediately with a
    // handle while the (blocking) trainer runs off-thread, the watch transitions
    // training → completed with NO polling, and a FAILING trainer publishes `failed`
    // with the error NAMED (never a silent Completed for a genome that never trained
    // — the exact silent break the Unsloth excision must never introduce). One
    // sequential test because the registry is single-resident by design (one native
    // train per host) — parallel scenarios would legitimately clobber each other.
    #[tokio::test]
    async fn fire_and_emit_success_then_failure_transitions() {
        // success: spawn returns the handle immediately; trainer completes off-thread.
        let jid = spawn_train_job("job-ok".into(), None, || Ok(fake_output()));
        assert_eq!(jid, "job-ok");
        let s = drive_until("completed").await;
        assert_eq!(s.phase, "completed", "reached terminal completed");
        assert!(!s.is_training_running);
        assert!(s.error.is_none());

        // failure: a failing trainer names the error, never a silent completed.
        spawn_train_job("job-err".into(), None, || Err("mlx_lm.lora exploded".into()));
        let f = drive_until("failed").await;
        assert_eq!(f.phase, "failed");
        assert!(!f.is_training_running);
        assert_eq!(f.error.as_deref(), Some("mlx_lm.lora exploded"));
    }
}
