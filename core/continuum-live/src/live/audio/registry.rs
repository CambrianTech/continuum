//! The parent that owns a kind's backends: registered in priority order, one active, and released
//! together. Speech-to-text and text-to-speech each hold one; nothing here knows which.

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;

use super::backend::Backend;
use crate::{clog_info, clog_warn};

/// No backend of that name is registered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("no {kind} backend named '{name}' is registered")]
pub struct NotRegistered {
    pub kind: &'static str,
    pub name: &'static str,
}

/// A kind's backends: registration order is priority order (first = tried first), the first
/// registered is active until another is chosen.
pub struct Registry<B: ?Sized + Backend> {
    kind: &'static str,
    backends: HashMap<&'static str, Arc<B>>,
    priority: Vec<&'static str>,
    active: Option<&'static str>,
}

impl<B: ?Sized + Backend> Registry<B> {
    /// An empty registry for one kind (`"STT"`, `"TTS"`), the name its logs use.
    pub fn new(kind: &'static str) -> Self {
        Self { kind, backends: HashMap::new(), priority: Vec::new(), active: None }
    }

    pub fn register(&mut self, backend: Arc<B>) {
        let name = backend.identity().name;
        clog_info!("{}: Registering adapter '{}'", self.kind, name);
        self.backends.insert(name, backend);
        self.priority.push(name);
        if self.active.is_none() {
            self.active = Some(name);
        }
    }

    pub fn set_active(&mut self, name: &'static str) -> Result<(), NotRegistered> {
        if !self.backends.contains_key(name) {
            return Err(NotRegistered { kind: self.kind, name });
        }
        self.active = Some(name);
        clog_info!("{}: Active adapter set to '{}'", self.kind, name);
        Ok(())
    }

    pub fn get_active(&self) -> Option<Arc<B>> {
        self.active.and_then(|name| self.backends.get(name)).cloned()
    }

    pub fn get(&self, name: &str) -> Option<Arc<B>> {
        self.backends.get(name).cloned()
    }

    /// Every backend in priority order, with whether it is loaded.
    pub fn list(&self) -> Vec<(&'static str, bool)> {
        self.priority.iter().filter_map(|name| self.backends.get(name).map(|backend| (*name, backend.is_initialized()))).collect()
    }

    /// Whether the active backend is loaded.
    pub fn is_initialized(&self) -> bool {
        self.get_active().is_some_and(|backend| backend.is_initialized())
    }

    pub fn len(&self) -> usize {
        self.backends.len()
    }

    pub fn is_empty(&self) -> bool {
        self.backends.is_empty()
    }

    /// The loaded backends, in priority order.
    fn loaded(&self) -> Vec<Arc<B>> {
        self.priority.iter().filter_map(|name| self.backends.get(name)).filter(|backend| backend.is_initialized()).cloned().collect()
    }
}

/// Release every loaded backend of a registry, each reporting its own outcome. The registry's lock
/// is held only to collect them, never across a release.
pub async fn release_loaded<B: ?Sized + Backend>(registry: &RwLock<Registry<B>>) -> Vec<&'static str> {
    let (kind, loaded) = {
        let registry = registry.read();
        (registry.kind, registry.loaded())
    };
    let mut released = Vec::with_capacity(loaded.len());
    for backend in loaded {
        let name = backend.identity().name;
        match backend.shutdown().await {
            Ok(()) => {
                clog_info!("{}: '{}' released", kind, name);
                released.push(name);
            }
            Err(failure) => clog_warn!("{}: '{}' release failed: {}", kind, name, failure),
        }
    }
    released
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use continuum_base::part::{Identity, Part};
    use std::sync::atomic::{AtomicBool, Ordering};

    /// A backend that loads and releases nothing heavy: whether it is loaded is all it holds.
    struct Fixture {
        name: &'static str,
        loaded: AtomicBool,
    }

    impl Fixture {
        fn new(name: &'static str, loaded: bool) -> Arc<Self> {
            Arc::new(Self { name, loaded: AtomicBool::new(loaded) })
        }
    }

    impl Part for Fixture {
        fn identity(&self) -> Identity {
            Identity { name: self.name, description: "a test backend" }
        }
    }

    #[async_trait]
    impl Backend for Fixture {
        type Error = std::io::Error;

        fn is_initialized(&self) -> bool {
            self.loaded.load(Ordering::Relaxed)
        }

        async fn initialize(&self) -> Result<(), Self::Error> {
            self.loaded.store(true, Ordering::Relaxed);
            Ok(())
        }

        async fn shutdown(&self) -> Result<(), Self::Error> {
            self.loaded.store(false, Ordering::Relaxed);
            Ok(())
        }
    }

    // what this catches: backends listed or tried in an arbitrary (hash) order instead of their
    // priority, the first registered not active, or an unknown name accepted as active.
    #[test]
    fn registration_order_is_priority_and_the_first_is_active() {
        let mut registry = Registry::<Fixture>::new("TEST");
        assert!(registry.get_active().is_none() && !registry.is_initialized());
        for name in ["local", "cloud", "stub"] {
            registry.register(Fixture::new(name, false));
        }
        assert_eq!(registry.list(), vec![("local", false), ("cloud", false), ("stub", false)]);
        assert_eq!(registry.get_active().map(|b| b.identity().name), Some("local"));
        assert!(registry.set_active("cloud").is_ok());
        assert_eq!(registry.get_active().map(|b| b.identity().name), Some("cloud"));
        assert_eq!(registry.set_active("nowhere"), Err(NotRegistered { kind: "TEST", name: "nowhere" }));
    }

    // what this catches: a release walk that touches backends that hold nothing, misses a loaded
    // one, or names one it did not release.
    #[tokio::test]
    async fn release_names_and_releases_only_the_loaded() {
        let mut registry = Registry::<Fixture>::new("TEST");
        let (loaded, idle, also) = (Fixture::new("loaded", true), Fixture::new("idle", false), Fixture::new("also", true));
        for backend in [&loaded, &idle, &also] {
            registry.register(Arc::clone(backend));
        }
        let registry = RwLock::new(registry);
        assert_eq!(release_loaded(&registry).await, vec!["loaded", "also"]);
        assert!(!loaded.is_initialized() && !also.is_initialized());
        assert!(release_loaded(&registry).await.is_empty(), "nothing loaded is left to release");
    }
}
