//! Avatar render loop — backend-agnostic frame production and factory.
//!
//! Spawns a background thread that renders avatar frames and sends them via channel.
//! The renderer is selected by `create_renderer()` — the loop is backend-agnostic.
//!
//! Identity deduplication: if an identity already has an active render loop, subsequent
//! requests reuse the existing loop's frame receiver. No duplicate slots are allocated.
//!
//! Slot recycling: when an agent leaves the call, their render thread exits and
//! the Bevy slot is returned to the pool for reuse. No slots are ever permanently leaked.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::collections::VecDeque;
use crate::{clog_info, clog_warn, clog_error};
use super::frame::{RgbaFrame, AvatarConfig};
use super::renderer::AvatarRenderer;
use super::backends::BevyChannelRenderer;

// =============================================================================
// Slot pool — thread-safe pool of reusable Bevy render slot indices
// =============================================================================

/// Global pool of available Bevy render slots.
/// Initialized with [0..MAX_AVATAR_SLOTS]. Slots are returned when render threads exit.
static SLOT_POOL: OnceLock<std::sync::Mutex<VecDeque<u8>>> = OnceLock::new();

fn slot_pool() -> &'static std::sync::Mutex<VecDeque<u8>> {
    SLOT_POOL.get_or_init(|| {
        let slots: VecDeque<u8> = (0..crate::live::video::bevy_renderer::MAX_AVATAR_SLOTS).collect();
        std::sync::Mutex::new(slots)
    })
}

/// Allocate a render slot from the pool. Returns None if all slots are in use.
fn allocate_slot() -> Option<u8> {
    let mut pool = slot_pool().lock().unwrap();
    let slot = pool.pop_front();
    if let Some(s) = slot {
        clog_info!("🎨 Allocated render slot {} ({} remaining)", s, pool.len());
    }
    slot
}

/// Return a render slot to the pool. Unloads the model from Bevy first.
fn release_slot(slot: u8) {
    // Unload the Bevy scene for this slot so it's clean for reuse
    if let Some(bevy) = crate::live::video::bevy_renderer::try_get() {
        bevy.unload_model(slot);
    }
    let mut pool = slot_pool().lock().unwrap();
    pool.push_back(slot);
    clog_info!("🎨 Released render slot {} ({} available)", slot, pool.len());
}

// =============================================================================
// SlotGuard — RAII cleanup for Bevy slots (used by allocate_bevy_slot path)
// =============================================================================

/// RAII guard for a Bevy render slot. When dropped, unloads the model,
/// removes the identity mapping, and returns the slot to the pool.
///
/// Used by `allocate_bevy_slot()` where there is no render thread to handle
/// cleanup on exit. The video loop holds the guard; when the tokio task exits,
/// the guard drops and the slot is recycled.
pub struct SlotGuard {
    slot: u8,
    identity: String,
    released: bool,
}

impl SlotGuard {
    fn new(slot: u8, identity: String) -> Self {
        Self { slot, identity, released: false }
    }
}

impl Drop for SlotGuard {
    fn drop(&mut self) {
        if !self.released {
            self.released = true;
            if let Some(bevy) = crate::live::video::bevy_renderer::try_get() {
                bevy.unregister_identity(&self.identity);
            }
            release_slot(self.slot);
            clog_info!(
                "🎨 SlotGuard: released slot {} for '{}'",
                self.slot, &self.identity[..8.min(self.identity.len())]
            );
        }
    }
}

// =============================================================================
// allocate_bevy_slot — direct Bevy slot allocation (no render thread)
// =============================================================================

/// Result of `allocate_bevy_slot`: everything the video loop needs.
pub struct BevySlotAllocation {
    /// Bevy's FrameChannels receiver — video loop reads frames directly from GPU readback.
    pub frame_rx: crossbeam_channel::Receiver<RgbaFrame>,
    /// The allocated slot number (for speaking control, logging, etc.).
    pub slot: u8,
    /// RAII guard — hold alive for the duration of the video loop. Dropping releases the slot.
    pub guard: SlotGuard,
}

