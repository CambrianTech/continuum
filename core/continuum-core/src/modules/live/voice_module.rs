use super::*;

mod session;
mod synthesis;
mod transcription;
mod ambient;
mod selftest;
mod snapshots;
mod resources;

#[async_trait]
impl ServiceModule for VoiceModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "live",
            priority: ModulePriority::Realtime,
            command_prefixes: &["voice/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    /// Receive the wired command executor at boot (the #224 late-bind slot). The
    /// perception-ingest drain reads it lazily to build the vision describer.
    fn install_executor(&self, executor: Arc<CommandExecutor>) {
        self.state.executor.install(executor);
    }

    async fn initialize(&self, ctx: &ModuleContext) -> Result<(), String> {
        // Spawn idle watcher here (inside tokio runtime), not in VoiceState::new()
        self.state.resource_lifecycle.spawn_idle_watcher();
        // A hosted persona's room line becomes her VOICE while the room has a
        // live call (Joel, 2026-09-05: "they say they're talking … no output").
        super::super::voice_reply_speaker::spawn(ctx.bus.clone(), self.state.clone());
        // Safety net: detect orphaned sessions (browser crash, lost WebSocket, deploy)
        self.state.resource_lifecycle.spawn_orphan_watchdog();

        // #192: spawn the live-call perception-ingest drain. The bridge reader thread
        // posts decoded frames onto a process-global mpsc (perception_ingest); this
        // drain, on the runtime, fans each frame out to every persona-viewer's
        // PerceptionBuffer. Installed once — a second module init would get None.
        if let Some(rx) = crate::media::perception_ingest::install_channel() {
            tokio::spawn(perception_ingest_drain(rx, self.state.clone()));
        }
        Ok(())
    }

    async fn handle_command(&self, command: &str, params: Value) -> Result<CommandResult, String> {
        let p = Params::new(&params);

        match command {
            "voice/register-session" => self.cmd_register_session(&p).await,
            "voice/end-session" => self.cmd_end_session(&p).await,
            "voice/on-utterance" => self.cmd_on_utterance(&p).await,
            "voice/synthesize" => self.cmd_synthesize(&p).await,
            "voice/speak-in-call" => self.cmd_speak_in_call(&p).await,
            "voice/synthesize-handle" => self.cmd_synthesize_handle(&p).await,
            "voice/play-handle" => self.cmd_play_handle(&p).await,
            "voice/discard-handle" => self.cmd_discard_handle(&p).await,
            "voice/transcribe" => self.cmd_transcribe(&p).await,
            "voice/inject-audio" => self.cmd_inject_audio(&p).await,
            "voice/ambient-add" => self.cmd_ambient_add(&p).await,
            "voice/ambient-inject" => self.cmd_ambient_inject(&p).await,
            "voice/stt-list" => self.cmd_stt_list(&p).await,
            "voice/transcribe-with-adapter" => self.cmd_transcribe_with_adapter(&p).await,
            "voice/test-audio-generate" => self.cmd_test_audio_generate(&p).await,
            "voice/selftest" => self.cmd_selftest(&p).await,
            "voice/poll-transcriptions" => self.cmd_poll_transcriptions(&p).await,
            "voice/ambient-remove" => self.cmd_ambient_remove(&p).await,
            "voice/set-cognitive-state" => self.cmd_set_cognitive_state(&p).await,
            "voice/snapshot-room" => self.cmd_snapshot_room(&p).await,
            "voice/snapshot-participant" => self.cmd_snapshot_participant(&p).await,
            "voice/resource-status" => self.cmd_resource_status(&p).await,
            "voice/resource-unload" => self.cmd_resource_unload(&p).await,

            _ => Err(format!("Unknown voice command: {command}")),
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
