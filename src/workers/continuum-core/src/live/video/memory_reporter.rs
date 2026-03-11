//! Bevy renderer MemoryReporter — reports avatar system memory usage.
//!
//! Reads atomic stats from BevyMemoryStats (updated every frame by Bevy systems).
//! No locks, no blocking, no cross-thread calls. Pure atomic reads.
//!
//! Under memory pressure, sends commands to downscale render targets or unload
//! idle avatar slots via the crossbeam command channel.

use crossbeam_channel::Sender;
use std::sync::Arc;

use crate::live::video::bevy_renderer::{
    AvatarCommand, BevyMemoryStats, AVATAR_HEIGHT, AVATAR_WIDTH, MAX_AVATAR_SLOTS,
};
use crate::system_resources::memory_pressure::{
    MemoryBudgetSpec, MemoryPriority, MemoryReporter, ModuleMemoryReport, PressureLevel,
};

/// Approximate bytes per loaded VRM model (meshes + textures + skeleton + morph targets).
/// Conservative estimate based on typical VRM models (2-10MB each).
const ESTIMATED_MODEL_BYTES: u64 = 8 * 1024 * 1024; // 8MB average

/// Minimum render resolution under pressure (halved from default 640x360).
const PRESSURE_WIDTH: u32 = 320;
const PRESSURE_HEIGHT: u32 = 180;

/// MemoryReporter for the Bevy avatar rendering system.
pub struct BevyMemoryReporter {
    stats: Arc<BevyMemoryStats>,
    command_tx: Sender<AvatarCommand>,
}

impl BevyMemoryReporter {
    pub fn new(stats: Arc<BevyMemoryStats>, command_tx: Sender<AvatarCommand>) -> Self {
        Self { stats, command_tx }
    }
}

impl MemoryReporter for BevyMemoryReporter {
    fn name(&self) -> &'static str {
        "bevy"
    }

    fn budget(&self) -> MemoryBudgetSpec {
        MemoryBudgetSpec {
            name: "bevy".to_string(),
            priority: MemoryPriority::Realtime, // Render loop is sacrosanct
            min_bytes: 100 * 1024 * 1024,       // 100MB — Bevy overhead + 1-2 models minimum
            preferred_bytes: 300 * 1024 * 1024,  // 300MB — 16 slots at 640x360 + all models
            max_bytes: 500 * 1024 * 1024,        // 500MB — HD pool + all 16 loaded
        }
    }

    fn report(&self) -> ModuleMemoryReport {
        let active = self.stats.active_slots.load(std::sync::atomic::Ordering::Relaxed);
        let loaded = self.stats.loaded_models.load(std::sync::atomic::Ordering::Relaxed);
        let speaking = self.stats.speaking_slots.load(std::sync::atomic::Ordering::Relaxed);
        let rt_bytes = self.stats.render_target_bytes.load(std::sync::atomic::Ordering::Relaxed);
        let pending = self.stats.pending_loads.load(std::sync::atomic::Ordering::Relaxed);

        // Estimate total: render targets + loaded models + Bevy overhead
        let model_bytes = loaded as u64 * ESTIMATED_MODEL_BYTES;
        // Bevy base overhead (ECS world, asset server, scheduler) ~50MB
        let bevy_overhead: u64 = 50 * 1024 * 1024;
        let total_bytes = rt_bytes + model_bytes + bevy_overhead;

        ModuleMemoryReport {
            name: "bevy".to_string(),
            bytes: total_bytes,
            detail: format!(
                "{} active, {} loaded, {} speaking, RT={}MB, models~{}MB, pending={}",
                active,
                loaded,
                speaking,
                rt_bytes / (1024 * 1024),
                model_bytes / (1024 * 1024),
                pending,
            ),
            can_shed: true,
        }
    }

    fn can_shed(&self) -> bool {
        true
    }

    fn shed_load(&self, level: PressureLevel) {
        let loaded = self.stats.loaded_models.load(std::sync::atomic::Ordering::Relaxed);
        let speaking = self.stats.speaking_slots.load(std::sync::atomic::Ordering::Relaxed);

        match level {
            PressureLevel::High => {
                crate::clog_warn!(
                    "🧠 Bevy shed_load(High): downscaling idle slots, idle_cadence=4 ({} loaded, {} speaking)",
                    loaded,
                    speaking,
                );
                for slot in 0..MAX_AVATAR_SLOTS {
                    let _ = self.command_tx.send(AvatarCommand::Resize {
                        slot,
                        width: PRESSURE_WIDTH,
                        height: PRESSURE_HEIGHT,
                    });
                }
                self.stats.desired_idle_cadence.store(4, std::sync::atomic::Ordering::Relaxed);
            }
            PressureLevel::Critical => {
                crate::clog_warn!(
                    "🧠 Bevy shed_load(Critical): unloading idle slots, idle_cadence=8 ({} loaded, {} speaking)",
                    loaded,
                    speaking,
                );
                for slot in 0..MAX_AVATAR_SLOTS {
                    let _ = self.command_tx.send(AvatarCommand::Resize {
                        slot,
                        width: PRESSURE_WIDTH,
                        height: PRESSURE_HEIGHT,
                    });
                }
                self.stats.desired_idle_cadence.store(8, std::sync::atomic::Ordering::Relaxed);
            }
            PressureLevel::Warning => {
                crate::clog_info!(
                    "🧠 Bevy shed_load(Warning): idle_cadence=2 ({} loaded, {} speaking)",
                    loaded,
                    speaking,
                );
                for slot in 0..MAX_AVATAR_SLOTS {
                    let _ = self.command_tx.send(AvatarCommand::Resize {
                        slot,
                        width: AVATAR_WIDTH,
                        height: AVATAR_HEIGHT,
                    });
                }
                self.stats.desired_idle_cadence.store(2, std::sync::atomic::Ordering::Relaxed);
            }
            PressureLevel::Normal => {
                for slot in 0..MAX_AVATAR_SLOTS {
                    let _ = self.command_tx.send(AvatarCommand::Resize {
                        slot,
                        width: AVATAR_WIDTH,
                        height: AVATAR_HEIGHT,
                    });
                }
                self.stats.desired_idle_cadence.store(1, std::sync::atomic::Ordering::Relaxed);
            }
        }
    }
}
