//! Shared serving identity and restoration checks for explicit maintenance callers.
use super::*;
use continuum_cli_lifecycle::supervisor_install::maintenance::LegacySchedulerWindow;
use continuum_core::inference::engine_slots::MaintenanceEngines;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ServingExpectation {
    schema: u32,
    core_revision: String,
    model: String,
    native_identity: String,
    /// Existing serving catalog identities; not adapter-content attestation.
    adapters: Vec<String>,
    window: u32,
    lanes: u32,
    host_prompt_cache_mib: u32,
    vision: bool,
    vision_model: Option<String>,
}

impl ServingExpectation {
    fn validate(&self) -> Result<(), String> {
        if self.schema != 1
            || self.core_revision.is_empty()
            || self.model.is_empty()
            || self.window == 0
            || self.lanes == 0
            || self.native_identity.len() != 64
            || !self.native_identity.bytes().all(|b| b.is_ascii_hexdigit())
            || self.vision != self.vision_model.is_some()
        {
            return Err("invalid saved serving expectation; recovery evidence retained".into());
        }
        Ok(())
    }

    pub(super) fn saved(home: &Path, source: &Path) -> Result<Self, String> {
        // Boundary: caller-owned typed projection carried by the existing journal.
        let saved: Self =
            serde_json::from_value(LegacySchedulerWindow::recovery_expectation(home, source)?) // Disk boundary: decode caller-owned expectation from the durable maintenance journal.
                .map_err(|e| format!("saved serving expectation: {e}"))?;
        saved.validate()?;
        Ok(saved)
    }

    pub(super) async fn capture() -> Result<Self, String> {
        let core_before = running_core_build_sha("maintenance serving expectation")
            .await?
            .ok_or("core revision unavailable")?;
        let response =
            running_core_provenance("serving/status", "maintenance serving expectation").await?;
        let snapshot: continuum_core::inference::llama_server::ServingSnapshot =
            serde_json::from_value(response) // IPC boundary: decode the running core serving/status response.
                .map_err(|e| format!("serving snapshot: {e}"))?;
        if !snapshot.ready || snapshot.degraded_reason.is_some() {
            return Err("original serving readiness is unavailable; no maintenance begun".into());
        }
        let lane = continuum_core::inference::lane_registry::live_lane()
            .ok_or("original serving engine ownership is unavailable")?;
        if snapshot.base_url.trim_end_matches('/') != format!("{}/v1", lane.root_url())
            || snapshot.active_model.as_deref() != Some(lane.model.as_str())
        {
            return Err("serving snapshot and owned live engine disagree".into());
        }
        let client = reqwest::Client::builder()
            .timeout(PING_BUDGET)
            .build()
            .map_err(|e| e.to_string())?;
        client
            .get(format!("{}/health", lane.root_url()))
            .send()
            .await
            .map_err(|e| format!("live serving health: {e}"))?
            .error_for_status()
            .map_err(|e| format!("live serving health: {e}"))?;
        let props: Value = client
            .get(format!("{}/props", lane.root_url()))
            .send()
            .await
            .map_err(|e| format!("live serving properties: {e}"))?
            .error_for_status()
            .map_err(|e| format!("live serving properties: {e}"))?
            .json()
            .await
            .map_err(|e| e.to_string())?;
        if props
            .pointer("/default_generation_settings/n_ctx")
            .and_then(Value::as_u64)
            != Some(snapshot.served_context_window as u64)
            || props.get("total_slots").and_then(Value::as_u64) != Some(snapshot.lanes as u64)
            || continuum_core::inference::lane_process::pid_listening_on_port(lane.port)
                != Some(lane.pid)
        {
            return Err("live engine properties/ownership differ from serving snapshot".into());
        }
        let program = lane
            .engine_bin
            .clone()
            .ok_or("owned engine lacks executable provenance")?;
        let native_identity =
            tokio::task::spawn_blocking(move || MaintenanceEngines::verified_identity(&program))
                .await
                .map_err(|e| e.to_string())??;
        let response =
            running_core_provenance("serving/status", "maintenance serving recheck").await?;
        let after: continuum_core::inference::llama_server::ServingSnapshot =
            serde_json::from_value(response) // IPC boundary: decode the running core serving/status response.
                .map_err(|e| e.to_string())?;
        let lane_after = continuum_core::inference::lane_registry::live_lane()
            .ok_or("serving generation disappeared during observation")?;
        if lane_after.pid != lane.pid
            || lane_after.started_s != lane.started_s
            || after.active_model != snapshot.active_model
            || after.adapters != snapshot.adapters
            || after.served_context_window != snapshot.served_context_window
            || after.lanes != snapshot.lanes
            || after.host_prompt_cache_mib != snapshot.host_prompt_cache_mib
            || after.vision_ready != snapshot.vision_ready
            || after.vision_model != snapshot.vision_model
            || after.base_url != snapshot.base_url
            || !after.ready
            || after.degraded_reason.is_some()
            || running_core_build_sha("maintenance serving recheck")
                .await?
                .as_deref()
                != Some(core_before.as_str())
            || continuum_core::inference::lane_process::pid_listening_on_port(lane.port)
                != Some(lane.pid)
        {
            return Err(
                "serving changed during observation; no stable original expectation captured"
                    .into(),
            );
        }
        let mut adapters = snapshot.adapters;
        adapters.sort();
        let result = Self {
            schema: 1,
            core_revision: core_before,
            model: snapshot.active_model.ok_or("served model unavailable")?,
            native_identity,
            adapters,
            window: snapshot.served_context_window,
            lanes: snapshot.lanes,
            host_prompt_cache_mib: snapshot.host_prompt_cache_mib,
            vision: snapshot.vision_ready,
            vision_model: snapshot.vision_model,
        };
        result.validate()?;
        Ok(result)
    }

