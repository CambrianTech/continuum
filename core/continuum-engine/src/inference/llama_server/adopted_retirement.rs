//! Retire a recorded adopted engine through its existing process identity owner.
use super::*;
use crate::inference::engine_residency::Liveness;

/// What retiring the recorded adopted engine means now ([`LlamaServerProcess::adopted_plan`]).
enum AdoptedPlan {
    Resume(crate::inference::engine_residency::EngineIncarnation),
    NoLane,
    Dead,
    Alive(crate::inference::engine_residency::EngineIncarnation),
}

impl LlamaServerProcess {
    /// What retiring the recorded adopted engine means now, from state alone: resume a
    /// retirement already signalled, nothing to retire, an engine already dead, or a live one.
    /// The refusals here are the replacement's own, so its pre-flight and its retirement make
    /// them from one place.
    fn adopted_plan(&self) -> Result<AdoptedPlan, LlamaServerError> {
        if let Some(incarnation) = *self.retiring_adopted.lock() {
            return Ok(AdoptedPlan::Resume(incarnation));
        }
        let Some(lane) = crate::inference::lane_registry::live_lane() else {
            // Absence grants no permission to kill a listener. Launch's port gate
            // will refuse any unrecorded occupant, including another llama-server.
            return Ok(AdoptedPlan::NoLane);
        };
        if lane.root_url() != self.root {
            return Err(LlamaServerError::RetirementRefused(RetirementRefusal::OtherEndpoint));
        }
        let incarnation = lane.incarnation();
        match incarnation.liveness() {
            Liveness::Dead => Ok(AdoptedPlan::Dead),
            Liveness::Unknown => Err(LlamaServerError::RetirementRefused(RetirementRefusal::IdentityUnverified)),
            Liveness::Alive => Ok(AdoptedPlan::Alive(incarnation)),
        }
    }

    /// Every refusal retiring the adopted engine can make before its drain (the replacement's
    /// pre-flight, `EndpointSlots::drain_for_replacement`). The checkpoint repeats its own checks
    /// under the guard. A never probed endpoint discovers its pool here, before anything is
    /// condemned (the M5 2026-11-04 14:56Z: growth refused 5 s after adoption, before any request).
    pub(super) async fn adopted_retirement_preflight(&self) -> Result<(), LlamaServerError> {
        match self.adopted_plan()? {
            AdoptedPlan::Alive(incarnation) => {
                let directory = crate::inference::slots::directory();
                if directory.endpoint(&self.root).paging_recovery_required() {
                    return Ok(());
                }
                // Bounded like every lifecycle probe: a wedged engine refuses, never stalls the launch.
                let discovery = directory.adopted_checkpoint_pool(&self.client, &self.root, incarnation);
                let checked = match tokio::time::timeout(PROBE_TIMEOUT, discovery).await {
                    Ok(checked) => checked,
                    Err(_) => Err(crate::inference::slots::AdoptedCheckpointRefusal::UnknownPool(
                        crate::inference::slots::PoolDiscoveryMiss::TimedOut,
                    )),
                };
                checked
                    .map(drop)
                    .map_err(|refusal| LlamaServerError::RetirementRefused(refusal.into()))
            }
            AdoptedPlan::Resume(_) | AdoptedPlan::NoLane | AdoptedPlan::Dead => Ok(()),
        }
    }

