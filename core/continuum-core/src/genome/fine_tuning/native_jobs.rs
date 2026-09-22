//! Shared process ownership for native trainers. Backends prepare inputs and
//! interpret their artifacts; handles, cancellation and terminal state live here.
use super::{FineTuningError, JobHandle, TrainingArtifact, TrainingStatus};
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

pub(super) struct PreparedJob {
    pub command: tokio::process::Command,
    pub output: PathBuf,
    pub parser: Option<LossParser>,
    pub finish: Box<dyn FnOnce(u64) -> Result<TrainingArtifact, String> + Send>,
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

pub(super) type LossParser = fn(&str) -> Option<(u64, &'static str, f64)>;

impl NativeJobs {
    pub fn new(provider: &'static str) -> Self {
        Self {
            provider,
            slots: DashMap::new(),
        }
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.slots.len()
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

    pub fn launch(
        &self,
        id: Uuid,
        command: tokio::process::Command,
        output: PathBuf,
        parser: Option<LossParser>,
        finish: impl FnOnce(u64) -> Result<TrainingArtifact, String> + Send + 'static,
    ) -> Result<JobHandle, FineTuningError> {
        Ok(self.prepare(id, move |_| async move {
            Ok(PreparedJob {
                command,
                output,
                parser,
                finish: Box::new(finish),
            })
        }))
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
        tokio::spawn(async move {
            let prepare = prepare(PreparationProgress(tx.clone()));
            let prepared = tokio::select! {
                biased;
                _ = cancelled.changed() => {
                    tx.send_replace(TrainingStatus::Cancelled);
                    return;
                }
                result = prepare => result,
            };
            let PreparedJob {
                mut command,
                output,
                parser,
                finish,
            } = match prepared {
                Ok(job) => job,
                Err(error) => {
                    tx.send_replace(TrainingStatus::Failed {
                        error: error.to_string(),
                    });
                    return;
                }
            };
            command
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .kill_on_drop(true);
            #[cfg(windows)]
            command.creation_flags(0x08000000);
            let mut child = match command.spawn() {
                Ok(child) => child,
                Err(error) => {
                    tx.send_replace(TrainingStatus::Failed {
                        error: error.to_string(),
                    });
                    return;
                }
            };
            let stdout = child.stdout.take();
            let stderr = child.stderr.take();
            let started = Instant::now();
            tx.send_replace(TrainingStatus::Running {
                progress_pct: 0.0,
                current_epoch: 0,
            });
            let tail = Arc::new(std::sync::Mutex::new(std::collections::VecDeque::new()));
            let mut drains = Vec::new();
            if let Some(pipe) = stdout {
                drains.push(tokio::spawn(drain(pipe, output.clone(), parser, None)));
            }
            if let Some(pipe) = stderr {
                drains.push(tokio::spawn(drain(
                    pipe,
                    output,
                    parser,
                    Some(tail.clone()),
                )));
            }
            let exit = tokio::select! {
                biased;
                _ = cancelled.changed() => {
                    // Kill AND reap before announcing cancellation or releasing resources.
                    let result = child.kill().await;
                    for drain in drains { let _ = drain.await; }
                    tx.send_replace(match result {
                        Ok(()) => TrainingStatus::Cancelled,
                        Err(e) => TrainingStatus::Failed { error: format!("cancel trainer: {e}") },
                    });
                    return;
                }
                result = child.wait() => result,
            };
            let mut drain_error = None;
            for drain in drains {
                match drain.await {
                    Ok(Ok(())) => {}
                    other => drain_error = Some(format!("trainer diagnostics failed: {other:?}")),
                }
            }
            let artifact = match exit {
                Ok(exit) if exit.success() => match drain_error {
                    Some(error) => Err(error),
                    None => finish(started.elapsed().as_millis() as u64),
                },
                other => Err(format!(
                    "trainer exit {other:?}: {}",
                    tail.lock()
                        .map(|t| t.iter().cloned().collect::<Vec<_>>().join("\n"))
                        .unwrap_or_else(|_| "stderr unavailable".into()) // Failed diagnostic reads report absence; they never change job success.
                )),
            };
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

async fn drain(
    pipe: impl tokio::io::AsyncRead + Unpin,
    directory: PathBuf,
    parser: Option<LossParser>,
    tail: Option<Arc<std::sync::Mutex<std::collections::VecDeque<String>>>>,
) -> Result<(), std::io::Error> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
    let mut log = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(directory.join("trainer.log"))
        .await?;
    let mut losses = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(directory.join("loss.jsonl"))
        .await?;
    let mut lines = tokio::io::BufReader::new(pipe).lines();
    while let Some(line) = lines.next_line().await? {
        log.write_all(format!("{line}\n").as_bytes()).await?;
        if let Some((iter, kind, loss)) = parser.and_then(|parse| parse(&line)) {
            let row = serde_json::json!({"iter":iter,"kind":kind,"loss":loss,
                "atMs":chrono::Utc::now().timestamp_millis()});
            losses.write_all(format!("{row}\n").as_bytes()).await?;
        }
        if let Some(tail) = &tail {
            if let Ok(mut tail) = tail.lock() {
                tail.push_back(line);
                // Diagnostic tail only, never a limit on training inputs or results.
                if tail.len() > 40 {
                    tail.pop_front();
                }
            }
        }
    }
    Ok(())
}

use super::{LoRAHyperparams, ScheduleParams, TrainingJobRequest};
/// `~/.continuum/genome/<persona>/<trait_kind>/<job_uuid>/` — honors an
/// explicit `local_artifact_dir` override when the caller set one.
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

pub(super) fn default_schedule() -> ScheduleParams {
    ScheduleParams {
        epochs: 3,
        batch_size: 4,
        sequence_length: 2048,
        learning_rate: 1e-5,
    }
}

pub(super) fn default_lora() -> LoRAHyperparams {
    LoRAHyperparams {
        rank: 8,
        alpha: 16,
        dropout: 0.0,
        target_modules: vec!["q_proj".into(), "v_proj".into()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let held = wait_for_training_memory(daemon.clone(), "serving", 1024, |_| {})
            .await
            .unwrap();
        let mut waiting = Box::pin(wait_for_training_memory(
            daemon.clone(),
            "trainer",
            512,
            |_| {},
        ));
        assert!(waiting.as_mut().now_or_never().is_none());
        drop(held);
        let granted = waiting.await.unwrap();

        let jobs = NativeJobs::new("capacity-test");
        let handle = jobs.prepare(Uuid::new_v4(), |progress| async move {
            let _guard = wait_for_training_memory(daemon, "blocked-trainer", 1024, |available| {
                progress.waiting_for_capacity(1024, available);
            })
            .await
            .map_err(FineTuningError::Transient)?;
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

    #[test]
    fn native_job_child_fixture() {
        if std::env::var_os("CONTINUUM_NATIVE_JOB_CHILD").is_some() {
            // The parent cancels this child. No ambient env mutation in tests.
            std::thread::sleep(std::time::Duration::from_secs(60));
        }
    }

    // what this catches: cancellation before the watcher runs must not be lost;
    // the handle's provider is authenticated and the child is reaped before terminal.
    #[tokio::test]
    async fn immediate_cancel_is_latched_and_foreign_handles_are_rejected() {
        for wait_for_spawn in [false, true] {
            let jobs = NativeJobs::new("native-test");
            let directory = tempfile::tempdir().unwrap();
            let mut command = tokio::process::Command::new(std::env::current_exe().unwrap());
            command
                .args([
                    "--exact",
                    "genome::fine_tuning::native_jobs::tests::native_job_child_fixture",
                ])
                .env("CONTINUUM_NATIVE_JOB_CHILD", "1");
            let handle = jobs
                .launch(
                    Uuid::new_v4(),
                    command,
                    directory.path().into(),
                    None,
                    |_| panic!("cancelled trainer must not publish an artifact"),
                )
                .unwrap();
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
            .expect("cancelled child was not reaped");
        }
    }
}
