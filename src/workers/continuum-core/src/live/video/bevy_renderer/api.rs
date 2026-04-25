//! Public API — BevyAvatarSystem singleton and its methods.

use crossbeam_channel::{Receiver, Sender};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};
use std::time::Duration;

use crate::live::avatar::RgbaFrame;
use crate::{clog_info, clog_warn};

use super::app::run_bevy_app;
use super::types::{AvatarCommand, BevyMemoryStats, Emotion, Gesture, SpeechAnimationClip};
use super::MAX_AVATAR_SLOTS;

/// GPU memory manager for render VRAM tracking.
static RENDERER_GPU_MANAGER: OnceLock<Arc<crate::gpu::memory_manager::GpuMemoryManager>> =
    OnceLock::new();

/// Provide the GPU memory manager to the renderer subsystem.
pub fn set_gpu_manager(mgr: Arc<crate::gpu::memory_manager::GpuMemoryManager>) {
    let _ = RENDERER_GPU_MANAGER.set(mgr);
}

pub(super) fn gpu_manager() -> Option<&'static Arc<crate::gpu::memory_manager::GpuMemoryManager>> {
    RENDERER_GPU_MANAGER.get()
}

/// Global singleton for the Bevy avatar rendering system.
/// RwLock<Option<Arc<...>>> allows shutdown + restart (memory reclaim on idle).
static BEVY_SYSTEM: RwLock<Option<Arc<BevyAvatarSystem>>> = RwLock::new(None);

/// Get or initialize the global BevyAvatarSystem.
/// Starts Bevy on first call; restarts it if previously shut down.
pub fn get_or_init() -> Arc<BevyAvatarSystem> {
    // Fast path: read lock, clone Arc if present
    {
        let guard = BEVY_SYSTEM.read().unwrap();
        if let Some(ref sys) = *guard {
            return Arc::clone(sys);
        }
    }
    // Slow path: write lock, double-check, start
    let mut guard = BEVY_SYSTEM.write().unwrap();
    if let Some(ref sys) = *guard {
        return Arc::clone(sys);
    }
    clog_info!(
        "🎨 Starting Bevy headless avatar renderer ({MAX_AVATAR_SLOTS} slots, {}x{} @{}fps)",
        super::AVATAR_WIDTH,
        super::AVATAR_HEIGHT,
        super::AVATAR_FPS
    );
    let sys = Arc::new(BevyAvatarSystem::start());
    *guard = Some(Arc::clone(&sys));
    sys
}

/// Get the BevyAvatarSystem if it is currently running.
/// Returns a cloned Arc — safe to hold across await points.
pub fn try_get() -> Option<Arc<BevyAvatarSystem>> {
    let guard = BEVY_SYSTEM.read().unwrap();
    guard.as_ref().map(Arc::clone)
}

/// Shut down the Bevy renderer entirely to reclaim ~3GB of memory.
/// Sends Shutdown command to the Bevy thread, then clears the global.
/// Next call to `get_or_init()` will restart it transparently.
pub fn shutdown() {
    let sys = {
        let mut guard = BEVY_SYSTEM.write().unwrap();
        guard.take()
    };
    if let Some(sys) = sys {
        clog_info!("🎨 Shutting down Bevy renderer to reclaim memory");
        let _ = sys.command_tx.send(AvatarCommand::Shutdown);
    }
}

/// Returns true if the Bevy renderer is currently running.
pub fn is_running() -> bool {
    let guard = BEVY_SYSTEM.read().unwrap();
    guard.is_some()
}

/// The singleton Bevy avatar rendering system.
pub struct BevyAvatarSystem {
    command_tx: Sender<AvatarCommand>,
    frame_receivers: Vec<Receiver<RgbaFrame>>,
    frame_notifiers: Vec<Arc<tokio::sync::Notify>>,
    ready: Arc<std::sync::atomic::AtomicBool>,
    identity_to_slot: std::sync::Mutex<HashMap<String, u8>>,
    pub memory_stats: Arc<BevyMemoryStats>,
}

