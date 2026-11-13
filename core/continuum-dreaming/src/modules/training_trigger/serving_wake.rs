//! Actual serving changes wake durable training recovery. The watch coalesces
//! notifications, never examples; the recovery owner rechecks its predicates.
use super::{TrainingTriggerModule, TrainingTriggerState};
use crate::inference::llama_server::ServingSnapshot;
use std::{
    future::Future,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tokio::sync::{watch, Mutex, Notify};

#[derive(Default)]
pub(super) struct ServingWake {
    receiver: Option<watch::Receiver<ServingSnapshot>>,
    task: Mutex<Option<tokio::task::JoinHandle<()>>>,
    recovery_requested: Arc<Notify>,
    alive: Arc<AtomicBool>,
}

struct Alive(Arc<AtomicBool>);

impl Alive {
    fn new(flag: Arc<AtomicBool>) -> Self {
        flag.store(true, Ordering::Release);
        Self(flag)
    }
}

impl Drop for Alive {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
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
    pub(super) fn request_recovery(&self) -> bool {
        if !self.alive.load(Ordering::Acquire) {
            return false;
        }
        self.recovery_requested.notify_one();
        true
    }

    pub(super) async fn start(&self, state: Arc<TrainingTriggerState>) {
        let Some(receiver) = self.receiver.clone() else {
            return;
        };
        let mut task = self.task.lock().await;
        if task.is_some() {
            return;
        }
        *task = Some(tokio::spawn(follow(
            receiver,
            self.recovery_requested.clone(),
            Alive::new(self.alive.clone()),
            move || {
                let state = state.clone();
                async move {
                    if let Err(error) = state.recover_pass().await {
                        continuum_base::probe!(class = "training.serving_wake.failed", error = %error,
                        "serving changed but training recovery failed; durable work remains owned");
                    }
                }
            },
        )));
    }

    pub(super) async fn stop(&self) {
        let task = self.task.lock().await.take();
        if let Some(task) = task {
            task.abort();
            let _ = task.await;
        }
    }
}

async fn follow<F, Fut>(
    mut receiver: watch::Receiver<ServingSnapshot>,
    requested: Arc<Notify>,
    _alive: Alive,
    mut recover: F,
) where
    F: FnMut() -> Fut,
    Fut: Future<Output = ()>,
{
    loop {
        // Mark observed BEFORE recovery: a transition during recovery remains
        // unseen and immediately causes another pass. No watch borrow crosses await.
        receiver.borrow_and_update();
        recover().await;
        tokio::select! {
            changed = receiver.changed() => if changed.is_err() { return; },
            _ = requested.notified() => {},
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
        let requested = Arc::new(Notify::new());
        let alive = Arc::new(AtomicBool::new(false));
        let task = tokio::spawn(follow(
            receiver,
            requested.clone(),
            Alive::new(alive.clone()),
            move || {
                let entered = entered.clone();
                let gate = gate.clone();
                async move {
                    entered.send(()).expect("test receiver remains open");
                    gate.acquire()
                        .await
                        .expect("test semaphore remains open")
                        .forget();
                }
            },
        ));
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
        requested.notify_one();
        release.add_permits(1);
        calls
            .recv()
            .await
            .expect("recovery tick queues behind the running pass");
        drop(sender);
        release.add_permits(1);
        task.await.expect("closed owner stops its follower");
        assert!(!alive.load(Ordering::Acquire));
    }

    #[tokio::test]
    async fn ticks_do_not_handoff_to_closed_or_aborted_followers() {
        for abort in [false, true] {
            let (sender, receiver) = watch::channel(ServingSnapshot::empty());
            let wake = ServingWake {
                receiver: Some(receiver.clone()),
                ..Default::default()
            };
            assert!(
                !wake.request_recovery(),
                "before start the tick owns recovery"
            );
            let task = tokio::spawn(follow(
                receiver,
                wake.recovery_requested.clone(),
                Alive::new(wake.alive.clone()),
                || async {},
            ));
            assert!(wake.request_recovery(), "live owner accepts recovery");
            if abort {
                task.abort();
            }
            drop(sender);
            let _ = task.await;
            assert!(
                !wake.request_recovery(),
                "tick must recover after owner exits"
            );
        }
    }
}
