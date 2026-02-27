//! RenderBackendRegistry — dispatches renderer creation by model format.
//!
//! Unlike TTS (one active adapter at a time), avatar rendering needs multiple
//! backends active simultaneously: Bevy for VRM, Live2D for .moc3, etc.
//! The registry maps ModelFormat → backend and falls through priority order
//! when multiple backends support the same format.
//!
//! Follows the TTS registry pattern: OnceCell global, HashMap + priority Vec.

use std::collections::HashMap;
use std::sync::Arc;
use once_cell::sync::OnceCell;
use parking_lot::RwLock;
use crate::clog_info;
use super::backend::{RenderBackend, AvatarError, ModelFormat};
use super::renderer::AvatarRenderer;
use super::frame::AvatarConfig;
use super::types::AvatarModel;

/// Registry that dispatches renderer creation by model format.
///
/// Multiple backends can be active simultaneously (Bevy for VRM, Live2D for .moc3).
/// When multiple backends support the same format, the first registered (highest
/// priority) that is initialized wins.
pub struct RenderBackendRegistry {
    backends: HashMap<&'static str, Arc<RwLock<dyn RenderBackend>>>,
    /// Registration order = priority order (first registered = highest priority)
    priority: Vec<&'static str>,
}

impl RenderBackendRegistry {
    pub fn new() -> Self {
        Self {
            backends: HashMap::new(),
            priority: Vec::new(),
        }
    }

    /// Register a backend. Order matters — first registered = highest priority
    /// when multiple backends support the same format.
    pub fn register(&mut self, backend: Arc<RwLock<dyn RenderBackend>>) {
        let name = backend.read().name();
        clog_info!("🎨 Avatar registry: registering backend '{}'", name);
        self.priority.push(name);
        self.backends.insert(name, backend);
    }

    /// Get a backend by name.
    pub fn get(&self, name: &str) -> Option<Arc<RwLock<dyn RenderBackend>>> {
        self.backends.get(name).cloned()
    }

    /// List all registered backends in priority order.
    pub fn list(&self) -> Vec<(&'static str, bool)> {
        self.priority
            .iter()
            .filter_map(|name| {
                self.backends.get(name).map(|b| (*name, b.read().is_initialized()))
            })
            .collect()
    }

    /// Find the best backend for a given model format.
    ///
    /// Iterates in priority order, returns the first initialized backend
    /// that supports the requested format.
    pub fn backend_for_format(&self, format: ModelFormat) -> Option<Arc<RwLock<dyn RenderBackend>>> {
        for name in &self.priority {
            if let Some(backend) = self.backends.get(name) {
                let guard = backend.read();
                if guard.is_initialized() && guard.supported_formats().contains(&format) {
                    return Some(backend.clone());
                }
            }
        }
        None
    }

    /// Create a renderer for a model, dispatching to the appropriate backend.
    ///
    /// 1. Infer format from model filename
    /// 2. Find the best backend for that format
    /// 3. Delegate to backend.create_renderer()
    pub fn create_renderer(
        &self,
        model: &AvatarModel,
        config: &AvatarConfig,
    ) -> Result<Box<dyn AvatarRenderer>, AvatarError> {
        let format = ModelFormat::from_filename(model.filename)
            .ok_or_else(|| AvatarError::UnsupportedFormat(
                format!("Cannot determine format for '{}'", model.filename)
            ))?;

        let backend = self.backend_for_format(format)
            .ok_or_else(|| AvatarError::NotInitialized(
                format!("No initialized backend for format {:?}", format)
            ))?;

        let result = backend.read().create_renderer(model, config);
        result
    }
}

impl Default for RenderBackendRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Global registry (TTS-style OnceCell pattern)
// =============================================================================

static AVATAR_REGISTRY: OnceCell<Arc<RwLock<RenderBackendRegistry>>> = OnceCell::new();

/// Get the global avatar backend registry.
pub fn get_registry() -> Arc<RwLock<RenderBackendRegistry>> {
    AVATAR_REGISTRY.get().cloned().unwrap_or_else(|| {
        init_registry();
        AVATAR_REGISTRY.get().cloned()
            .expect("AVATAR_REGISTRY must be set after init_registry()")
    })
}

