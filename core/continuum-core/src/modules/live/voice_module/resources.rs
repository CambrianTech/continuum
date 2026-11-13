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

        let mut unloaded = Vec::new();

        // Collect initialized STT adapters (drop lock before await)
        let stt_adapters: Vec<_> = {
            let registry = crate::live::audio::stt::get_registry();
            let reg = registry.read();
            reg.list()
                .into_iter()
                .filter(|(_, initialized)| *initialized)
                .filter_map(|(name, _)| reg.get(name).map(|a| (name, a)))
                .collect()
        };
        for (name, adapter) in stt_adapters {
            match adapter.shutdown().await {
                Ok(()) => unloaded.push(name.to_string()),
                Err(e) => {
                    log_error!(
                        "module",
                        "voice_resource_unload",
                        "STT '{}' shutdown failed: {}",
                        name,
                        e
                    );
                }
            }
        }

        // Collect initialized TTS adapters (drop lock before await)
        let tts_adapters: Vec<_> = {
            let registry = crate::live::audio::tts::get_registry();
            let reg = registry.read();
            reg.list()
                .into_iter()
                .filter(|(_, initialized)| *initialized)
                .filter_map(|(name, _)| reg.get(name).map(|a| (name, a)))
                .collect()
        };
        for (name, adapter) in tts_adapters {
            match adapter.shutdown().await {
                Ok(()) => unloaded.push(name.to_string()),
                Err(e) => {
                    log_error!(
                        "module",
                        "voice_resource_unload",
                        "TTS '{}' shutdown failed: {}",
                        name,
                        e
                    );
                }
            }
        }

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
                unloaded.push("bevy-renderer".to_string());
            }
            running
        };
        #[cfg(not(feature = "avatar-3d"))]
        let bevy_was_running = false;

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