    pub(super) async fn retire_adopted(
        &self,
        transition: &mut crate::inference::slots::EndpointTransition,
        current: &ServingLifecycleCheck<'_>,
    ) -> Result<(), LlamaServerError> {
        use crate::inference::slots::DiscardReason;
        let endpoint = crate::inference::slots::directory().endpoint(&self.root);
        let incarnation = match self.adopted_plan()? {
            // Resume observation, never checkpoint or reopen a process already
            // signalled by an earlier cancelled/timed-out launch.
            AdoptedPlan::Resume(incarnation) => {
                return self.await_adopted_exit(incarnation, Duration::from_secs(10)).await
            }
            AdoptedPlan::NoLane => return Ok(()),
            AdoptedPlan::Dead => {
                // Its residents' KV died with it; their next turns name this (card 6b28a445).
                endpoint.pending_discard(DiscardReason::AdoptedEngineDead).commit();
                return Ok(());
            }
            AdoptedPlan::Alive(incarnation) => incarnation,
        };
        // Quarantined paging cannot save: its residents are captured now and recorded only
        // when the engine is seen dead, so a superseded, refused or still-waiting retirement
        // never reports a loss (card 6b28a445).
        let unsaved = if endpoint.paging_recovery_required() {
            Some(endpoint.pending_discard(DiscardReason::PagingUncertain))
        } else {
            transition.checkpoint_adopted_before_replacement(
                &self.client, &self.root, incarnation, current,
            ).await.map_err(|refusal| LlamaServerError::RetirementRefused(refusal.into()))?;
            None
        };
        if !current() {
            return Err(LlamaServerError::Superseded);
        }
        // A completed checkpoint is not authority to signal a reused PID.
        match incarnation.liveness() {
            Liveness::Dead => {
                if let Some(unsaved) = unsaved {
                    unsaved.because(DiscardReason::AdoptedEngineDead).commit();
                }
                return Ok(());
            }
            Liveness::Unknown => return Err(LlamaServerError::RetirementRefused(RetirementRefusal::IdentityUnverifiedAfterCheckpoint)),
            Liveness::Alive => {}
        }
        continuum_process::signal_if_same(
            crate::system_resources::process_tree::ProcessIdentity {
                pid: incarnation.pid,
                start_time: incarnation.started_s,
            },
        ).map_err(|detail| LlamaServerError::RetirementRefused(RetirementRefusal::Signal(detail)))?;
        // No await between signalling and retaining custody. Cancellation of the
        // wait cannot turn this predecessor back into a serving candidate. A signal is
        // not a death: the residents' loss is recorded when the exit is observed.
        *self.retiring_adopted.lock() = Some(incarnation);
        *self.retiring_discard.lock() = unsaved;
        self.await_adopted_exit(incarnation, Duration::from_secs(10)).await
    }

