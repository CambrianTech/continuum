//! Actual serving changes wake durable training recovery. The watch coalesces
//! notifications, never examples; the recovery owner rechecks its predicates.
use super::{TrainingTriggerModule, TrainingTriggerState};
use crate::inference::llama_server::ServingSnapshot;
use std::{future::Future, sync::Arc};
use tokio::sync::{watch, Mutex};

#[derive(Default)]
pub(super) struct ServingWake {
    receiver: Option<watch::Receiver<ServingSnapshot>>,
    task: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl TrainingTriggerModule {
    /// Wire the local process owner's actual state, never a proposed plan or a
    /// remote bus payload. Subscribe at registration, before the initial read.
    pub fn with_serving_changes(mut self, receiver: watch::Receiver<ServingSnapshot>) -> Self {
        self.serving_wake.receiver = Some(receiver);
        self
    }
}

impl ServingWake {
    pub(super) async fn start(&self, state: Arc<TrainingTriggerState>) {
        let Some(receiver) = self.receiver.clone() else {
            return;
        };
        let mut task = self.task.lock().await;
        if task.is_some() {
            return;
        }
        *task = Some(tokio::spawn(follow(receiver, move || {
            let state = state.clone();
            async move {
                if let Err(error) = state.recover_pass().await {
                    continuum_base::probe!(class = "training.serving_wake.failed", error = %error,
                        "serving changed but training recovery failed; durable work remains owned");
                }
            }
        })));
    }

    pub(super) async fn stop(&self) {
        let task = self.task.lock().await.take();
        if let Some(task) = task {
            task.abort();
            let _ = task.await;
        }
    }
}

async fn follow<F, Fut>(mut receiver: watch::Receiver<ServingSnapshot>, mut recover: F)
where
    F: FnMut() -> Fut,
    Fut: Future<Output = ()>,
{
    loop {
        // Mark observed BEFORE recovery: a transition during recovery remains
        // unseen and immediately causes another pass. No watch borrow crosses await.
        receiver.borrow_and_update();
        recover().await;
        if receiver.changed().await.is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn boot_and_changes_during_recovery_wake_without_a_timer() {
        let (sender, receiver) = watch::channel(ServingSnapshot::empty());
        let (entered, mut calls) = tokio::sync::mpsc::unbounded_channel();
        let release = Arc::new(tokio::sync::Semaphore::new(0));
        let gate = release.clone();
        let task = tokio::spawn(follow(receiver, move || {
            let entered = entered.clone();
            let gate = gate.clone();
            async move {
                entered.send(()).expect("test receiver remains open");
                gate.acquire()
                    .await
                    .expect("test semaphore remains open")
                    .forget();
            }
        }));
        calls
            .recv()
            .await
            .expect("boot evaluates already published state");
        sender.send_replace(ServingSnapshot::empty());
        sender.send_replace(ServingSnapshot::empty());
        release.add_permits(1);
        calls
            .recv()
            .await
            .expect("changes during recovery are not lost");
        assert!(
            calls.try_recv().is_err(),
            "one serial recovery, no task per notification"
        );
        drop(sender);
        release.add_permits(1);
        task.await.expect("closed owner stops its follower");
    }
}