impl BevyAvatarSystem {
    fn start() -> Self {
        let (command_tx, command_rx) = crossbeam_channel::bounded(512);
        let ready = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let ready_clone = ready.clone();

        let mut frame_senders = Vec::with_capacity(MAX_AVATAR_SLOTS as usize);
        let mut frame_receivers = Vec::with_capacity(MAX_AVATAR_SLOTS as usize);
        let mut frame_notifiers = Vec::with_capacity(MAX_AVATAR_SLOTS as usize);
        for _ in 0..MAX_AVATAR_SLOTS {
            let (tx, rx) = crossbeam_channel::bounded(4);
            frame_senders.push(tx);
            frame_receivers.push(rx);
            frame_notifiers.push(Arc::new(tokio::sync::Notify::new()));
        }

        let notifiers_for_bevy: Vec<Arc<tokio::sync::Notify>> = frame_notifiers.to_vec();

        let memory_stats = Arc::new(BevyMemoryStats::new());
        let stats_for_bevy = memory_stats.clone();

        std::thread::Builder::new()
            .name("bevy-avatar-renderer".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run_bevy_app(
                        command_rx,
                        frame_senders,
                        notifiers_for_bevy,
                        ready_clone,
                        stats_for_bevy,
                    );
                }));
                if let Err(e) = result {
                    let msg = if let Some(s) = e.downcast_ref::<&str>() {
                        s.to_string()
                    } else if let Some(s) = e.downcast_ref::<String>() {
                        s.clone()
                    } else {
                        "unknown panic".to_string()
                    };
                    crate::runtime::logger("bevy").error(&format!("BEVY THREAD CRASHED: {}", msg));
                }
            })
            .expect("Failed to spawn Bevy avatar renderer thread");

        for _ in 0..50 {
            if ready.load(std::sync::atomic::Ordering::Acquire) {
                clog_info!("🎨 Bevy renderer confirmed ready");
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }

        if !ready.load(std::sync::atomic::Ordering::Acquire) {
            clog_warn!(
                "🎨 Bevy renderer did not report ready within 5s — may have failed to init GPU"
            );
        }

        Self {
            command_tx,
            frame_receivers,
            frame_notifiers,
            ready,
            identity_to_slot: std::sync::Mutex::new(HashMap::new()),
            memory_stats,
        }
    }

    pub fn is_ready(&self) -> bool {
        self.ready.load(std::sync::atomic::Ordering::Acquire)
    }

    /// Clone the command sender for use by external systems (e.g., memory pressure shedding).
    pub fn command_sender(&self) -> Sender<AvatarCommand> {
        self.command_tx.clone()
    }

    pub fn frame_receiver(&self, slot: u8) -> Option<&Receiver<RgbaFrame>> {
        self.frame_receivers.get(slot as usize)
    }

    pub fn frame_notifier(&self, slot: u8) -> Option<Arc<tokio::sync::Notify>> {
        self.frame_notifiers.get(slot as usize).cloned()
    }

    pub fn load_model(&self, slot: u8, model_path: &str, display_name: &str, identity: &str) {
        if slot >= MAX_AVATAR_SLOTS {
            clog_warn!("Avatar slot {slot} exceeds max {MAX_AVATAR_SLOTS}");
            return;
        }
        let _ = self.command_tx.send(AvatarCommand::Load {
            slot,
            model_path: model_path.to_string(),
            display_name: display_name.to_string(),
            identity: identity.to_string(),
        });
    }

    pub fn unload_model(&self, slot: u8) {
        let _ = self.command_tx.send(AvatarCommand::Unload { slot });
    }

    pub fn set_speaking(&self, slot: u8, speaking: bool) {
        let _ = self
            .command_tx
            .send(AvatarCommand::SetSpeaking { slot, speaking });
    }

    pub fn register_identity(&self, identity: &str, slot: u8) {
        self.identity_to_slot
            .lock()
            .unwrap()
            .insert(identity.to_string(), slot);
    }

    pub fn unregister_identity(&self, identity: &str) {
        self.identity_to_slot.lock().unwrap().remove(identity);
    }

    pub fn identity_to_slot_map(&self) -> HashMap<String, u8> {
        self.identity_to_slot.lock().unwrap().clone()
    }

    /// Resolve identity to slot and execute a command. Returns false if identity not registered.
    fn send_by_identity(&self, identity: &str, make_cmd: impl FnOnce(u8) -> AvatarCommand) -> bool {
        if let Some(&slot) = self.identity_to_slot.lock().unwrap().get(identity) {
            let _ = self.command_tx.send(make_cmd(slot));
            true
        } else {
            false
        }
    }

    pub fn set_speaking_by_identity(&self, identity: &str, speaking: bool) -> bool {
        let found = self.send_by_identity(identity, |slot| AvatarCommand::SetSpeaking {
            slot,
            speaking,
        });
        if !found && speaking {
            clog_warn!(
                "🎨 set_speaking: identity '{}' not registered (no slot)",
                &identity[..8.min(identity.len())]
            );
        }
        found
    }

    pub fn set_mouth_weight(&self, slot: u8, weight: f32) {
        let _ = self
            .command_tx
            .send(AvatarCommand::SetMouthWeight { slot, weight });
    }

    pub fn set_mouth_weight_by_identity(&self, identity: &str, weight: f32) -> bool {
        self.send_by_identity(identity, |slot| AvatarCommand::SetMouthWeight {
            slot,
            weight,
        })
    }

    pub fn set_mouth_weight_sequence_by_identity(
        &self,
        identity: &str,
        weights: Vec<f32>,
        interval_ms: u32,
    ) -> bool {
        self.send_by_identity(identity, |slot| AvatarCommand::SetMouthWeightSequence {
            slot,
            weights,
            interval_ms,
        })
    }

    pub fn play_speech_by_identity(&self, identity: &str, clip: SpeechAnimationClip) -> bool {
        self.send_by_identity(identity, |slot| AvatarCommand::PlaySpeech { slot, clip })
    }

    pub fn stop_speech_by_identity(&self, identity: &str) -> bool {
        self.send_by_identity(identity, |slot| AvatarCommand::StopSpeech { slot })
    }

    pub fn resize_slot(&self, slot: u8, width: u32, height: u32) {
        let _ = self.command_tx.send(AvatarCommand::Resize {
            slot,
            width,
            height,
        });
    }

    pub fn set_emotion_by_identity(
        &self,
        identity: &str,
        emotion: Emotion,
        weight: f32,
        transition_ms: u32,
    ) -> bool {
        self.send_by_identity(identity, |slot| AvatarCommand::SetEmotion {
            slot,
            emotion,
            weight,
            transition_ms,
        })
    }

    pub fn set_gesture_by_identity(
        &self,
        identity: &str,
        gesture: Gesture,
        duration_ms: u32,
    ) -> bool {
        self.send_by_identity(identity, |slot| AvatarCommand::SetGesture {
            slot,
            gesture,
            duration_ms,
        })
    }

    pub fn set_cognitive_state_by_identity(
        &self,
        identity: &str,
        state: crate::live::session::cognitive_animation::CognitiveState,
    ) -> bool {
        self.send_by_identity(identity, |slot| AvatarCommand::SetCognitiveState {
            slot,
            state,
        })
    }

    pub fn resize_by_identity(&self, identity: &str, width: u32, height: u32) -> bool {
        self.send_by_identity(identity, |slot| AvatarCommand::Resize {
            slot,
            width,
            height,
        })
    }
}