/// Allocate a Bevy render slot directly, without spawning a render thread.
///
/// Returns the Bevy FrameChannels receiver so the caller (video loop) can
/// read frames directly from GPU readback, eliminating the BevyChannelRenderer
/// middleman and its redundant Vec<u8> clone per frame.
///
/// Identity dedup: if this identity already has a Bevy slot, returns the
/// existing slot's receiver (no duplicate allocation).
///
/// The returned `SlotGuard` MUST be held alive for the duration of the video loop.
/// Dropping it unloads the model, unregisters the identity, and returns the slot to the pool.
///
/// This function acquires std::sync::Mutex locks and may block for up to 5s
/// waiting for slot availability. Call from `tokio::task::spawn_blocking`.
pub fn allocate_bevy_slot(config: AvatarConfig) -> Result<BevySlotAllocation, String> {
    let identity = config.identity.clone();
    let vrm_path = config.vrm_model_path.as_ref()
        .ok_or_else(|| format!("No VRM model for '{}'", identity))?;

    if !std::path::Path::new(vrm_path).exists() {
        return Err(format!("VRM model not found: {}", vrm_path));
    }

    let bevy_system = crate::live::video::bevy_renderer::get_or_init();
    if !bevy_system.is_ready() {
        return Err(format!("Bevy renderer not ready for '{}'", identity));
    }

    // Identity dedup: reuse existing slot if this persona already has one
    {
        let identity_map = bevy_system.identity_to_slot_map();
        if let Some(&existing_slot) = identity_map.get(&identity) {
            if let Some(frame_rx) = bevy_system.frame_receiver(existing_slot) {
                clog_info!(
                    "🎨 allocate_bevy_slot: reusing slot {} for '{}' (identity dedup)",
                    existing_slot, &identity[..8.min(identity.len())]
                );
                return Ok(BevySlotAllocation {
                    frame_rx: frame_rx.clone(),
                    slot: existing_slot,
                    // No-op guard for reused slots — original owner manages lifecycle
                    guard: SlotGuard { slot: existing_slot, identity: identity.clone(), released: true },
                });
            }
        }
    }

    // Allocate new slot from pool (with 5s retry for slot availability)
    let mut slot_opt = allocate_slot();
    if slot_opt.is_none() {
        clog_warn!(
            "🎨 allocate_bevy_slot: all slots occupied for '{}', waiting up to 5s...",
            &identity[..8.min(identity.len())]
        );
        for _ in 0..50 {
            std::thread::sleep(std::time::Duration::from_millis(100));
            slot_opt = allocate_slot();
            if slot_opt.is_some() { break; }
        }
    }

    let slot = slot_opt.ok_or_else(|| {
        format!("No Bevy slots available for '{}' after 5s wait", identity)
    })?;

    bevy_system.load_model(slot, vrm_path, &config.display_name, &identity);
    bevy_system.register_identity(&identity, slot);

    let frame_rx = bevy_system.frame_receiver(slot).ok_or_else(|| {
        release_slot(slot);
        format!("frame_receiver failed for slot {}", slot)
    })?;

    clog_info!(
        "🎨 allocate_bevy_slot: slot {} for '{}' (model: {})",
        slot, &identity[..8.min(identity.len())], vrm_path
    );

    Ok(BevySlotAllocation {
        frame_rx: frame_rx.clone(),
        slot,
        guard: SlotGuard::new(slot, identity),
    })
}

// =============================================================================
// Active renderer registry — identity-based deduplication
// =============================================================================

/// Tracks active render loops by identity. Prevents duplicate slot allocation
/// when the same persona identity is registered multiple times (e.g. reconnection race).
struct ActiveRenderer {
    rx: crossbeam_channel::Receiver<RgbaFrame>,
    interval_nanos: Arc<AtomicU64>,
}

static ACTIVE_RENDERERS: OnceLock<std::sync::Mutex<HashMap<String, ActiveRenderer>>> = OnceLock::new();

fn active_renderers() -> &'static std::sync::Mutex<HashMap<String, ActiveRenderer>> {
    ACTIVE_RENDERERS.get_or_init(|| std::sync::Mutex::new(HashMap::new()))
}

// =============================================================================
// Renderer factory
// =============================================================================

/// Create the best available renderer for a persona's avatar.
///
/// Returns (renderer, optional_slot). If a Bevy slot was allocated, the caller
/// MUST call `release_slot(slot)` when the renderer is no longer needed.
///
/// Selection: BevyChannelRenderer (3D VRM) — requires VRM model, Bevy ready, slot available.
/// If no VRM model is specified (human user), returns an error — callers should not
/// request renderers for participants without VRM models.
///
/// This is the single factory function. All renderer selection logic lives here.
pub fn create_renderer(config: AvatarConfig) -> Result<(Box<dyn AvatarRenderer>, u8), String> {
    let vrm_path = config.vrm_model_path.as_ref()
        .ok_or_else(|| format!("No VRM model for '{}'", config.identity))?;

    if !std::path::Path::new(vrm_path).exists() {
        return Err(format!("VRM model not found: {}", vrm_path));
    }

    let bevy_system = crate::live::video::bevy_renderer::get_or_init();
    if !bevy_system.is_ready() {
        return Err(format!("Bevy renderer not ready for '{}'", config.identity));
    }

    // Check if this identity already has a slot — reuse it (prevents duplicate allocation)
    {
        let identity_map = bevy_system.identity_to_slot_map();
        if let Some(&existing_slot) = identity_map.get(&config.identity) {
            if let Some(frame_rx) = bevy_system.frame_receiver(existing_slot) {
                clog_info!(
                    "🎨 Reusing existing slot {} for '{}' (identity dedup)",
                    existing_slot, &config.identity[..8.min(config.identity.len())]
                );
                return Ok((
                    Box::new(BevyChannelRenderer::new(config, frame_rx.clone(), existing_slot)),
                    existing_slot,
                ));
            }
        }
    }

    // Allocate a new slot with retry (slots may be in the process of being released)
    let mut slot_opt = allocate_slot();
    if slot_opt.is_none() {
        clog_warn!(
            "🎨 All Bevy slots occupied for '{}', waiting up to 5s...",
            &config.identity[..8.min(config.identity.len())]
        );
        for _ in 0..50 {
            std::thread::sleep(std::time::Duration::from_millis(100));
            slot_opt = allocate_slot();
            if slot_opt.is_some() { break; }
        }
    }

    let slot = slot_opt.ok_or_else(|| {
        format!("No Bevy slots available for '{}' after 5s wait", config.identity)
    })?;

    bevy_system.load_model(slot, vrm_path, &config.display_name, &config.identity);
    bevy_system.register_identity(&config.identity, slot);

    let frame_rx = bevy_system.frame_receiver(slot).ok_or_else(|| {
        release_slot(slot);
        format!("frame_receiver failed for slot {}", slot)
    })?;

    clog_info!(
        "🎨 Using BevyChannelRenderer for '{}' (slot {}, model: {})",
        &config.identity[..8.min(config.identity.len())], slot, vrm_path
    );

    Ok((Box::new(BevyChannelRenderer::new(config, frame_rx.clone(), slot)), slot))
}

