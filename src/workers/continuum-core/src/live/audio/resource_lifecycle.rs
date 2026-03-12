//! AudioResourceLifecycle — session counting + idle-timeout unloading.
//!
//! Tracks how many active voice sessions exist. When the count drops to zero,
//! starts an idle timer. If no new sessions start within the timeout period,
//! shuts down all STT/TTS adapters to reclaim ~3GB of memory.
//!
//! Models reload transparently on next call (1-5s latency, acceptable since
//! call setup already takes longer).

use crate::{clog_info, clog_warn};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Notify;

/// Default idle timeout before unloading models (seconds).
const DEFAULT_IDLE_TIMEOUT_SECS: u64 = 60;

/// Manages the lifecycle of audio models based on active session count.
///
/// Pattern:
/// - `on_session_start()` → increment counter
/// - `on_session_end()` → decrement counter, notify watcher when zero
/// - Idle watcher task → waits for zero sessions, sleeps timeout, re-checks, shuts down
pub struct AudioResourceLifecycle {
    active_sessions: AtomicU32,
    idle_timeout: Duration,
    /// Notified when active_sessions transitions from >0 to 0
    idle_notify: Notify,
}

impl AudioResourceLifecycle {
    pub fn new() -> Self {
        Self {
            active_sessions: AtomicU32::new(0),
            idle_timeout: Duration::from_secs(DEFAULT_IDLE_TIMEOUT_SECS),
            idle_notify: Notify::new(),
        }
    }

    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            active_sessions: AtomicU32::new(0),
            idle_timeout: timeout,
            idle_notify: Notify::new(),
        }
    }

    /// Called when a new voice session starts.
    pub fn on_session_start(&self) {
        let prev = self.active_sessions.fetch_add(1, Ordering::SeqCst);
        clog_info!(
            "AudioResourceLifecycle: session started (active: {} → {})",
            prev,
            prev + 1
        );
    }

    /// Called when a voice session ends.
    /// Notifies the idle watcher when the count reaches zero.
    pub fn on_session_end(&self) {
        let prev = self.active_sessions.fetch_sub(1, Ordering::SeqCst);
        let new_count = prev.saturating_sub(1);
        clog_info!(
            "AudioResourceLifecycle: session ended (active: {} → {})",
            prev,
            new_count
        );

        if new_count == 0 {
            clog_info!(
                "AudioResourceLifecycle: all sessions ended, idle timer starts ({}s)",
                self.idle_timeout.as_secs()
            );
            self.idle_notify.notify_one();
        }
    }

    /// Current number of active sessions.
    pub fn active_count(&self) -> u32 {
        self.active_sessions.load(Ordering::SeqCst)
    }

    /// Spawn the idle watcher as a background tokio task.
    ///
    /// This task loops forever:
    /// 1. Wait for notification that sessions hit zero
    /// 2. Sleep for the idle timeout
    /// 3. Re-check: if still zero sessions, shut down all audio models
    /// 4. If sessions were created during sleep, go back to waiting
    pub fn spawn_idle_watcher(self: &Arc<Self>) {
        let lifecycle = Arc::clone(self);

        tokio::spawn(async move {
            loop {
                // Wait for the "all sessions ended" notification
                lifecycle.idle_notify.notified().await;

                // Sleep for the idle timeout
                tokio::time::sleep(lifecycle.idle_timeout).await;

                // Re-check: did a new session start while we were sleeping?
                let count = lifecycle.active_sessions.load(Ordering::SeqCst);
                if count > 0 {
                    clog_info!(
                        "AudioResourceLifecycle: {} sessions active after idle timeout, skipping shutdown",
                        count
                    );
                    continue;
                }

                // Still zero sessions — shut down all audio models
                clog_info!(
                    "AudioResourceLifecycle: idle timeout expired with 0 sessions — shutting down audio models"
                );
                Self::shutdown_all_adapters().await;
            }
        });
    }

    /// Shut down all STT and TTS adapters to reclaim memory.
    ///
    /// Collects adapter Arcs while holding the registry lock, then drops the lock
    /// before awaiting shutdown (parking_lot guards are !Send across await points).
    async fn shutdown_all_adapters() {
        // Collect initialized STT adapters (drop lock before await)
        let stt_adapters: Vec<(&str, std::sync::Arc<dyn super::stt::SpeechToText>)> = {
            let registry = super::stt::get_registry();
            let reg = registry.read();
            reg.list()
                .into_iter()
                .filter(|(_, initialized)| *initialized)
                .filter_map(|(name, _)| reg.get(name).map(|a| (name, a)))
                .collect()
        };

        for (name, adapter) in stt_adapters {
            match adapter.shutdown().await {
                Ok(()) => clog_info!("AudioResourceLifecycle: STT '{}' shut down", name),
                Err(e) => clog_warn!(
                    "AudioResourceLifecycle: STT '{}' shutdown failed: {}",
                    name,
                    e
                ),
            }
        }

        // Collect initialized TTS adapters (drop lock before await)
        let tts_adapters: Vec<(&str, std::sync::Arc<dyn super::tts::TextToSpeech>)> = {
            let registry = super::tts::get_registry();
            let reg = registry.read();
            reg.list()
                .into_iter()
                .filter(|(_, initialized)| *initialized)
                .filter_map(|(name, _)| reg.get(name).map(|a| (name, a)))
                .collect()
        };

        for (name, adapter) in tts_adapters {
            match adapter.shutdown().await {
                Ok(()) => clog_info!("AudioResourceLifecycle: TTS '{}' shut down", name),
                Err(e) => clog_warn!(
                    "AudioResourceLifecycle: TTS '{}' shutdown failed: {}",
                    name,
                    e
                ),
            }
        }
    }
}

impl Default for AudioResourceLifecycle {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_counting() {
        let lifecycle = AudioResourceLifecycle::new();
        assert_eq!(lifecycle.active_count(), 0);

        lifecycle.on_session_start();
        assert_eq!(lifecycle.active_count(), 1);

        lifecycle.on_session_start();
        assert_eq!(lifecycle.active_count(), 2);

        lifecycle.on_session_end();
        assert_eq!(lifecycle.active_count(), 1);

        lifecycle.on_session_end();
        assert_eq!(lifecycle.active_count(), 0);
    }

    #[test]
    fn test_custom_timeout() {
        let lifecycle = AudioResourceLifecycle::with_timeout(Duration::from_secs(120));
        assert_eq!(lifecycle.idle_timeout.as_secs(), 120);
    }
}
