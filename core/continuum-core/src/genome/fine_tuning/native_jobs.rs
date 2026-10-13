//! Shared job ownership for native trainers. Backends prepare inputs and interpret their
//! artifacts; handles, cancellation, probes and terminal state live here, whether the run is a
//! trainer PROCESS or a run driven IN PLACE on something already running (the engine's /train).
use super::{FineTuningError, JobHandle, LoRAHyperparams, ScheduleParams, TrainingArtifact, TrainingJobRequest, TrainingStatus};
use dashmap::DashMap;
use std::{path::PathBuf, sync::Arc, time::Instant};
use tokio::sync::watch;
use uuid::Uuid;

struct JobSlot {
    status: watch::Receiver<TrainingStatus>,
    cancel: watch::Sender<bool>,
}

pub(super) struct NativeJobs {
    provider: &'static str,
    slots: DashMap<Uuid, JobSlot>,
}

/// A prepared job: its run, driven IN PLACE on something already running (the engine's
/// `/train` on the lane that serves the base, where the weights are the served ones and there
/// is no process of ours to kill, so cancelling is the run's own business: [`InPlaceRun`]).
pub(super) struct PreparedJob {
    pub run: Box<dyn InPlaceRun>,
    pub finish: Box<dyn FnOnce(u64) -> Result<TrainingArtifact, String> + Send>,
}


/// A run driven in place. `run` resolves only when the run has ENDED where it happens. On a
/// cancel it stops the run there and returns [`InPlaceEnd::Cancelled`] once it HAS stopped:
/// the job reports Cancelled (and the backend's admission is released) only then, or the next
/// admission would collide with a run still holding the memory.
#[async_trait::async_trait]
pub(super) trait InPlaceRun: Send {
    /// Where the run happens, for the job's `started` receipt (the lane's address).
    fn place(&self) -> String;
    async fn run(self: Box<Self>, cancel: watch::Receiver<bool>, progress: RunProgress) -> InPlaceEnd;
}

pub(super) enum InPlaceEnd {
    Finished,
    Cancelled,
    Failed(String),
}

/// What an in-place run may publish while it runs: progress, never a terminal state (those
/// are the job owner's, like [`PreparationProgress`]).
pub(super) struct RunProgress(watch::Sender<TrainingStatus>);

impl RunProgress {
    pub fn running(&self, progress_pct: f32, current_epoch: u32) {
        self.0.send_replace(TrainingStatus::Running {
            progress_pct,
            current_epoch,
        });
    }
}

/// Backends may describe preparation, but only the job owner can publish
/// running or terminal states. This shares the existing status channel.
pub(super) struct PreparationProgress(watch::Sender<TrainingStatus>);

impl PreparationProgress {
    pub fn waiting_for_capacity(&self, required_bytes: u64, available_bytes: u64) {
        self.0.send_replace(TrainingStatus::WaitingForCapacity {
            required_bytes,
            available_bytes,
        });
    }
}

impl NativeJobs {
    pub fn new(provider: &'static str) -> Self {
        Self {
            provider,
            slots: DashMap::new(),
        }
    }

    pub fn poll(&self, handle: &JobHandle) -> Result<TrainingStatus, FineTuningError> {
        if handle.provider_id != self.provider {
            return Err(FineTuningError::UnknownHandle(handle.clone()));
        }
        self.slots
            .get(&handle.local_id)
            .map(|s| s.status.borrow().clone())
            .ok_or_else(|| FineTuningError::UnknownHandle(handle.clone()))
    }

    pub fn cancel(&self, handle: &JobHandle) -> Result<(), FineTuningError> {
        self.poll(handle)?;
        if let Some(slot) = self.slots.get(&handle.local_id) {
            slot.cancel.send_replace(true);
        }
        Ok(())
    }

