//! Foreground composition of the existing placement, scheduler and shutdown owners.
//! No background cleanup, installer retry or automatic legacy-reader assumption.
use super::*;
use continuum_cli_lifecycle::supervisor_install::maintenance::{
    LegacyImageCustody, LegacyRestoration, LegacySchedulerWindow,
};
use continuum_core::inference::engine_slots::MaintenanceEngines;
use continuum_core::model_registry::artifacts::expert_placement::{
    self as placement, ContainerMigrationCustody, MigrationStage,
};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ServingExpectation {
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

    fn saved(home: &Path, source: &Path) -> Result<Self, String> {
        // Boundary: caller-owned typed projection carried by the existing journal.
        let saved: Self =
            serde_json::from_value(LegacySchedulerWindow::recovery_expectation(home, source)?) // Disk boundary: decode caller-owned expectation from the durable maintenance journal.
                .map_err(|e| format!("saved serving expectation: {e}"))?;
        saved.validate()?;
        Ok(saved)
    }

    async fn capture() -> Result<Self, String> {
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

    async fn verify(&self) -> Result<(), String> {
        let actual = Self::capture()
            .await
            .map_err(|e| format!("restoration acceptance pending: {e}"))?;
        if &actual != self {
            return Err("restoration acceptance pending: model/native revision/adapters/geometry/vision differs from saved original; no retry or configuration change performed".into());
        }
        Ok(())
    }

    fn check_pinned(&self, window: &LegacySchedulerWindow) -> Result<(), String> {
        self.check_value(window.serving_expectation())
    }

    fn check_value(&self, value: &Value) -> Result<(), String> {
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

struct Custody {
    images: LegacyImageCustody,
    engines: MaintenanceEngines,
    source: PathBuf,
    runtime: tokio::runtime::Handle,
}

fn confirm_exited() -> Result<(), String> {
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

fn confirm_images_exited(images: &LegacyImageCustody) -> Result<(), String> {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
    let held: std::collections::HashSet<PathBuf> =
        images.held_image_paths().map(Path::to_path_buf).collect();
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet),
    );
    if !system
        .processes()
        .contains_key(&sysinfo::Pid::from_u32(std::process::id()))
    {
        return Err("process census is unavailable after image exclusion".into());
    }
    for process in system.processes().values() {
        if process.pid().as_u32() == std::process::id() {
            continue;
        }
        let possibly_managed = process_matches_fragment(process.name(), process.exe(), "continuum")
            || process_matches_fragment(process.name(), process.exe(), "llama-server");
        let actual = process.exe().map(Path::canonicalize).transpose();
        match actual {
            Ok(Some(path)) if held.contains(&path) => {
                return Err(format!(
                    "managed image PID {} still has an existing mapping after launch exclusion",
                    process.pid()
                ))
            }
            Ok(Some(_)) => {} // Outside the managed closure: explicit operator window.
            _ if possibly_managed => {
                return Err(format!(
                    "cannot identify possible managed image PID {}; absence is unproven",
                    process.pid()
                ))
            }
            _ => {}
        }
    }
    Ok(())
}

impl ContainerMigrationCustody for Custody {
    fn revalidate(&self, root: &Path, _stage: MigrationStage) -> Result<(), String> {
        if root != self.source {
            return Err("maintenance source differs from bound preparation".into());
        }
        self.runtime.block_on(self.images.revalidate())?;
        self.engines.revalidate()?;
        confirm_exited()?;
        confirm_images_exited(&self.images)
    }
}

async fn launch_restored_once(
    service: &PreparedCoreService,
    outgoing: &[i32],
) -> Result<(), String> {
    if core_is_up().await {
        return Ok(());
    }
    if PreparedCoreService::query().await?.state == "Running" {
        return Err("restoration acceptance pending: original task is starting; journal retained, no second launch".into());
    }
    service.launch_windows_once(outgoing).await?;
    Ok(())
}

pub(super) async fn run(args: Vec<String>) -> Result<(), String> {
    let abort = args.first().is_some_and(|arg| arg == "abort");
    let recovery = abort || args.first().is_some_and(|arg| arg == "recover");
    let offset = usize::from(recovery);
    if args.len() != offset + 2 || args[offset + 1] != "--legacy-operator-window" {
        return Err("usage: continuum container-maintenance [recover|abort] <container> --legacy-operator-window; requires an attended window excluding unregistered/copied legacy readers and producers until exact recovery completes".into());
    }
    let source = PathBuf::from(&args[offset]);
    if !source.is_absolute() {
        return Err("container maintenance requires its absolute owned path".into());
    }
    let home = continuum_root()?;
    let payload = continuum_core::paths::payload_root(&home)?;
    let task = PreparedCoreService::query().await?;
    if !task.enabled && !recovery {
        return Err("maintenance requires an originally enabled managed core task".into());
    }
    let service = PreparedCoreService {
        task,
        engine_selection: None,
    };
    // The verified prebuilt CLI must be outside the closure it will exclude.
    let own = std::env::current_exe().map_err(|e| e.to_string())?;
    if own.canonicalize().map_err(|e| e.to_string())?.starts_with(
        payload
            .join("bin")
            .canonicalize()
            .map_err(|e| e.to_string())?,
    ) {
        return Err(
            "run the verified prebuilt maintenance CLI outside installed service/engine slots"
                .into(),
        );
    }
    let saved = placement::saved_operation_digest(&source)?;
    if recovery && LegacySchedulerWindow::restoration_pending(&home, &source)? {
        let expectation = ServingExpectation::saved(&home, &source)?;
        let digest = LegacySchedulerWindow::recovery_binding(&home, &source)?;
        let check_source = source.clone();
        let check_digest = digest.clone();
        let _placement = tokio::task::spawn_blocking(move || {
            placement::hold_restoration(&check_source, &check_digest)
        })
        .await
        .map_err(|e| format!("restoration reconciliation worker: {e}"))??;
        let pending = LegacyRestoration::resume(&home, &payload, &digest, &source).await?;
        expectation.check_value(pending.serving_expectation())?;
        launch_restored_once(&service, &[]).await?;
        expectation.verify().await?;
        pending.finish().await?;
        println!("original serving expectation verified; restoration journal completed without repeating migration");
        return Ok(());
    }
    if abort {
        if saved.is_some() {
            return Err(
                "placement has begun; abort cannot re-enable legacy readers, use exact recovery"
                    .into(),
            );
        }
        let source = continuum_core::paths::cargo::physical_root(&source)?;
        let expectation = ServingExpectation::saved(&home, &source)?;
        let digest = LegacySchedulerWindow::recovery_binding(&home, &source)?;
        let window =
            LegacySchedulerWindow::begin_recovery(&home, &payload, &digest, &source).await?;
        expectation.check_pinned(&window)?;
        let _unstarted = placement::hold_unstarted(&source)?;
        // Repeat under installation/journal custody. No preparation, capacity
        // admission or successful migration is required to undo only scheduling.
        if placement::saved_operation_digest(&source)?.is_some() {
            return Err("placement appeared during abort; reconciliation remains required".into());
        }
        let pending = window.restore().await?;
        expectation.check_value(pending.serving_expectation())?;
        launch_restored_once(&service, &[]).await?;
        expectation.verify().await?;
        pending.finish().await?;
        println!("placement never began; original scheduler restored without copying or retiring source data");
        return Ok(());
    }
    if !recovery && saved.is_some() {
        return Err("saved placement requires explicit recovery".into());
    }
    let source_copy = source.clone();
    let prepared = if saved.is_none() {
        Some(
            tokio::task::spawn_blocking(move || placement::prepare_online_windows(&source_copy))
                .await
                .map_err(|e| format!("online preparation worker: {e}"))??,
        )
    } else {
        None
    };
    let digest = match (&prepared, saved) {
        (Some(prepared), _) => prepared.operation_digest()?,
        (_, Some(saved)) => saved,
        _ => return Err("placement identity unavailable".into()),
    };
    let source = match &prepared {
        Some(prepared) => prepared.source_root()?,
        None => continuum_core::paths::cargo::physical_root(&source)?,
    };
    let expectation = if recovery {
        ServingExpectation::saved(&home, &source)?
    } else {
        ServingExpectation::capture().await?
    };
    let window = if recovery {
        LegacySchedulerWindow::begin_recovery(&home, &payload, &digest, &source).await?
    } else {
        // Boundary: persist this typed original expectation in the operation journal before disabling.
        LegacySchedulerWindow::begin(
            &home,
            &payload,
            &digest,
            &source,
            serde_json::to_value(&expectation).map_err(|e| e.to_string())?, // Disk boundary: persist the typed original serving expectation in the maintenance journal.
        )
        .await?
    };
    expectation.check_pinned(&window)?;
    let engines = tokio::task::spawn_blocking(move || MaintenanceEngines::capture(&home))
        .await
        .map_err(|e| format!("engine custody worker: {e}"))??;
    let outgoing = cores_to_tear_down();
    stop_with_policy(false, true, true)
        .await?
        .require_durable()?;
    confirm_exited()?;
    let images = window.exclude_managed_images().await?;
    let runtime = tokio::runtime::Handle::current();
    let (outcome, custody) = tokio::task::spawn_blocking(move || {
        let mut custody = Custody {
            images,
            engines,
            source,
            runtime,
        };
        let outcome = (|| {
            custody.engines.exclude(custody.images.exclusive_files())?;
            // A legacy bootstrap might already have been spawning when disabled.
            // Successful image opens alone do not prove its old mapping exited.
            custody.revalidate(&custody.source, MigrationStage::Observe)?;
            if custody.images.operation_digest() != digest {
                return Err("maintenance operation changed".into());
            }
            match prepared {
                Some(prepared) => prepared.freeze(&custody),
                None => placement::freeze(&custody.source, &custody),
            }
        })();
        (outcome, custody)
    })
    .await
    .map_err(|e| format!("placement worker failed; saved scheduler recovery required: {e}"))?;
    // Any ambiguous partial retirement retains the disabled journal. Never
    // automatically re-enable legacy launchers before exact reconciliation.
    let bytes = outcome.map_err(|e| {
        format!("placement refused; explicit bound recovery required before restarting: {e}")
    })?;
    let pending = custody.images.restore().await?;
    expectation.check_value(pending.serving_expectation())?;
    launch_restored_once(&service, &outgoing).await?;
    expectation.verify().await?;
    pending.finish().await?;
    println!(
        "retired {bytes} verified source bytes; original model, native identity, adapter path bindings and geometry verified"
    );
    Ok(())
}
