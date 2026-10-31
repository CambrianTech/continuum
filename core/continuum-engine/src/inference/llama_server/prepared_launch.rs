//! Prepared launch responsibility of the llama-server process owner.
use super::*;
/// Private original launch recipe; no rediscovery at replay.
#[derive(Clone)]
pub(super) struct PreparedLocalLaunch {
    pub(super) target: ServingTarget,
    /// The engine checked against the pin when this launch was prepared: the ONLY binary it runs.
    pub(super) engine: super::checked_engine::CheckedEngine,
    /// The slot claim taken when `engine` was checked, until the first launch of this recipe
    /// takes it (a relaunch of a retained recipe claims anew).
    pub(super) engine_custody: Arc<parking_lot::Mutex<Option<crate::inference::engine_slots::SlotClaim>>>,
    pub(super) endpoint: String,
    pub(super) invocation: crate::inference::lane_args::LaneInvocation,
    pub(super) engine_install: Option<crate::inference::engine_install::EngineInstallReceipt>,
    pub(super) gguf: PathBuf,
    pub(super) slot_save_dir: PathBuf,
    /// The KV type the engine RUNS, for the page contract: the plan's effective type, never
    /// the launcher flag (f16 is passed as no flag at all, which read as "unknown" and cost
    /// every CPU lane its saved-page ledger, IntelMac 2026-10-11). `None` = genuinely unknown.
    pub(super) page_cache_type: Option<String>,
    /// Whether the engine runs flash attention: part of a saved page's format (its V layout).
    pub(super) page_flash_attn: Option<bool>,
    /// The MTP draft model, whose KV a page's context checkpoints carry beside the target's.
    pub(super) mtp_draft: Option<PathBuf>,
    pub(super) mmproj: bool,
    // Err retains an ordinary/legacy launch without inventing restoration authority.
    pub(super) local_inputs: Result<crate::inference::launch_inputs::LocalLaunchInputs, String>,
    pub(super) managed_environment: Vec<(String, Option<std::ffi::OsString>)>,
    pub(super) expected_geometry: Option<(u32, u32)>,
}


pub(super) fn unsupported_launch_environment(managed: &[(String, Option<std::ffi::OsString>)]) -> bool {
    std::env::vars_os().any(|(key, _)| {
        let key = key.to_string_lossy().to_ascii_uppercase();
        (key.starts_with("LLAMA_") || key.starts_with("GGML_"))
            && !managed.iter().any(|(known, _)| known == &key)
    })
}


impl PreparedLocalLaunch {
    /// The binary this launch runs.
    pub(super) fn engine_program(&self) -> &str {
        self.engine.program()
    }

    /// The claim on this launch's slot: the one taken at prepare, or a new one for a recipe
    /// launched again (see [`super::checked_engine`]).
    pub(super) fn take_engine_custody(
        &self,
    ) -> Result<Option<crate::inference::engine_slots::SlotClaim>, LlamaServerError> {
        let taken = self.engine_custody.lock().take();
        match taken {
            Some(claim) => {
                self.confirm_engine()?;
                Ok(Some(claim))
            }
            None => self.engine.claim_slot().map_err(LlamaServerError::EngineUnpinned),
        }
    }

    /// Observe that the checked engine's slot still holds its validated build.
    pub(super) fn confirm_engine(&self) -> Result<(), LlamaServerError> {
        self.engine.confirm().map_err(LlamaServerError::EngineUnpinned)
    }

    pub(super) fn verify_geometry(&self, context: u32, lanes: u32) -> Result<(), LlamaServerError> {
        if self
            .expected_geometry
            .is_some_and(|expected| expected != (context, lanes))
        {
            return Err(LlamaServerError::Spawn(
                "restored engine geometry differs from the original verified launch".into(),
            ));
        }
        Ok(())
    }

    pub(super) fn command(&self) -> Result<tokio::process::Command, LlamaServerError> {
        let mut command = match &self.engine_install {
            Some(receipt) => receipt.command().map_err(LlamaServerError::Spawn)?,
            None => tokio::process::Command::new(self.engine.program()),
        };
        command.args(&self.invocation.args);
        for (key, value) in &self.managed_environment {
            match value {
                Some(value) => {
                    command.env(key, value);
                }
                None => {
                    command.env_remove(key);
                }
            }
        }
        for (key, value) in &self.invocation.envs {
            command.env(key, value);
        }
        Ok(command)
    }

    pub(super) async fn validate_restoration(
        &self,
        endpoint: &str,
        program: &str,
    ) -> Result<(), LlamaServerError> {
        if self.endpoint != endpoint || self.engine.program() != program {
            return Err(LlamaServerError::Spawn(
                "prepared launch belongs to another endpoint".into(),
            ));
        }
        if self.expected_geometry.is_none() {
            return Err(LlamaServerError::Spawn(
                "original launch has no verified geometry".into(),
            ));
        }
        let inputs = self
            .local_inputs
            .as_ref()
            .map_err(|reason| LlamaServerError::Spawn(reason.clone()))?;
        if unsupported_launch_environment(&self.managed_environment) {
            return Err(LlamaServerError::Spawn(
                "unclassified engine environment prevents exact restoration".into(),
            ));
        }
        let application = self.engine_install.as_ref().ok_or_else(|| {
            LlamaServerError::Spawn("original application receipt is absent".into())
        })?;
        application
            .confirm()
            .await
            .map_err(LlamaServerError::Spawn)?;
        inputs.validate().await.map_err(LlamaServerError::Spawn)
    }
}