    async fn await_adopted_exit(
        &self,
        incarnation: crate::inference::engine_residency::EngineIncarnation,
        budget: Duration,
    ) -> Result<(), LlamaServerError> {
        // A free port is not proof that the engine released its memory. Retain
        // the transition until this exact process is positively gone: its EXIT EVENT, the moment
        // the kernel delivers it, never a 100 ms liveness poll (card b5b9bef3). `budget` is only
        // the watchdog that reports an engine that never went.
        let wait = async {
            match incarnation.liveness() {
                Liveness::Dead => return Ok(()),
                Liveness::Unknown => return Err(LlamaServerError::RetirementPending("adopted engine exit is unverified".into())),
                Liveness::Alive => {}
            }
            match continuum_host::process_exit::exited(incarnation.pid).await {
                // The exit of the pid this identity named; its identity re-read confirms it.
                continuum_host::process_exit::ExitObserved::Exited => match incarnation.liveness() {
                    Liveness::Dead => Ok(()),
                    Liveness::Alive | Liveness::Unknown => Err(LlamaServerError::RetirementPending(
                        "adopted engine pid exited but its identity still answers".into(),
                    )),
                },
                continuum_host::process_exit::ExitObserved::Unobservable(why) => Err(
                    LlamaServerError::RetirementPending(format!("adopted engine exit is unobservable: {why}")),
                ),
            }
        };
        tokio::time::timeout(budget, wait).await
            .map_err(|_| LlamaServerError::RetirementPending(format!(
                "adopted engine pid {} started {} exit timed out; endpoint remains suspended. \
                 Next serving reconciliation resumes exact-process exit observation, then launches \
                 and verifies readiness before reopening admission", incarnation.pid, incarnation.started_s
            )))??;
        *self.retiring_adopted.lock() = None;
        // Its death is observed: the residents it held unsaved are now lost.
        let discarded = self.retiring_discard.lock().take();
        if let Some(discarded) = discarded {
            discarded.commit();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference::engine_residency::{EngineIncarnation, ProcessProbe, probe_process};

    // Exercise the wait against a real isolated process. Cancellation and timeout
    // retain custody; only its observed exit permits the next launch to proceed. The
    // residents it held unsaved are recorded as lost at that observed exit, never while a
    // cancelled or timed-out wait still holds custody (card 6b28a445).
    #[tokio::test]
    async fn adopted_exit_wait_retains_custody_until_observed_exit() -> Result<(), Box<dyn std::error::Error>> {
        const CHILD: &str = "CONTINUUM_ADOPTED_EXIT_FIXTURE";
        if std::env::var_os(CHILD).is_some() {
            use std::io::Read;
            let _ = std::io::stdin().read(&mut [0]);
            return Ok(());
        }
        let mut child = std::process::Command::new(std::env::current_exe()?)
            .args(["--exact", "inference::llama_server::adopted_retirement::tests::adopted_exit_wait_retains_custody_until_observed_exit"])
            .env(CHILD, "1")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .spawn()?;
        let ProcessProbe::Present(started_s) = probe_process(child.id()) else {
            drop(child.stdin.take());
            let _ = child.wait();
            return Err("fixture process identity unavailable".into());
        };
        let incarnation = EngineIncarnation { pid: child.id(), started_s, port: 0 };
        let root = format!("http://exit-fixture-{}", uuid::Uuid::new_v4());
        let process = LlamaServerProcess::with_root(root.clone());
        *process.retiring_adopted.lock() = Some(incarnation);
        let endpoint = crate::inference::slots::directory().endpoint(&root);
        let resident = crate::inference::slots::PageKey::mind(uuid::Uuid::new_v4()).ok_or("non-nil id")?;
        let lost = crate::inference::slots::DiscardReason::PagingUncertain;
        *process.retiring_discard.lock() =
            Some(crate::inference::slots::PendingDiscard::for_test(endpoint.clone(), vec![resident], lost));
        {
            use std::future::Future;
            let wait = process.await_adopted_exit(incarnation, Duration::from_secs(10));
            tokio::pin!(wait);
            std::future::poll_fn(|cx| {
                assert!(wait.as_mut().poll(cx).is_pending());
                std::task::Poll::Ready(())
            }).await;
        }
        assert_eq!(*process.retiring_adopted.lock(), Some(incarnation));
        let error = process.await_adopted_exit(incarnation, Duration::ZERO).await;
        let Err(error) = error else {
            drop(child.stdin.take());
            let _ = child.wait();
            return Err("a live predecessor unexpectedly retired".into());
        };
        assert!(matches!(error, LlamaServerError::RetirementPending(_)));
        let closed = crate::inference::slots::directory().endpoint(&format!("{root}-admission"));
        let transition = closed.transition_if(&|| true).await.map_err(|()| "transition refused")?;
        super::super::launch::handle_retirement_failure(&transition, &error);
        drop(transition);
        assert!(!closed.is_ready(), "post-signal uncertainty must keep admission closed");
        assert!(closed.admit().await.is_err());
        let untouched = crate::inference::slots::directory().endpoint(&format!("{root}-untouched"));
        let transition = untouched.transition_if(&|| true).await.map_err(|()| "transition refused")?;
        super::super::launch::handle_retirement_failure(&transition,
            &LlamaServerError::RetirementRefused(RetirementRefusal::Checkpoint(
                crate::inference::slots::AdoptedCheckpointRefusal::UnknownPool(
                    crate::inference::slots::PoolDiscoveryMiss::NoGeometry,
                ),
            )));
        drop(transition);
        assert!(untouched.admit().await.is_ok(), "pre-signal refusal still hands admission back");
        assert!(!super::super::engine_install::roll_back_engine_after_failed_launch(&error),
            "failure to retire the predecessor cannot demote the candidate engine");
        assert_eq!(*process.retiring_adopted.lock(), Some(incarnation));
        assert_eq!(endpoint.admit().await?.take_relaunch_discard(&resident), None,
            "no loss is recorded while the wait still holds custody");
        drop(child.stdin.take());
        child.wait()?;
        process.await_adopted_exit(incarnation, Duration::from_secs(1)).await?;
        assert_eq!(*process.retiring_adopted.lock(), None);
        assert_eq!(endpoint.admit().await?.take_relaunch_discard(&resident), Some(lost),
            "the observed exit records the loss");
        Ok(())
    }
}
