//! Audio injection and ambient sources into a live call.

use super::super::*;

impl VoiceModule {
    pub(super) async fn cmd_inject_audio(&self, p: &Params<'_>) -> Result<CommandResult, String> {
        let _timer = TimingGuard::new("module", "voice_inject_audio");
        let call_id = self.state.canonical_call_id(p.str("call_id")?);
        let user_id = p.str("user_id")?;
        let samples: Vec<i16> = p.json("samples")?;

        self.state
            .livekit_manager
            .inject_audio(&call_id, user_id, samples)
            .await
            .map_err(|e| {
                log_error!("module", "voice_inject_audio", "inject-audio failed: {}", e);
                format!("inject-audio failed: {}", e)
            })?;

        log_info!(
            "module",
            "voice_inject_audio",
            "Injected audio into call {} for {}",
            call_id,
            user_id
        );
        Ok(CommandResult::Json(serde_json::json!({ "success": true })))
    }

    pub(super) async fn cmd_ambient_add(&self, p: &Params<'_>) -> Result<CommandResult, String> {
        let _timer = TimingGuard::new("module", "voice_ambient_add");
        let call_id = self.state.canonical_call_id(p.str("call_id")?);
        let source_name = p.str("source_name")?;

        let handle = self
            .state
            .livekit_manager
            .add_ambient_source(&call_id, source_name)
            .await
            .map_err(|e| {
                log_error!("module", "voice_ambient_add", "ambient-add failed: {}", e);
                format!("ambient-add failed: {}", e)
            })?;

        log_info!(
            "module",
            "voice_ambient_add",
            "Added ambient source '{}' to call {}",
            source_name,
            call_id
        );
        Ok(CommandResult::Json(serde_json::json!({
            "handle": handle,
            "source_name": source_name,
        })))
    }

    pub(super) async fn cmd_ambient_inject(&self, p: &Params<'_>) -> Result<CommandResult, String> {
        let _timer = TimingGuard::new("module", "voice_ambient_inject");
        let call_id = self.state.canonical_call_id(p.str("call_id")?);
        let handle_str = p.str("handle")?;
        let samples: Vec<i16> = p.json("samples")?;

        self.state
            .livekit_manager
            .inject_ambient(&call_id, handle_str, samples)
            .await
            .map_err(|e| {
                log_error!(
                    "module",
                    "voice_ambient_inject",
                    "ambient-inject failed: {}",
                    e
                );
                format!("ambient-inject failed: {}", e)
            })?;

        Ok(CommandResult::Json(serde_json::json!({ "success": true })))
    }

    pub(super) async fn cmd_ambient_remove(&self, p: &Params<'_>) -> Result<CommandResult, String> {
        let _timer = TimingGuard::new("module", "voice_ambient_remove");
        let call_id = self.state.canonical_call_id(p.str("call_id")?);
        let handle_str = p.str("handle")?;

        self.state
            .livekit_manager
            .remove_ambient_source(&call_id, handle_str)
            .await
            .map_err(|e| {
                log_error!(
                    "module",
                    "voice_ambient_remove",
                    "ambient-remove failed: {}",
                    e
                );
                format!("ambient-remove failed: {}", e)
            })?;

        log_info!(
            "module",
            "voice_ambient_remove",
            "Removed ambient source from call {}",
            call_id
        );
        Ok(CommandResult::Json(serde_json::json!({ "removed": true })))
    }
}
