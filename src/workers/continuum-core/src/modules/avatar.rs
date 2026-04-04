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
use crate::utils::params::Params;
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

    async fn snapshot(&self, params: Value) -> Result<CommandResult, String> {
        let p = Params::new(&params);
        let identity = p.str("identity")?.to_string();
        let width = p.u32_opt("width").unwrap_or(480);
        let height = p.u32_opt("height").unwrap_or(480);

        // Check if snapshot already exists on disk
        let avatar_dir = dirs::home_dir()
            .ok_or("Cannot determine home directory")?
            .join(".continuum")
            .join("avatars");

        let png_path = avatar_dir.join(format!("{identity}.png"));
        let force = p.bool_or("force", false);

        if png_path.exists() && !force {
            log_info!(
                "module",
                "avatar",
                "Avatar snapshot already exists for '{}', returning cached",
                identity
            );
            return Ok(CommandResult::Json(serde_json::json!({
                "path": format!("/avatars/{identity}.png"),
                "cached": true,
            })));
        }

        // Run the blocking Bevy slot allocation + frame capture on a dedicated thread
        let result = tokio::task::spawn_blocking(move || -> Result<String, String> {
            Self::capture_snapshot(&identity, width, height, &avatar_dir)
        })
        .await
        .map_err(|e| format!("Snapshot task panicked: {e}"))?;

        let relative_path = result?;

        Ok(CommandResult::Json(serde_json::json!({
            "path": relative_path,
            "cached": false,
        })))
    }

    /// Blocking snapshot capture — runs on spawn_blocking thread.
    fn capture_snapshot(
        identity: &str,
        width: u32,
        height: u32,
        avatar_dir: &std::path::Path,
    ) -> Result<String, String> {
        // Select avatar model for this identity
        let model = select_avatar_by_identity(identity);
        let vrm_path = avatar_model_path(model.filename);

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

        // Wait for model to load and render clean frames.
        // Skip initial frames (loading/black), then grab a good one.
        let mut best_frame = None;
        let mut frames_received = 0u32;
        let max_wait = std::time::Duration::from_secs(5);
        let start = std::time::Instant::now();

        while start.elapsed() < max_wait {
            // Drain all available frames, keeping the latest
            while let Ok(frame) = allocation.frame_rx.try_recv() {
                frames_received += 1;
                // Skip first ~30 frames (model loading, initial black)
                if frames_received > 30 {
                    best_frame = Some(frame);
                }
            }

            // If we have a good frame after the skip window, we're done
            if best_frame.is_some() && frames_received > 40 {
                break;
            }

            // Wait for next frame notification
            std::thread::sleep(std::time::Duration::from_millis(50));
        }

        let frame = best_frame.ok_or_else(|| {
            format!(
                "No usable frame after {}ms ({frames_received} frames received)",
                start.elapsed().as_millis()
            )
        })?;

        log_info!(
            "module",
            "avatar",
            "Got frame {}x{} after {} frames in {}ms",
            frame.width,
            frame.height,
            frames_received,
            start.elapsed().as_millis()
        );

        // Encode RGBA → PNG
        let img = image::ImageBuffer::<image::Rgba<u8>, Vec<u8>>::from_raw(
            frame.width,
            frame.height,
            frame.data,
        )
        .ok_or("Invalid frame dimensions for image buffer")?;

        // Ensure output directory exists
        std::fs::create_dir_all(avatar_dir)
            .map_err(|e| format!("Failed to create avatar directory: {e}"))?;

        let png_path = avatar_dir.join(format!("{identity}.png"));
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

        Ok(format!("/avatars/{identity}.png"))
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
            // Tick every 60s — refresh stale avatar snapshots when Bevy is running.
            // Independent of live calls. Personas always have a current face.
            tick_interval: Some(std::time::Duration::from_secs(60)),
        }
    }

    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
        log_info!("module", "avatar", "AvatarModule initialized (auto-refresh every 60s)");
        Ok(())
    }

    async fn handle_command(&self, command: &str, params: Value) -> Result<CommandResult, String> {
        match command {
            "avatar/snapshot" => self.snapshot(params).await,
            _ => Err(format!("Unknown avatar command: {command}")),
        }
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
        let result = tokio::task::spawn_blocking(move || {
            Self::capture_snapshot(&id, 480, 480, &dir)
        }).await;

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
}
