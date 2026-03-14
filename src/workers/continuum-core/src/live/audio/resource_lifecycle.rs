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
        // CAS loop to prevent underflow (fetch_sub on 0 wraps to u32::MAX)
        loop {
            let current = self.active_sessions.load(Ordering::SeqCst);
            if current == 0 {
                clog_warn!(
                    "AudioResourceLifecycle: on_session_end called with 0 active sessions (double-end?)"
                );
                return;
            }
            if self
                .active_sessions
                .compare_exchange(current, current - 1, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                let new_count = current - 1;
                clog_info!(
                    "AudioResourceLifecycle: session ended (active: {} → {})",
                    current,
                    new_count
                );

                if new_count == 0 {
                    clog_info!(
                        "AudioResourceLifecycle: all sessions ended, idle timer starts ({}s)",
                        self.idle_timeout.as_secs()
                    );
                    self.idle_notify.notify_one();
                }
                return;
            }
            // CAS failed — another thread changed the value, retry
        }
    }

    /// Current number of active sessions.
    pub fn active_count(&self) -> u32 {
        self.active_sessions.load(Ordering::SeqCst)
    }

    /// Force-reset the session counter to zero.
    ///
    /// Used by `voice/resource-unload` when force-unloading models regardless
    /// of session state. Does NOT trigger the idle watcher — the caller is
    /// already unloading models directly.
    pub fn reset_sessions(&self) {
        let prev = self.active_sessions.swap(0, Ordering::SeqCst);
        if prev > 0 {
            clog_warn!(
                "AudioResourceLifecycle: force-reset sessions ({} → 0)",
                prev
            );
        }
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

                // Still zero sessions — shut down all audio models + avatar renderer
                clog_info!(
                    "AudioResourceLifecycle: idle timeout expired with 0 sessions — shutting down audio models + avatars"
                );
                Self::shutdown_all_adapters().await;
                Self::unload_avatar_models();
            }
        });
    }

    /// Spawn a safety-net watchdog that detects orphaned sessions.
    ///
    /// If the session count has been stuck at the same non-zero value for
    /// longer than the orphan timeout (5 minutes), it means voice/end-session
    /// was never called (browser crash, lost WebSocket, deploy killed the tab).
    /// Force-reset the counter and trigger shutdown.
    pub fn spawn_orphan_watchdog(self: &Arc<Self>) {
        let lifecycle = Arc::clone(self);
        const ORPHAN_CHECK_INTERVAL_SECS: u64 = 60;
        const ORPHAN_TIMEOUT_SECS: u64 = 300; // 5 minutes without change = orphaned
        const CHECKS_UNTIL_ORPHAN: u64 = ORPHAN_TIMEOUT_SECS / ORPHAN_CHECK_INTERVAL_SECS;

        tokio::spawn(async move {
            let mut stale_count: u32 = 0;
            let mut stale_ticks: u64 = 0;

            loop {
                tokio::time::sleep(Duration::from_secs(ORPHAN_CHECK_INTERVAL_SECS)).await;

                let current = lifecycle.active_sessions.load(Ordering::SeqCst);
                if current == 0 {
                    // No sessions — nothing to watch
                    stale_count = 0;
                    stale_ticks = 0;
                    continue;
                }

                if current == stale_count {
                    stale_ticks += 1;
                    if stale_ticks >= CHECKS_UNTIL_ORPHAN {
                        clog_warn!(
                            "AudioResourceLifecycle: {} sessions stuck for {}s — orphaned, force-resetting",
                            current,
                            stale_ticks * ORPHAN_CHECK_INTERVAL_SECS
                        );
                        lifecycle.active_sessions.store(0, Ordering::SeqCst);
                        Self::shutdown_all_adapters().await;
                        Self::unload_avatar_models();
                        stale_count = 0;
                        stale_ticks = 0;
                    }
                } else {
                    // Count changed — reset staleness tracker
                    stale_count = current;
                    stale_ticks = 0;
                }
            }
        });
    }

    /// Shut down the Bevy renderer entirely to reclaim ~3GB of GPU/ECS memory.
    ///
    /// Called after idle timeout — all agents are long gone, safe to teardown.
    /// Shuts down the entire Bevy thread (ECS world, wgpu device, Metal pipelines).
    /// Next call to `get_or_init()` restarts it transparently.
    fn unload_avatar_models() {
        if crate::live::video::bevy_renderer::is_running() {
            crate::live::video::bevy_renderer::shutdown();
            clog_info!("AudioResourceLifecycle: Bevy renderer shut down (~3GB freed)");
        }
        // Reset slot pool to full capacity. If any video loop tasks didn't exit
        // cleanly (e.g., disconnect signal lost), their held slots are reclaimed.
        // Safe because active_sessions == 0 — no call is using any slots.
        crate::live::avatar::reset_slot_pool();
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

    #[test]
    fn test_reset_sessions() {
        let lifecycle = AudioResourceLifecycle::new();
        lifecycle.on_session_start();
        lifecycle.on_session_start();
        lifecycle.on_session_start();
        assert_eq!(lifecycle.active_count(), 3);

        lifecycle.reset_sessions();
        assert_eq!(lifecycle.active_count(), 0);
    }
}
