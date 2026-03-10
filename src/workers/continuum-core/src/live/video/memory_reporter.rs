//! Bevy renderer MemoryReporter — reports avatar system memory usage.
//!
//! Reads atomic stats from BevyMemoryStats (updated every frame by Bevy systems).
//! No locks, no blocking, no cross-thread calls. Pure atomic reads.

use std::sync::Arc;

use crate::live::video::bevy_renderer::BevyMemoryStats;
use crate::system_resources::memory_pressure::{
    MemoryReporter, ModuleMemoryReport, PressureLevel,
};

/// Approximate bytes per loaded VRM model (meshes + textures + skeleton + morph targets).
/// Conservative estimate based on typical VRM models (2-10MB each).
const ESTIMATED_MODEL_BYTES: u64 = 8 * 1024 * 1024; // 8MB average

/// MemoryReporter for the Bevy avatar rendering system.
pub struct BevyMemoryReporter {
    stats: Arc<BevyMemoryStats>,
}

impl BevyMemoryReporter {
    pub fn new(stats: Arc<BevyMemoryStats>) -> Self {
        Self { stats }
    }
}

impl MemoryReporter for BevyMemoryReporter {
    fn name(&self) -> &'static str {
        "bevy"
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
        // Future: send commands to Bevy to deactivate slots or reduce resolution.
        // For now, just log. The actual shed_load implementation requires sending
        // AvatarCommand::Deactivate through the command channel.
        let loaded = self.stats.loaded_models.load(std::sync::atomic::Ordering::Relaxed);
        let speaking = self.stats.speaking_slots.load(std::sync::atomic::Ordering::Relaxed);
        crate::clog_warn!(
            "🧠 Bevy shed_load({:?}): {} models loaded, {} speaking — action needed",
            level,
            loaded,
            speaking,
        );
    }
}