/// Initialize the global avatar backend registry with default backends.
pub fn init_registry() {
    use super::backends::{ProceduralBackend, Bevy3DBackend, Live2DBackend};

    AVATAR_REGISTRY.get_or_init(|| {
        let mut reg = RenderBackendRegistry::new();

        // Bevy 3D — highest priority for VRM/glTF (GPU rendering)
        let mut bevy = Bevy3DBackend::new();
        let _ = bevy.initialize(); // non-blocking, checks if Bevy app is ready
        reg.register(Arc::new(RwLock::new(bevy)));

        // Live2D — sprite-sheet compositing for .moc3 / sprite-sheet formats
        let mut live2d = Live2DBackend::new();
        let _ = live2d.initialize(); // always succeeds (pure CPU compositing)
        reg.register(Arc::new(RwLock::new(live2d)));

        // Procedural — universal fallback (always works, zero dependencies)
        let mut procedural = ProceduralBackend::new();
        let _ = procedural.initialize(); // always succeeds
        reg.register(Arc::new(RwLock::new(procedural)));

        clog_info!("🎨 Avatar registry initialized with {} backends", reg.backends.len());
        Arc::new(RwLock::new(reg))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_basics() {
        let reg = RenderBackendRegistry::new();
        assert!(reg.list().is_empty());
        assert!(reg.backend_for_format(ModelFormat::Vrm0x).is_none());
    }

    #[test]
    fn test_registry_with_procedural() {
        use super::super::backends::ProceduralBackend;

        let mut reg = RenderBackendRegistry::new();
        let mut backend = ProceduralBackend::new();
        backend.initialize().unwrap();
        reg.register(Arc::new(RwLock::new(backend)));

        assert_eq!(reg.list().len(), 1);
        assert_eq!(reg.list()[0], ("procedural", true));

        // Procedural doesn't support VRM
        assert!(reg.backend_for_format(ModelFormat::Vrm0x).is_none());
        // Procedural supports StaticImage (universal fallback)
        assert!(reg.backend_for_format(ModelFormat::StaticImage).is_some());
    }

    #[test]
    fn test_registry_with_live2d() {
        use super::super::backends::Live2DBackend;

        let mut reg = RenderBackendRegistry::new();
        let mut backend = Live2DBackend::new();
        backend.initialize().unwrap();
        reg.register(Arc::new(RwLock::new(backend)));

        assert_eq!(reg.list().len(), 1);
        assert_eq!(reg.list()[0], ("live2d", true));

        // Live2D supports its formats
        assert!(reg.backend_for_format(ModelFormat::Live2D).is_some());
        assert!(reg.backend_for_format(ModelFormat::SpriteSheet).is_some());
        // But not VRM
        assert!(reg.backend_for_format(ModelFormat::Vrm0x).is_none());
    }

    #[test]
    fn test_registry_multi_backend_dispatch() {
        use super::super::backends::{ProceduralBackend, Live2DBackend};

        let mut reg = RenderBackendRegistry::new();

        let mut live2d = Live2DBackend::new();
        live2d.initialize().unwrap();
        reg.register(Arc::new(RwLock::new(live2d)));

        let mut procedural = ProceduralBackend::new();
        procedural.initialize().unwrap();
        reg.register(Arc::new(RwLock::new(procedural)));

        assert_eq!(reg.list().len(), 2);

        // Live2D should handle .moc3
        let backend = reg.backend_for_format(ModelFormat::Live2D).unwrap();
        assert_eq!(backend.read().name(), "live2d");

        // SpriteSheet → Live2D (higher priority, registered first)
        let backend = reg.backend_for_format(ModelFormat::SpriteSheet).unwrap();
        assert_eq!(backend.read().name(), "live2d");

        // StaticImage → Procedural (Live2D doesn't support it)
        let backend = reg.backend_for_format(ModelFormat::StaticImage).unwrap();
        assert_eq!(backend.read().name(), "procedural");

        // VRM → none (neither supports it)
        assert!(reg.backend_for_format(ModelFormat::Vrm0x).is_none());
    }
}
