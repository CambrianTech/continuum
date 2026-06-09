//! [`FineTuningRegistry`] — substrate-wide bag of
//! [`super::FineTuningAdapter`] impls keyed by `provider_id`.
//!
//! Filled at boot from the model registry's provider list +
//! whatever credentials are present in the environment (same shape
//! `AIProviderModule::initialize_adapters` uses for inference
//! adapters). Used by the `genome/job-create` ServiceModule (when
//! that lands in Rust) to pick an adapter per request.
//!
//! Per [[commands-are-dumb-daemons-are-smart]] this registry is a
//! daemon — it carries state. The command (the future Rust
//! `genome/job-create` ServiceModule) stays narrow: it validates,
//! looks up the adapter, dispatches.

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;

use super::adapter::{ArcFineTuningAdapter, FineTuningAdapter};

/// Substrate-wide collection of registered fine-tuning adapters.
///
/// Per provider, exactly one adapter at a time. Re-registering the
/// same `provider_id` replaces the prior entry — used for
/// hot-swapping under test, and for the local Candle trainer
/// rebooting under memory pressure.
#[derive(Default)]
pub struct FineTuningRegistry {
    by_provider: RwLock<HashMap<String, ArcFineTuningAdapter>>,
}

impl FineTuningRegistry {
    /// Empty registry. Boot path (the future Rust `genome/job-create`
    /// ServiceModule) populates it from the model_registry catalog.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert / replace. Returns the previous adapter (if any) so
    /// the caller can `Drop` it on a separate thread if its
    /// shutdown is heavy.
    pub fn register(&self, adapter: ArcFineTuningAdapter) -> Option<ArcFineTuningAdapter> {
        let provider_id = adapter.capabilities().provider_id;
        self.by_provider.write().insert(provider_id, adapter)
    }

    /// Look up an adapter by provider id. Returns `None` if no
    /// adapter for that provider is registered (e.g. credentials
    /// missing). Per [[no-fallbacks-ever]]: the caller surfaces
    /// the absence as a typed error instead of silently picking a
    /// different provider.
    pub fn get(&self, provider_id: &str) -> Option<ArcFineTuningAdapter> {
        self.by_provider.read().get(provider_id).cloned()
    }

    /// Snapshot of currently-registered provider ids. Used by the
    /// `genome/job-create` ServiceModule's help / capability
    /// listing.
    pub fn list(&self) -> Vec<String> {
        self.by_provider.read().keys().cloned().collect()
    }

    /// Number of registered adapters.
    pub fn len(&self) -> usize {
        self.by_provider.read().len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_provider.read().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::fine_tuning::adapter::{FineTuningCapabilities, FineTuningError};
    use crate::genome::fine_tuning::types::{JobHandle, TrainingJobRequest, TrainingStatus};
    use async_trait::async_trait;
    use uuid::Uuid;

    struct NamedStub(&'static str);

    #[async_trait]
    impl FineTuningAdapter for NamedStub {
        fn capabilities(&self) -> FineTuningCapabilities {
            FineTuningCapabilities {
                provider_id: self.0.to_string(),
                supports_lora: true,
                supports_validation: true,
                produces_local_artifact: false,
                supported_base_model_prefixes: vec![],
            }
        }

        async fn create_job(
            &self,
            _request: TrainingJobRequest,
        ) -> Result<JobHandle, FineTuningError> {
            unimplemented!()
        }

        async fn poll(&self, _handle: &JobHandle) -> Result<TrainingStatus, FineTuningError> {
            unimplemented!()
        }

        async fn cancel(&self, _handle: &JobHandle) -> Result<(), FineTuningError> {
            unimplemented!()
        }
    }

    // what this catches: registry.get returns the registered adapter
    // and is keyed by capabilities().provider_id. A future refactor
    // that keys by a different field (a name, a config struct, etc.)
    // would silently break lookup. The Stage B sentinel/escalate
    // experience reminded us how easy it is for a lookup to lie
    // about whether it found anything.
    #[test]
    fn register_and_lookup_by_provider_id() {
        let reg = FineTuningRegistry::new();
        assert!(reg.is_empty());

        assert!(reg.register(Arc::new(NamedStub("openai"))).is_none());
        assert!(reg.register(Arc::new(NamedStub("mistral"))).is_none());

        assert_eq!(reg.len(), 2);
        assert!(reg.get("openai").is_some());
        assert!(reg.get("mistral").is_some());
        assert!(reg.get("anthropic").is_none());

        let mut names = reg.list();
        names.sort();
        assert_eq!(names, vec!["mistral", "openai"]);
    }

    // what this catches: re-registering the same provider id MUST
    // return the prior entry (so callers can shut it down cleanly).
    // A future change that silently rejects duplicates would leave
    // a leaked old adapter around — exactly the "lying about
    // success" pattern Stage B was about removing.
    #[test]
    fn re_register_returns_prior() {
        let reg = FineTuningRegistry::new();
        let first = Arc::new(NamedStub("openai"));
        let second = Arc::new(NamedStub("openai"));

        assert!(reg.register(first.clone()).is_none());
        let displaced = reg
            .register(second.clone())
            .expect("re-register must return prior");
        assert!(Arc::ptr_eq(&displaced, &(first as _)));
    }
}