    pub(super) async fn verify(&self) -> Result<(), String> {
        let actual = Self::capture()
            .await
            .map_err(|e| format!("restoration acceptance pending: {e}"))?;
        if &actual != self {
            return Err("restoration acceptance pending: model/native revision/adapters/geometry/vision differs from saved original; no retry or configuration change performed".into());
        }
        Ok(())
    }

    pub(super) fn check_pinned(&self, window: &LegacySchedulerWindow) -> Result<(), String> {
        self.check_value(window.serving_expectation())
    }

    pub(super) fn check_value(&self, value: &Value) -> Result<(), String> {
        // Boundary: discovery must agree with the operation journal held under custody.
        let pinned: Self = serde_json::from_value(value.clone()) // Disk boundary: validate expectation decoded from the pinned maintenance journal.
            .map_err(|e| format!("pinned serving expectation: {e}"))?;
        pinned.validate()?;
        if &pinned != self {
            return Err("serving expectation changed before journal custody".into());
        }
        Ok(())
    }
}

pub(super) fn confirm_exited() -> Result<(), String> {
    core_process_evidence()
        .map_err(|e| e.to_string())?
        .ensure_offline(None)
        .map_err(|e| e.to_string())?;
    let records =
        continuum_core::inference::lane_registry::records_checked().map_err(|e| e.to_string())?;
    if records.iter().any(|record| pid_alive(record.pid as i32)) {
        return Err(
            "a recorded engine generation remains alive; maintenance stays disabled".into(),
        );
    }
    Ok(())
}

pub(super) async fn launch_restored_once(
    service: &PreparedCoreService,
    outgoing: &[i32],
) -> Result<(), String> {
    if core_is_up().await {
        return Ok(());
    }
    if PreparedCoreService::query().await?.state == "Running" {
        return Err("restoration acceptance pending: original task is starting; no second launch".into());
    }
    service.launch_windows_once(outgoing).await?;
    Ok(())
}
