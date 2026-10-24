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
            return Err(LlamaServerError::Spawn("recorded live lane belongs to another endpoint".into()));
        }
        let incarnation = lane.incarnation();
        let endpoint = crate::inference::slots::directory().endpoint(&self.root);
        match incarnation.liveness() {
            Liveness::Dead => {
                // Its residents' KV died with it; their next turns name this (card 6b28a445).
                endpoint.record_relaunch_discard(crate::inference::slots::DiscardReason::AdoptedEngineDead);
                return Ok(());
            }
            Liveness::Unknown => return Err(LlamaServerError::Spawn("adopted engine identity is unverified; refusing retirement".into())),
            Liveness::Alive => {}
        }
        if endpoint.paging_recovery_required() {
            endpoint.record_relaunch_discard(crate::inference::slots::DiscardReason::PagingUncertain);
        } else {
            transition.checkpoint_adopted_before_replacement(
                &self.client, &self.root, incarnation, current,
            ).await.map_err(LlamaServerError::Spawn)?;
        }
        if !current() {
            return Err(LlamaServerError::Superseded);
        }
        // A completed checkpoint is not authority to signal a reused PID.
        match incarnation.liveness() {
            Liveness::Dead => return Ok(()),
            Liveness::Unknown => return Err(LlamaServerError::Spawn("adopted engine identity became unverified after checkpoint".into())),
            Liveness::Alive => {}
        }
        crate::inference::lane_process::signal_if_same(
            crate::system_resources::process_tree::ProcessIdentity {
                pid: incarnation.pid,
                start_time: incarnation.started_s,
            },
        ).map_err(LlamaServerError::Spawn)?;
        // No await between signalling and retaining custody. Cancellation of the
        // wait cannot turn this predecessor back into a serving candidate.
        *self.retiring_adopted.lock() = Some(incarnation);
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
                    Liveness::Unknown => return Err(LlamaServerError::Spawn("adopted engine exit is unverified".into())),
                    Liveness::Alive => tokio::time::sleep(Duration::from_millis(100)).await,
                }
            }
        };
        tokio::time::timeout(budget, wait).await
            .map_err(|_| LlamaServerError::Spawn(format!(
                "adopted engine pid {} started {} exit timed out; endpoint remains suspended. \
                 Next serving reconciliation resumes exact-process exit observation, then launches \
                 and verifies readiness before reopening admission", incarnation.pid, incarnation.started_s
            )))??;
        *self.retiring_adopted.lock() = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference::engine_residency::{EngineIncarnation, ProcessProbe, probe_process};

    // Exercise the wait against a real isolated process. Cancellation and timeout
    // retain custody; only its observed exit permits the next launch to proceed.
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
        let process = LlamaServerProcess::with_root(format!("http://exit-fixture-{}", uuid::Uuid::new_v4()));
        *process.retiring_adopted.lock() = Some(incarnation);
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
        assert!(process.await_adopted_exit(incarnation, Duration::ZERO).await.is_err());
        assert_eq!(*process.retiring_adopted.lock(), Some(incarnation));
        drop(child.stdin.take());
        child.wait()?;
        process.await_adopted_exit(incarnation, Duration::from_secs(1)).await?;
        assert_eq!(*process.retiring_adopted.lock(), None);
        Ok(())
    }
}
