//! ReloadableModel<T> — a thread-safe container for hot-swappable GPU/CPU models.
//!
//! Replaces `OnceCell<Arc<T>>` statics with a container that supports unloading.
//! When `unload()` is called, the inner Option is set to None, but any in-flight
//! inference holding a cloned Arc<T> continues safely until completion.
//! Memory frees when the last Arc drops.
//!
//! This is the foundation for resource lifecycle management:
//! models load on first call (lazy), unload after configurable idle period,
//! and reload transparently on next use.

use parking_lot::RwLock;
use std::sync::Arc;

/// A thread-safe, reloadable model container.
///
/// Unlike `OnceCell<Arc<T>>` which is set-once-never-cleared,
/// `ReloadableModel` allows setting the inner value to `None`
/// to release memory when models are no longer needed.
///
/// # Safety Properties
/// - `unload()` sets inner to None, but any in-flight callers who already
///   called `get()` hold their own Arc<T> — they keep working.
/// - Memory frees only when the last Arc<T> drops (no dangling references).
/// - `load_with()` is idempotent — if already loaded, returns Ok(false).
/// - All operations are non-blocking (RwLock, not Mutex).
pub struct ReloadableModel<T> {
    inner: RwLock<Option<Arc<T>>>,
    label: &'static str,
}

impl<T> ReloadableModel<T> {
    /// Create a new empty ReloadableModel.
    pub const fn new(label: &'static str) -> Self {
        Self {
            inner: RwLock::new(None),
            label,
        }
    }

    /// Get a clone of the Arc<T> if loaded.
    ///
    /// Returns None if the model is not currently loaded.
    /// The returned Arc<T> is a cheap reference-count bump —
    /// callers can hold it for the duration of their inference
    /// without blocking unload.
    pub fn get(&self) -> Option<Arc<T>> {
        self.inner.read().clone()
    }

    /// Check if the model is currently loaded.
    pub fn is_loaded(&self) -> bool {
        self.inner.read().is_some()
    }

    /// Load the model using the provided factory function.
    ///
    /// Returns `Ok(true)` if the model was loaded, `Ok(false)` if already loaded.
    /// The factory is only called if the model is not currently loaded.
    ///
    /// Uses a write lock to prevent concurrent loading.
    pub fn load_with<E>(&self, f: impl FnOnce() -> Result<T, E>) -> Result<bool, E> {
        let mut guard = self.inner.write();
        if guard.is_some() {
            return Ok(false); // Already loaded
        }
        let model = f()?;
        *guard = Some(Arc::new(model));
        Ok(true)
    }

    /// Unload the model, releasing the container's reference.
    ///
    /// Returns `true` if a model was unloaded, `false` if already empty.
    ///
    /// Any in-flight callers who previously called `get()` still hold
    /// their own Arc<T> and continue working. The underlying T is freed
    /// only when the last Arc drops.
    pub fn unload(&self) -> bool {
        let mut guard = self.inner.write();
        if guard.is_some() {
            *guard = None;
            true
        } else {
            false
        }
    }

    /// The label for this model (used in logging).
    pub fn label(&self) -> &'static str {
        self.label
    }
}

// SAFETY: ReloadableModel is Send + Sync because RwLock<Option<Arc<T>>> is Send + Sync
// when T: Send + Sync. This is automatically derived by the compiler, but we document
// the intent here for clarity.
unsafe impl<T: Send + Sync> Send for ReloadableModel<T> {}
unsafe impl<T: Send + Sync> Sync for ReloadableModel<T> {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_is_empty() {
        let model: ReloadableModel<String> = ReloadableModel::new("test");
        assert!(!model.is_loaded());
        assert!(model.get().is_none());
    }

    #[test]
    fn test_load_and_get() {
        let model = ReloadableModel::new("test");
        let loaded = model
            .load_with(|| Ok::<_, String>("hello".to_string()))
            .unwrap();
        assert!(loaded);
        assert!(model.is_loaded());
        assert_eq!(*model.get().unwrap(), "hello");
    }

    #[test]
    fn test_load_idempotent() {
        let model = ReloadableModel::new("test");
        model
            .load_with(|| Ok::<_, String>("first".to_string()))
            .unwrap();

        // Second load should be a no-op
        let loaded = model
            .load_with(|| Ok::<_, String>("second".to_string()))
            .unwrap();
        assert!(!loaded);
        assert_eq!(*model.get().unwrap(), "first");
    }

    #[test]
    fn test_unload() {
        let model = ReloadableModel::new("test");
        model
            .load_with(|| Ok::<_, String>("data".to_string()))
            .unwrap();
        assert!(model.is_loaded());

        let unloaded = model.unload();
        assert!(unloaded);
        assert!(!model.is_loaded());
        assert!(model.get().is_none());
    }

    #[test]
    fn test_unload_empty_is_false() {
        let model: ReloadableModel<String> = ReloadableModel::new("test");
        assert!(!model.unload());
    }

    #[test]
    fn test_inflight_survives_unload() {
        let model = ReloadableModel::new("test");
        model
            .load_with(|| Ok::<_, String>("important".to_string()))
            .unwrap();

        // Simulate in-flight inference holding an Arc
        let inflight = model.get().unwrap();

        // Unload the model
        model.unload();
        assert!(!model.is_loaded());

        // In-flight reference still works
        assert_eq!(*inflight, "important");
    }

    #[test]
    fn test_reload_after_unload() {
        let model = ReloadableModel::new("test");
        model
            .load_with(|| Ok::<_, String>("v1".to_string()))
            .unwrap();
        model.unload();

        // Reload with new data
        let loaded = model
            .load_with(|| Ok::<_, String>("v2".to_string()))
            .unwrap();
        assert!(loaded);
        assert_eq!(*model.get().unwrap(), "v2");
    }

    #[test]
    fn test_load_with_error() {
        let model: ReloadableModel<String> = ReloadableModel::new("test");
        let result = model.load_with(|| Err::<String, _>("load failed"));
        assert!(result.is_err());
        assert!(!model.is_loaded());
    }

    #[test]
    fn test_label() {
        let model: ReloadableModel<String> = ReloadableModel::new("Whisper RT");
        assert_eq!(model.label(), "Whisper RT");
    }
}