    pub fn prepare<F>(
        &self,
        id: Uuid,
        prepare: impl FnOnce(PreparationProgress) -> F + Send + 'static,
    ) -> JobHandle
    where
        F: std::future::Future<Output = Result<PreparedJob, FineTuningError>> + Send + 'static,
    {
        let (tx, rx) = watch::channel(TrainingStatus::Queued);
        let (cancel, mut cancelled) = watch::channel(false);
        self.slots.insert(id, JobSlot { status: rx, cancel });
        // EVERY STEP OF A LOCAL TRAINING JOB IS A RECEIPT (Joel, 2026-09-26, before Kimi's
        // first run: "make sure all probes in place"). Before this, a job's whole life —
        // prepared, admitted, spawned, finished, failed, cancelled — was a `TrainingStatus`
        // in a watch channel that nobody outside the poller could read; the 5090's ledger
        // ended in three killed-by-reboot jobs with no row saying when or why. One class
        // per transition, the job id on every row, so the room can follow a run.
        let provider = self.provider;
        crate::probe!(
            class = "training.job.queued",
            job = %id,
            provider,
            "a local training job is queued — preparation (plan + admission) begins"
        );
        tokio::spawn(async move {
            let queued_at = Instant::now();
            let prepare = prepare(PreparationProgress(tx.clone()));
            let prepared = tokio::select! {
                biased;
                _ = cancelled.changed() => {
                    crate::probe!(
                        class = "training.job.cancelled",
                        job = %id,
                        provider,
                        phase = "preparing",
                        ms = queued_at.elapsed().as_millis() as u64,
                        "a local training job was cancelled before it spawned"
                    );
                    tx.send_replace(TrainingStatus::Cancelled);
                    return;
                }
                result = prepare => result,
            };
            let PreparedJob { run, finish } = match prepared {
                Ok(job) => job,
                Err(error) => {
                    crate::probe!(
                        class = "training.job.prepare_failed",
                        job = %id,
                        provider,
                        ms = queued_at.elapsed().as_millis() as u64,
                        error = %error,
                        "a local training job failed before it spawned (plan, admission, \
                         or the trainer's own prerequisites) — nothing ran on the card"
                    );
                    tx.send_replace(TrainingStatus::Failed {
                        error: error.to_string(),
                    });
                    return;
                }
            };
            let (started, ended) = {
                let started = Instant::now();
                crate::probe!(
                    class = "training.job.started",
                    job = %id,
                    provider,
                    place = run.place().as_str(),
                    prepared_ms = queued_at.elapsed().as_millis() as u64,
                    "the training run is running in place (on the lane that serves the base)"
                );
                tx.send_replace(TrainingStatus::Running {
                    progress_pct: 0.0,
                    current_epoch: 0,
                });
                match run.run(cancelled.clone(), RunProgress(tx.clone())).await {
                    InPlaceEnd::Finished => (started, Ok(())),
                    InPlaceEnd::Failed(error) => (started, Err(error)),
                    InPlaceEnd::Cancelled => {
                        crate::probe!(
                            class = "training.job.cancelled",
                            job = %id,
                            provider,
                            phase = "running",
                            ms = started.elapsed().as_millis() as u64,
                            "a running in-place training job was cancelled — the run \
                             stopped where it runs before this receipt"
                        );
                        tx.send_replace(TrainingStatus::Cancelled);
                        return;
                    }
                }
            };
            // finish moves the adapter and records its footprint: file I/O, and a footprint rename
            // may wait out a Windows sharing violation (fs_portable::replace), so off the runtime
            let wall_clock_ms = started.elapsed().as_millis() as u64;
            let artifact = match ended {
                Ok(()) => tokio::task::spawn_blocking(move || finish(wall_clock_ms))
                    .await
                    .unwrap_or_else(|e| Err(format!("the job's finish did not complete: {e}"))),
                Err(e) => Err(e),
            };
            match &artifact {
                Ok(artifact) => crate::probe!(
                    class = "training.job.finished",
                    job = %id,
                    provider,
                    ms = started.elapsed().as_millis() as u64,
                    model_id = artifact.model_id.as_str(),
                    trained_tokens = artifact.metrics.trained_tokens,
                    final_loss = artifact.metrics.final_loss.unwrap_or(f64::NAN), // unwrap_or: `finish` already refused a job with no finite loss; NAN here can only mean that guard moved
                    "a local training job finished with a measured learning receipt"
                ),
                Err(error) => crate::probe!(
                    class = "training.job.failed",
                    job = %id,
                    provider,
                    ms = started.elapsed().as_millis() as u64,
                    error = error.as_str(),
                    "a local training job failed after it started — the trainer's own \
                     account (its exit and stderr tail, or the run's error) is the error"
                ),
            }
            tx.send_replace(match artifact {
                Ok(artifact) => TrainingStatus::Completed { artifact },
                Err(error) => TrainingStatus::Failed { error },
            });
        });
        JobHandle {
            provider_id: self.provider.into(),
            provider_job_id: id.to_string(),
            local_id: id,
        }
    }
}

