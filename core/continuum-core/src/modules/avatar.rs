//! Avatar Module — Bevy 3D avatar snapshots for profile pictures.
//!
//! Allocates a temporary Bevy render slot, loads the persona's VRM model,
//! waits for a clean frame, encodes it as PNG, and saves to disk.
//! The resulting file is served by the HTTP server at `/avatars/{identity}.png`.

use crate::live::avatar::catalog::avatar_model_path;
use crate::live::avatar::frame::AvatarConfig;
use crate::live::avatar::render_loop::allocate_bevy_slot;
use crate::live::avatar::selection::select_avatar_by_identity;
use crate::log_info;
use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use async_trait::async_trait;
use serde_json::Value;
use std::any::Any;
use tracing::info as trace_info;

pub struct AvatarModule;

impl Default for AvatarModule {
    fn default() -> Self {
        Self
    }
}

impl AvatarModule {
    pub fn new() -> Self {
        Self
    }

    /// Blocking snapshot capture — runs on spawn_blocking thread.
    ///
    /// `pub(crate)`: both the `avatar/snapshot` command body (in `commands/avatar.rs`)
    /// and the module's `tick()` auto-refresh drive it. The Bevy-render domain logic
    /// stays here in the module; the command orchestrates the cache check + threading.
    pub(crate) fn capture_snapshot(
        identity: &str,
        width: u32,
        height: u32,
        avatar_dir: &std::path::Path,
        // #174: the persona's PINNED VRM path (durable, sticky). `Some` → render THIS
        // model directly, bypassing the roster-dependent selection that thrashes to a
        // default when the in-memory gender roster is cold. `None` → deterministic
        // selection (correct when warm).
        pinned_vrm: Option<std::path::PathBuf>,
        // Glass box (#172): optional expression/pose to render instead of the idle
        // neutral face; `out_stem` is the state-suffixed output filename (no `.png`).
        expression: Option<crate::live::video::bevy_renderer::Emotion>,
        pose: Option<crate::live::video::bevy_renderer::Gesture>,
        // Mouth openness weight 0.0..1.0 (viseme/lip-sync glass box). None → resting.
        mouth: Option<f32>,
        out_stem: &str,
    ) -> Result<String, String> {
        // Pinned VRM wins (sticky, #174); else fall back to the deterministic selection.
        let vrm_path = match pinned_vrm {
            Some(p) => p,
            None => avatar_model_path(select_avatar_by_identity(identity).filename),
        };

        if !vrm_path.exists() {
            return Err(format!("VRM model not found: {}", vrm_path.display()));
        }

        let vrm_path_str = vrm_path.to_string_lossy().to_string();
        log_info!(
            "module",
            "avatar",
            "Capturing avatar snapshot for '{}' ({}x{}) from {}",
            identity,
            width,
            height,
            &vrm_path_str
        );

        let config = AvatarConfig {
            identity: identity.to_string(),
            display_name: identity.to_string(),
            width,
            height,
            fps: 15.0,
            vrm_model_path: Some(vrm_path_str),
            preference: Default::default(),
        };

        // Allocate a Bevy render slot
        let allocation = allocate_bevy_slot(config)?;

        // Phase 1 — warm up. A COLD VRM load emits no frames until the model is parsed
        // and the scene is instantiated (SceneInstanceReady) — observed ~15s on a 21MB
        // VRM before the first healthy frame, then a warmup. 30s covers cold load +
        // warmup; the result is cached so this wait is paid once per (avatar, state).
        // Capture NOTHING here — just drain until the avatar is clearly loaded, so the
        // expression/pose in phase 2 has a spawned entity to land on.
        let mut frames_received = 0u32;
        let max_wait = std::time::Duration::from_secs(30);
        let start = std::time::Instant::now();
        while start.elapsed() < max_wait {
            while let Ok(_frame) = allocation.frame_rx.try_recv() {
                frames_received += 1;
            }
            if frames_received > 40 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }

        // Phase 2 — glass box (#172): apply the requested expression/pose to the now-
        // loaded avatar. Issued AFTER warmup because SetEmotion/SetGesture target a
        // spawned entity — before load it would be dropped (mirrors avatar_emote.rs).
        // No-op for a neutral snapshot (both None).
        let has_state = expression.is_some() || pose.is_some() || mouth.is_some();
        if has_state {
            let system = crate::live::video::bevy_renderer::get_or_init();
            if let Some(e) = expression {
                system.set_emotion_by_identity(identity, e, 1.0, 300);
            }
            if let Some(g) = pose {
                system.set_gesture_by_identity(identity, g, 1500);
            }
            if let Some(m) = mouth {
                system.set_mouth_weight_by_identity(identity, m.clamp(0.0, 1.0));
            }
        }

        // Phase 3 — capture the latest frame after a settle window (longer when a
        // ~300ms morph transition was applied, so the expression is fully on).
        let settle = std::time::Duration::from_millis(if has_state { 1200 } else { 200 });
        let settle_start = std::time::Instant::now();
        let mut best_frame = None;
        while start.elapsed() < max_wait {
            while let Ok(frame) = allocation.frame_rx.try_recv() {
                frames_received += 1;
                best_frame = Some(frame);
            }
            if best_frame.is_some() && settle_start.elapsed() >= settle {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }

        let frame = best_frame.ok_or_else(|| {
            format!(
                "No usable frame after {}ms ({frames_received} frames received)",
                start.elapsed().as_millis()
            )
        })?;

        // Derive actual dimensions from data length (readback may differ from requested).
        // Software renderers (llvmpipe) can produce different resolutions than requested.
        let actual_pixels = frame.data.len() / 4;
        let (actual_w, actual_h) = if (frame.width * frame.height) as usize == actual_pixels {
            (frame.width, frame.height)
        } else {
            // Try to infer dimensions from data length using common aspect ratios
            let w = (actual_pixels as f64).sqrt() as u32;
            let h = actual_pixels as u32 / w.max(1);
            if (w * h) as usize == actual_pixels {
                (w, h)
            } else {
                // Last resort: assume 16:9 or square
                let h = ((actual_pixels as f64 / (16.0 / 9.0)).sqrt()) as u32;
                let w = actual_pixels as u32 / h.max(1);
                if (w * h) as usize == actual_pixels {
                    (w, h)
                } else {
                    return Err(format!(
                        "Cannot determine frame dimensions for '{}': {} bytes ({} pixels), reported {}x{}",
                        identity, frame.data.len(), actual_pixels, frame.width, frame.height
                    ));
                }
            }
        };

        log_info!(
            "module",
            "avatar",
            "Got frame {}x{} (reported {}x{}) after {} frames in {}ms",
            actual_w,
            actual_h,
            frame.width,
            frame.height,
            frames_received,
            start.elapsed().as_millis()
        );

        // Encode RGBA → PNG
        let img = image::ImageBuffer::<image::Rgba<u8>, Vec<u8>>::from_raw(
            actual_w, actual_h, frame.data,
        )
        .ok_or("Invalid frame dimensions for image buffer")?;

        // Ensure output directory exists
        std::fs::create_dir_all(avatar_dir)
            .map_err(|e| format!("Failed to create avatar directory: {e}"))?;

        let png_path = avatar_dir.join(format!("{out_stem}.png"));
        img.save(&png_path)
            .map_err(|e| format!("Failed to save PNG: {e}"))?;

        let file_size = std::fs::metadata(&png_path).map(|m| m.len()).unwrap_or(0);

        log_info!(
            "module",
            "avatar",
            "Saved avatar snapshot: {} ({} bytes)",
            png_path.display(),
            file_size
        );

        // SlotGuard drops here via RAII, releasing the Bevy slot back to the pool

        Ok(format!("/avatars/{out_stem}.png"))
    }
}

#[async_trait]
impl ServiceModule for AvatarModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "avatar",
            priority: ModulePriority::Normal,
            command_prefixes: &["avatar/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 2,
            // Avatar auto-refresh disabled in Docker (software Vulkan renderer
            // produces invalid frames that crash ORT via mutex poisoning).
            // Avatars use static fallbacks. Bevy 3D renders work on native GPU.
            // TODO: Re-enable when GPU Vulkan works in Docker containers.
            tick_interval: None,
        }
    }

    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
        log_info!(
            "module",
            "avatar",
            "AvatarModule initialized (auto-refresh every 60s)"
        );
        Ok(())
    }

    async fn handle_command(&self, command: &str, _params: Value) -> Result<CommandResult, String> {
        // `avatar/snapshot` is migrated to the typed registry
        // (`commands/avatar.rs`); the module retains only its tick-driven
        // auto-refresh. Fail loud — no silent legacy fallback.
        Err(format!(
            "avatar command surface is migrated to the typed registry; \
             '{command}' has no legacy handler"
        ))
    }

    async fn handle_event(&self, _event_name: &str, _payload: Value) -> Result<(), String> {
        Ok(())
    }

    async fn tick(&self) -> Result<(), String> {
        // Auto-refresh avatar snapshots for all known personas.
        // Only runs when Bevy is available (headless 3D renderer).
        // Independent of live calls — personas always have a current face.
        // Initialize Bevy on first tick if not already running.
        let bevy_system = crate::live::video::bevy_renderer::get_or_init();
        let ready = bevy_system.is_ready();
        trace_info!("🖼️ Avatar tick: Bevy ready={}", ready);
        if !ready {
            return Ok(());
        }

        let avatar_dir = match dirs::home_dir() {
            Some(h) => h.join(".continuum").join("avatars"),
            None => return Ok(()),
        };

        // Get all persona identities that need avatars.
        // First try allocated personas (populated during live calls).
        // If none allocated yet, scan the avatars directory for existing entries
        // and check known persona names from the catalog.
        let mut identities = crate::live::avatar::get_allocated_identities();

        // If no personas allocated yet, discover from avatar directory
        // (static fallback PNGs exist from seeding — their filenames are the uniqueIds)
        if identities.is_empty() {
            if let Ok(entries) = std::fs::read_dir(&avatar_dir) {
                for entry in entries.flatten() {
                    if let Some(name) = entry.path().file_stem() {
                        let name = name.to_string_lossy().to_string();
                        // Skip UUID-named files (36 chars with dashes)
                        if name.len() < 30 && !name.contains('-') || name.len() < 10 {
                            identities.push(name);
                        }
                    }
                }
            }
        }

        trace_info!("🖼️ Avatar tick: {} identities found", identities.len());
        if identities.is_empty() {
            return Ok(());
        }

        // Find personas whose avatar is a small static fallback (< 100KB)
        // or missing entirely. Real Bevy renders are ~250KB+.
        let mut needs_refresh = Vec::new();

        for identity in &identities {
            let png_path = avatar_dir.join(format!("{identity}.png"));
            let needs_update = if png_path.exists() {
                // Small file = static fallback from seeding, not a real 3D render
                match std::fs::metadata(&png_path) {
                    Ok(meta) => meta.len() < 100_000, // < 100KB = not a Bevy render
                    Err(_) => true,
                }
            } else {
                true
            };

            if needs_update {
                needs_refresh.push(identity.clone());
            }
        }

        if needs_refresh.is_empty() {
            return Ok(());
        }

        trace_info!(
            "🖼️ Auto-refreshing {} avatar snapshots ({} total personas)",
            needs_refresh.len(),
            identities.len()
        );

        // Refresh one per tick (don't overwhelm Bevy with 17 simultaneous renders)
        let identity = &needs_refresh[0];
        let id = identity.clone();
        let dir = avatar_dir.clone();
        // Auto-refresh renders the NEUTRAL profile under `<identity>.png` (no state).
        let result = tokio::task::spawn_blocking(move || {
            Self::capture_snapshot(&id, 480, 480, &dir, None, None, None, None, &id)
        })
        .await;

        match result {
            Ok(Ok(path)) => {
                trace_info!("🖼️ Auto-refreshed avatar: {}", path);
            }
            Ok(Err(e)) => {
                trace_info!("🖼️ Avatar refresh deferred for '{}': {}", identity, e);
            }
            Err(e) => {
                trace_info!("🖼️ Avatar refresh task failed for '{}': {}", identity, e);
            }
        }

        Ok(())
    }

    fn adjusted_priority(&self) -> Option<ModulePriority> {
        None
    }

    async fn shutdown(&self) -> Result<(), String> {
        Ok(())
    }

    fn command_schemas(&self) -> Vec<crate::runtime::CommandSchema> {
        vec![]
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_avatar_module_config() {
        let module = AvatarModule::new();
        let config = module.config();
        assert_eq!(config.name, "avatar");
        assert_eq!(config.command_prefixes, &["avatar/"]);
    }

    // what this catches: avatar/snapshot is migrated to the typed registry, so the
    // legacy handle_command must fail loud (no silent fallback) for any command name.
    #[tokio::test]
    async fn legacy_handle_command_fails_loud() {
        let module = AvatarModule::new();
        let err = module
            .handle_command("avatar/snapshot", serde_json::json!({}))
            .await
            .unwrap_err();
        assert!(err.contains("migrated to the typed registry"));
    }
}
