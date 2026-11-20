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

/// Grace period before unloading models after last session ends (seconds).
/// Short — just enough for in-flight async ops to complete.
/// NOT a "user might rejoin" timer — that's speculative and wastes GB of memory.
const DEFAULT_IDLE_TIMEOUT_SECS: u64 = 5;

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

                // Still zero sessions — shut down audio models only.
                // Bevy renderer stays alive — restart cost is too high (~5s)
                // and the orphan watchdog was killing it mid-conversation.
                clog_info!(
                    "AudioResourceLifecycle: idle timeout expired with 0 sessions — shutting down audio models (renderer stays alive)"
                );
                Self::shutdown_all_adapters().await;
            }
        });
    }

    /// Report a long-lived session count for lifecycle diagnostics.
    ///
    /// An unchanged count does NOT prove an orphan: a healthy call can retain
    /// the same participants for hours. Only explicit lifecycle end events
    /// authorize the idle watcher to unload adapters.
    pub fn spawn_orphan_watchdog(self: &Arc<Self>) {
        let lifecycle = Arc::clone(self);
        const ORPHAN_CHECK_INTERVAL_SECS: u64 = 30;
        const ORPHAN_TIMEOUT_SECS: u64 = 600; // 10 minutes — live calls have stable session counts for the entire call duration. 60s was killing Whisper mid-conversation.
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
                            "AudioResourceLifecycle: {} sessions unchanged for {}s; retaining audio adapters because count stability is not proof of an orphan",
                            current,
                            stale_ticks * ORPHAN_CHECK_INTERVAL_SECS
                        );
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

    /// Release every loaded speech-to-text, then text-to-speech, backend to reclaim memory, and
    /// name the ones released. The idle watcher pulls this after its idle timeout, and the voice
    /// `ResourceConsumer` pulls the same lever when the resource authority asks for memory back
    /// while no call is live. Models reload on the next call.
    pub async fn shutdown_all_adapters() -> Vec<&'static str> {
        let mut released = super::registry::release_loaded(&super::stt::get_registry()).await;
        released.extend(super::registry::release_loaded(&super::tts::get_registry()).await);
        released
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