/// Spawns a background thread that renders avatar frames and sends them via channel.
/// Returns a crossbeam receiver that the LiveKit video loop consumes, plus an
/// `Arc<AtomicU64>` that controls the sleep interval in nanoseconds (for FPS adaptation).
///
/// Identity deduplication: if this identity already has an active render loop, returns
/// the existing loop's frame receiver instead of spawning a new one. This prevents
/// duplicate slot allocation from registration races.
///
/// The renderer is selected by `create_renderer()` — the loop is backend-agnostic.
/// Any AvatarRenderer implementation plugs in without touching this code.
///
/// When the consumer drops the receiver, the render thread exits and the Bevy slot
/// is automatically returned to the pool for reuse.
pub fn spawn_renderer_loop(
    config: AvatarConfig,
) -> Option<(crossbeam_channel::Receiver<RgbaFrame>, Arc<AtomicU64>)> {
    let identity = config.identity.clone();

    // Identity dedup: if this identity already has a render loop, reuse it
    {
        let active = active_renderers().lock().unwrap();
        if let Some(existing) = active.get(&identity) {
            clog_info!(
                "🎨 Reusing existing render loop for '{}' (identity dedup)",
                &identity[..8.min(identity.len())]
            );
            return Some((existing.rx.clone(), existing.interval_nanos.clone()));
        }
    }

    // Create renderer (may fail if no VRM model or no slots)
    let (renderer, bevy_slot) = match create_renderer(config.clone()) {
        Ok((r, s)) => (r, s),
        Err(e) => {
            clog_error!(
                "🎨 Failed to create renderer for '{}': {}",
                &identity[..8.min(identity.len())], e
            );
            return None;
        }
    };

    let fps = config.fps;
    let interval_nanos = Arc::new(AtomicU64::new(
        (1_000_000_000.0 / fps) as u64
    ));
    let interval_nanos_clone = interval_nanos.clone();

    let (tx, rx) = crossbeam_channel::bounded(2);

    // Register in active renderers BEFORE spawning the thread
    {
        let mut active = active_renderers().lock().unwrap();
        active.insert(identity.clone(), ActiveRenderer {
            rx: rx.clone(),
            interval_nanos: interval_nanos.clone(),
        });
    }

    let renderer = Arc::new(std::sync::Mutex::new(renderer));
    let identity_for_thread = identity.clone();

    std::thread::Builder::new()
        .name(format!("avatar-renderer-{}", &identity[..8.min(identity.len())]))
        .spawn(move || {
            let mut renderer = renderer.lock().unwrap();
            loop {
                let frame = renderer.render_frame();
                match tx.try_send(frame) {
                    Ok(_) => {}
                    Err(crossbeam_channel::TrySendError::Full(_)) => {
                        // Consumer is slow, drop this frame
                    }
                    Err(crossbeam_channel::TrySendError::Disconnected(_)) => {
                        // Consumer dropped, stop rendering
                        break;
                    }
                }
                let nanos = interval_nanos_clone.load(Ordering::Relaxed);
                std::thread::sleep(std::time::Duration::from_nanos(nanos));
            }
            // Release the Bevy slot back to the pool for reuse
            release_slot(bevy_slot);
            // Remove from active renderers
            {
                let mut active = active_renderers().lock().unwrap();
                active.remove(&identity_for_thread);
            }
            clog_info!("🎨 Render loop exited for '{}'", &identity_for_thread[..8.min(identity_for_thread.len())]);
        })
        .expect("Failed to spawn avatar renderer thread");

    Some((rx, interval_nanos))
}
