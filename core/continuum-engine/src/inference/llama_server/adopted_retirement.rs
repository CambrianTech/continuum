//! Retire a recorded adopted engine through its existing process identity owner.
use super::*;
use crate::inference::engine_residency::Liveness;

impl LlamaServerProcess {
    pub(super) async fn retire_adopted(
        &self,
        transition: &mut crate::inference::slots::EndpointTransition,
        current: &ServingLifecycleCheck<'_>,
    ) -> Result<(), LlamaServerError> {
        let Some(lane) = crate::inference::lane_registry::live_lane() else {
            // Absence grants no permission to kill a listener. Launch's port gate
            // will refuse any unrecorded occupant, including another llama-server.
            return Ok(());
        };
        if lane.root_url() != self.root {
            return Err(LlamaServerError::Spawn("recorded live lane belongs to another endpoint".into()));
        }
        let incarnation = lane.incarnation();
        match incarnation.liveness() {
            Liveness::Dead => return Ok(()),
            Liveness::Unknown => return Err(LlamaServerError::Spawn("adopted engine identity is unverified; refusing retirement".into())),
            Liveness::Alive => {}
        }
        let endpoint = crate::inference::slots::directory().endpoint(&self.root);
        if !endpoint.paging_recovery_required() {
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
        tokio::time::timeout(Duration::from_secs(10), wait).await
            .map_err(|_| LlamaServerError::Spawn("adopted engine exit timed out; endpoint stays suspended".into()))?
    }
}
