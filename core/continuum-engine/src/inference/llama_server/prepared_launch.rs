//! Prepared launch responsibility of the llama-server process owner.
use super::*;
/// Private original launch recipe; no rediscovery at replay.
#[derive(Clone)]
pub(super) struct PreparedLocalLaunch {
    pub(super) target: ServingTarget,
    pub(super) engine_program: String,
    pub(super) endpoint: String,
    pub(super) invocation: crate::inference::lane_args::LaneInvocation,
    pub(super) engine_install: Option<crate::inference::engine_install::EngineInstallReceipt>,
    pub(super) gguf: PathBuf,
    pub(super) slot_save_dir: PathBuf,
    /// The KV type the engine RUNS, for the page contract: the plan's effective type, never
    /// the launcher flag (f16 is passed as no flag at all, which read as "unknown" and cost
    /// every CPU lane its saved-page ledger, IntelMac 2026-10-11). `None` = genuinely unknown.
    pub(super) page_cache_type: Option<String>,
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
            None => tokio::process::Command::new(&self.engine_program),
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
        if self.endpoint != endpoint || self.engine_program != program {
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