pub(super) fn job_dir_for(request: &TrainingJobRequest, local_id: Uuid) -> PathBuf {
    if let Some(dir) = &request.local_artifact_dir {
        return dir.join(local_id.to_string());
    }
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")); // Preserve the existing MLX current-directory job-root fallback when no home is available.
    home.join(".continuum/genome")
        .join(request.persona_name.replace(['/', ' '], "_"))
        .join(sanitize(&request.trait_kind))
        .join(local_id.to_string())
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// The learning rate a LoRA run takes when its request names none, for EVERY backend (the
/// engine, CUDA, MLX and the stand-in actor). It was two numbers that disagreed: 1e-5 here
/// (from #4322, never measured against working gradients; every gene before 2026-10-07
/// trained on broken ones) and 1e-4 in the actor. Measured on the 5090 2026-10-10 (job
/// ff186697, Kimi's 18 'code' examples, the exact walk): at 1e-5 train_loss went 2.7796 ->
/// 2.7733 over an epoch, -0.2%. The exact walk takes ONE AdamW step per window, so 18
/// examples x 3 epochs is 54 steps, and at 1e-5 the adapter barely moves. 1e-4 is the
/// conventional floor of the LoRA range at rank 8.
pub(super) const DEFAULT_LEARNING_RATE: f64 = 1e-4;

pub(super) fn default_schedule() -> ScheduleParams {
    ScheduleParams {
        epochs: 3,
        batch_size: 4,
        sequence_length: 2048,
        learning_rate: DEFAULT_LEARNING_RATE,
    }
}

pub(super) fn default_lora() -> LoRAHyperparams {
    LoRAHyperparams {
        rank: 8,
        alpha: 16,
        dropout: 0.0,
        target_modules: vec!["q_proj".into(), "v_proj".into()],
        top_layers: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (the 5090, 2026-10-10, job a7893e09): a calibration run (an unmeasured
    // shape) planned "all free" while serving was mid-handoff, serving then grew into it, and the
    // job waited forever for the plan-time number. A calibration admission takes what is free
    // AT ADMISSION, and its bind sees the bytes actually granted.
    #[tokio::test]
    async fn a_calibration_run_takes_what_is_free_at_admission_not_at_plan() {
        use crate::forge::training_admission::{wait_for_training_memory, wait_for_training_memory_bound, TrainingNeed};
        use crate::resources::{capacity::MockCapacitySource, DaemonConfig, ResourceDaemon, ResourceKind};
        use futures::FutureExt;
        let mib = 1024u64 * 1024;
        let daemon = ResourceDaemon::start(
            vec![Arc::new(MockCapacitySource::new(ResourceKind::Vram, 1024 * mib))],
            vec![],
            DaemonConfig::default(),
        );
        let gate = crate::modules::serving_daemon::LifecycleGate::unowned(true);
        let planned = daemon.available_for("trainer", ResourceKind::Vram);
        assert_eq!(planned, 1024 * mib, "test: everything is free when the run plans");
        // serving grows into the memory after the plan
        let _serving = wait_for_training_memory(daemon.clone(), &gate, "serving", 768 * mib, |_| {})
            .await
            .unwrap();
        let bound = Arc::new(std::sync::Mutex::new(None));
        let saw = bound.clone();
        let admitted = wait_for_training_memory_bound(daemon.clone(), &gate, "trainer", TrainingNeed::Calibrate { at_least: None }, |_| {}, move |granted| {
            *saw.lock().unwrap() = Some(granted);
            Ok(())
        })
        .now_or_never()
        .expect("a calibration admission does not wait for the plan-time number")
        .expect("test: admitted");
        assert_eq!(admitted.bytes(), 256 * mib, "it leases what is free now");
        assert_eq!(*bound.lock().unwrap(), Some(256 * mib), "and binds with the bytes it was granted");
        // what this catches (the 5090, 2026-10-11 14:08Z, job 61b782cd): a sliver under the engine's
        // smallest budget was granted, posted as memory_budget_mib=0 and refused unreturnably.
        // With less than 1 MiB free, a calibration admission waits instead of granting it.
        drop(admitted);
        let _rest = wait_for_training_memory(daemon.clone(), &gate, "serving-2", 256 * mib - 512 * 1024, |_| {})
            .await
            .unwrap();
        assert!(
            wait_for_training_memory_bound(daemon.clone(), &gate, "trainer", TrainingNeed::Calibrate { at_least: None }, |_| {}, |_| Ok(()))
                .now_or_never()
                .is_none(),
            "512 KiB free: the calibration waits, it is not granted"
        );
        // and a family that once measured a whole run waits for at least that much (Cormac on #4949)
        drop(_rest);
        assert!(
            wait_for_training_memory_bound(daemon.clone(), &gate, "trainer", TrainingNeed::Calibrate { at_least: Some(300 * mib) }, |_| {}, |_| Ok(()))
                .now_or_never()
                .is_none(),
            "256 MiB free under a 300 MiB measured floor: it waits"
        );
    }

    // Regression: capacity contention must wait without allocating, wake on
    // release, and cancellation must never turn a queued job into a trainer.
    #[tokio::test]
    async fn prepared_training_waits_for_capacity_and_remains_cancellable() {
        use crate::forge::training_admission::wait_for_training_memory;
        use crate::resources::{
            capacity::MockCapacitySource, DaemonConfig, ResourceDaemon, ResourceKind,
        };
        use futures::FutureExt;
        let daemon = ResourceDaemon::start(
            vec![Arc::new(MockCapacitySource::new(ResourceKind::Vram, 1024))],
            vec![],
            DaemonConfig::default(),
        );
        let gate = crate::modules::serving_daemon::LifecycleGate::unowned(true);
        let held = wait_for_training_memory(daemon.clone(), &gate, "serving", 1024, |_| {})
            .await
            .unwrap();
        let mut waiting = Box::pin(wait_for_training_memory(
            daemon.clone(),
            &gate,
            "trainer",
            512,
            |_| {},
        ));
        assert!(waiting.as_mut().now_or_never().is_none());
        drop(held);
        let granted = waiting.await.unwrap();

        // A preparation waiting on the shared admission reports WaitingForCapacity, and a
        // cancel while it waits ends it Cancelled without ever acquiring the memory.
        let jobs = NativeJobs::new("capacity-test");
        let id = Uuid::new_v4();
        let handle = jobs.prepare(id, move |progress| async move {
            let _guard = wait_for_training_memory(daemon, &gate, "trainer", 1024, |available| {
                progress.waiting_for_capacity(1024, available)
            })
            .await
            .map_err(FineTuningError::LocalTrainerFailed)?;
            panic!("cancelled preparation must never acquire capacity");
        });
        let mut status = jobs.slots.get(&handle.local_id).unwrap().status.clone();
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            while !matches!(
                *status.borrow_and_update(),
                TrainingStatus::WaitingForCapacity { .. }
            ) {
                status.changed().await.unwrap();
            }
        })
        .await
        .unwrap();
        assert!(matches!(
            jobs.poll(&handle).unwrap(),
            TrainingStatus::WaitingForCapacity {
                required_bytes: 1024,
                available_bytes: 512
            }
        ));
        jobs.cancel(&handle).unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            while !matches!(*status.borrow_and_update(), TrainingStatus::Cancelled) {
                status.changed().await.unwrap();
            }
        })
        .await
        .unwrap();
        drop(granted);
    }

    // what this catches: card aae8af55 — free memory read inside a serving window
    // admitted a trainer nobody called (the 9/16 class). On a fully free card: before
    // serving's first plan (the boot-empty window) admission waits; with the gate held
    // by a relaunch it waits; the gate's release alone (no governor edge) admits it; and
    // admission must NOT keep the gate for the run.
    #[tokio::test]
    async fn training_admission_waits_out_boot_and_serving_operations_and_never_keeps_the_gate() {
        use crate::forge::training_admission::wait_for_training_memory;
        use crate::modules::serving_daemon::LifecycleGate;
        use crate::resources::{
            capacity::MockCapacitySource, DaemonConfig, ResourceDaemon, ResourceKind,
        };
        use futures::FutureExt;
        let daemon = ResourceDaemon::start(
            vec![Arc::new(MockCapacitySource::new(ResourceKind::Vram, 1024))],
            vec![],
            DaemonConfig::default(),
        );
        let gate = LifecycleGate::unowned(false);
        let mut booting = Box::pin(wait_for_training_memory(
            daemon.clone(),
            &gate,
            "trainer",
            512,
            |_| {},
        ));
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
        assert!(
            booting.as_mut().now_or_never().is_none(),
            "nothing is admitted before serving's first plan on this core"
        );
        drop(booting);
        gate.settle();
        let relaunch = gate.hold_for_admission().ok().expect("an idle settled gate is free");
        let mut admission = Box::pin(wait_for_training_memory(
            daemon.clone(),
            &gate,
            "trainer",
            512,
            |_| {},
        ));
        assert!(admission.as_mut().now_or_never().is_none());
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
        assert!(
            admission.as_mut().now_or_never().is_none(),
            "a held gate refuses admission whatever the governor shows"
        );
        drop(relaunch);
        let lease = tokio::time::timeout(std::time::Duration::from_secs(5), admission)
            .await
            .expect("the gate's release admits without a governor edge")
            .unwrap();
        assert!(
            gate.hold_for_admission().is_ok(),
            "a granted admission releases the gate; the lease holds the memory"
        );
        drop(lease);
    }

    /// An in-place run that ends only when cancelled, and records that it did.
    struct UntilCancelled(Arc<std::sync::atomic::AtomicBool>);

    #[async_trait::async_trait]
    impl InPlaceRun for UntilCancelled {
        fn place(&self) -> String {
            "test-lane".into()
        }
        async fn run(self: Box<Self>, mut cancel: watch::Receiver<bool>, _progress: RunProgress) -> InPlaceEnd {
            while !*cancel.borrow_and_update() {
                if cancel.changed().await.is_err() {
                    break;
                }
            }
            self.0.store(true, std::sync::atomic::Ordering::SeqCst);
            InPlaceEnd::Cancelled
        }
    }

    // what this catches: cancellation before the run starts must not be lost; the handle's
    // provider is authenticated; and Cancelled is published only after the run has stopped
    // where it runs (the next admission would otherwise collide with it).
    #[tokio::test]
    async fn immediate_cancel_is_latched_and_foreign_handles_are_rejected() {
        for wait_for_spawn in [false, true] {
            let jobs = NativeJobs::new("native-test");
            let stopped = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let ran = stopped.clone();
            let handle = jobs.prepare(Uuid::new_v4(), move |_| async move {
                Ok(PreparedJob {
                    run: Box::new(UntilCancelled(ran)),
                    finish: Box::new(|_| panic!("a cancelled run must not publish an artifact")),
                })
            });
            let mut foreign = handle.clone();
            foreign.provider_id = "other-backend".into();
            assert!(jobs.poll(&foreign).is_err());
            assert!(jobs.cancel(&foreign).is_err());
            let mut status = jobs.slots.get(&handle.local_id).unwrap().status.clone();
            tokio::time::timeout(std::time::Duration::from_secs(10), async {
                if wait_for_spawn {
                    while !matches!(*status.borrow(), TrainingStatus::Running { .. }) {
                        status.changed().await.unwrap();
                    }
                }
                jobs.cancel(&handle).unwrap();
                loop {
                    if matches!(*status.borrow(), TrainingStatus::Cancelled) {
                        break;
                    }
                    status.changed().await.unwrap();
                }
            })
            .await
            .expect("a cancelled run was not stopped");
            if wait_for_spawn {
                assert!(stopped.load(std::sync::atomic::Ordering::SeqCst), "Cancelled before the run stopped");
            }
        }
    }
}
