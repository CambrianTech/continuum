//! Retire a recorded adopted engine through its existing process identity owner.
use super::*;
use crate::inference::engine_residency::Liveness;

impl LlamaServerProcess {
    pub(super) async fn retire_adopted(
        &self,
        transition: &mut crate::inference::slots::EndpointTransition,
        current: &ServingLifecycleCheck<'_>,
    ) -> Result<(), LlamaServerError> {
        let pending = *self.retiring_adopted.lock();
        if let Some(incarnation) = pending {
            // Resume observation, never checkpoint or reopen a process already
            // signalled by an earlier cancelled/timed-out launch.
            return self.await_adopted_exit(incarnation, Duration::from_secs(10)).await;
        }
        let Some(lane) = crate::inference::lane_registry::live_lane() else {
            // Absence grants no permission to kill a listener. Launch's port gate
            // will refuse any unrecorded occupant, including another llama-server.
            return Ok(());
        };
        if lane.root_url() != self.root {
            return Err(LlamaServerError::RetirementRefused("recorded live lane belongs to another endpoint".into()));
        }
        let incarnation = lane.incarnation();
        use crate::inference::slots::DiscardReason;
        let endpoint = crate::inference::slots::directory().endpoint(&self.root);
        match incarnation.liveness() {
            Liveness::Dead => {
                // Its residents' KV died with it; their next turns name this (card 6b28a445).
                endpoint.pending_discard(DiscardReason::AdoptedEngineDead).commit();
                return Ok(());
            }
            Liveness::Unknown => return Err(LlamaServerError::RetirementRefused("adopted engine identity is unverified".into())),
            Liveness::Alive => {}
        }
        // Quarantined paging cannot save: its residents are captured now and recorded only
        // when the engine is seen dead, so a superseded, refused or still-waiting retirement
        // never reports a loss (card 6b28a445).
        let unsaved = if endpoint.paging_recovery_required() {
            Some(endpoint.pending_discard(DiscardReason::PagingUncertain))
        } else {
            transition.checkpoint_adopted_before_replacement(
                &self.client, &self.root, incarnation, current,
            ).await.map_err(LlamaServerError::RetirementRefused)?;
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
            Liveness::Unknown => return Err(LlamaServerError::RetirementRefused("adopted engine identity became unverified after checkpoint".into())),
            Liveness::Alive => {}
        }
        continuum_process::signal_if_same(
            crate::system_resources::process_tree::ProcessIdentity {
                pid: incarnation.pid,
                start_time: incarnation.started_s,
            },
        ).map_err(LlamaServerError::RetirementRefused)?;
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
        // the transition until this exact process is positively gone.
        let wait = async {
            loop {
                match incarnation.liveness() {
                    Liveness::Dead => return Ok(()),
                    Liveness::Unknown => return Err(LlamaServerError::RetirementPending("adopted engine exit is unverified".into())),
                    Liveness::Alive => tokio::time::sleep(Duration::from_millis(100)).await,
                }
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
            &LlamaServerError::RetirementRefused("checkpoint refused before signal".into()));
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
