//! Audio resource status and force-unload.

use super::super::*;

impl VoiceModule {
    pub(super) async fn cmd_resource_status(&self, _p: &Params<'_>) -> Result<CommandResult, String> {
        let _timer = TimingGuard::new("module", "voice_resource_status");

        // Collect STT adapter status
        let stt_status: Vec<serde_json::Value> = {
            let registry = crate::live::audio::stt::get_registry();
            let reg = registry.read();
            reg.list()
                .iter()
                .map(|(name, initialized)| {
                    serde_json::json!({
                        "name": name,
                        "loaded": initialized,
                    })
                })
                .collect()
        };

        // Collect TTS adapter status
        let tts_status: Vec<serde_json::Value> = {
            let registry = crate::live::audio::tts::get_registry();
            let reg = registry.read();
            reg.list()
                .iter()
                .map(|(name, initialized)| {
                    serde_json::json!({
                        "name": name,
                        "loaded": initialized,
                    })
                })
                .collect()
        };

        let active_sessions = self.state.resource_lifecycle.active_count();
        #[cfg(feature = "avatar-3d")]
        let bevy_running = crate::live::video::bevy_renderer::is_running();
        // No renderer is compiled into this build, so it is not running.
        #[cfg(not(feature = "avatar-3d"))]
        let bevy_running = false;

        Ok(CommandResult::Json(serde_json::json!({
            "active_sessions": active_sessions,
            "bevy_renderer": bevy_running,
            "stt_adapters": stt_status,
            "tts_adapters": tts_status,
        })))
    }

    pub(super) async fn cmd_resource_unload(&self, _p: &Params<'_>) -> Result<CommandResult, String> {
        let _timer = TimingGuard::new("module", "voice_resource_unload");

        log_info!(
            "module",
            "voice_resource_unload",
            "Force-unloading all audio models"
        );

        let released = crate::live::audio::resource_lifecycle::AudioResourceLifecycle::shutdown_all_adapters().await;

        // Reset session counter — force-unload means we don't care about
        // in-flight sessions. Models will reload on next call.
        self.state.resource_lifecycle.reset_sessions();

        // Shut down Bevy renderer to reclaim ~3GB GPU/ECS memory (only exists
        // under `avatar-3d`; without it there is nothing to shut down).
        #[cfg(feature = "avatar-3d")]
        let bevy_was_running = {
            let running = crate::live::video::bevy_renderer::is_running();
            if running {
                crate::live::video::bevy_renderer::shutdown();
                crate::live::avatar::reset_slot_pool();
            }
            running
        };
        #[cfg(not(feature = "avatar-3d"))]
        let bevy_was_running = false;
        let unloaded: Vec<String> = released.into_iter().map(String::from).chain(bevy_was_running.then(|| "bevy-renderer".to_string())).collect();

        log_info!(
            "module",
            "voice_resource_unload",
            "Unloaded {} adapters: {:?}",
            unloaded.len(),
            unloaded
        );

        Ok(CommandResult::Json(serde_json::json!({
            "unloaded": unloaded,
            "count": unloaded.len(),
            "bevy_shutdown": bevy_was_running,
        })))
    }
}
