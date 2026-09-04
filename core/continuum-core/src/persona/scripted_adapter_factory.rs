//! `ScriptedPersonaAdapterFactory` — system-level closure-based
//! `PersonaAdapterFactory`. Every test, replay path, demo binary, or
//! ad-hoc tooling that needs a non-production adapter factory leases
//! this one struct.
//!
//! Per [[test-fixtures-are-system-primitives]]: one configurable
//! primitive at the system level, not N bespoke factories per test
//! module. Subsumes the pre-#1517 `OkFactory` / `ErrFactory` /
//! `WarmupFailingFactory` (all of which were `#[cfg(test)]` cancer
//! shaped) into one expressive constructor.
//!
//! ## Usage
//!
//! ```ignore
//! use crate::persona::scripted_adapter_factory::ScriptedPersonaAdapterFactory;
//! use crate::ai::heuristic_adapter::HeuristicInferenceAdapter;
//!
//! // Always-succeeds factory with the deterministic heuristic adapter:
//! let f = ScriptedPersonaAdapterFactory::heuristic();
//!
//! // Always-rejects factory:
//! let f = ScriptedPersonaAdapterFactory::always_fails(
//!     "simulated factory rejection",
//! );
//!
//! // Warmup-failure factory (verifies SupervisorError::AdapterWarmup):
//! let f = ScriptedPersonaAdapterFactory::heuristic_with_warmup_failure(
//!     "simulated warmup failure",
//! );
//!
//! // Slow factory (verifies turn_latency captures real elapsed time):
//! let f = ScriptedPersonaAdapterFactory::heuristic_with_delay_ms(80);
//!
//! // Per-profile dynamic behavior (e.g., one profile succeeds, the
//! // other refuses):
//! let f = ScriptedPersonaAdapterFactory::custom(|profile| {
//!     if profile.persona_name == "Pax" {
//!         Err("Pax adapter refused".to_string())
//!     } else {
//!         Ok(Arc::new(HeuristicInferenceAdapter::new()))
//!     }
//! });
//! ```
//!
//! ## Doctrine
//!
//! - [[test-fixtures-are-system-primitives]]: this is THE factory
//!   for non-LlamaCpp persona-adapter materialization paths.
//! - [[no-fallbacks-ever]]: every failure mode is explicit; no
//!   default substitution.
//! - The `builds` counter is part of the system API — tests assert
//!   the supervisor called the factory the expected number of times.

use crate::ai::adapter::AIProviderAdapter;
use crate::ai::heuristic_adapter::HeuristicInferenceAdapter;
use crate::persona::inference_profile::PersonaInferenceProfile;
use crate::persona::supervisor::PersonaAdapterFactory;
use async_trait::async_trait;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

type BuildFn =
    dyn Fn(&PersonaInferenceProfile) -> Result<Arc<dyn AIProviderAdapter>, String> + Send + Sync;

/// Closure-based factory. Public, system-level, ubiquitous.
pub struct ScriptedPersonaAdapterFactory {
    build: Box<BuildFn>,
    builds: AtomicUsize,
}

impl ScriptedPersonaAdapterFactory {
    /// Build with an explicit closure. Use this when no preset fits —
    /// e.g., per-profile dynamic behavior, mixing adapter types,
    /// returning different adapters by role.
    pub fn custom<F>(build: F) -> Self
    where
        F: Fn(&PersonaInferenceProfile) -> Result<Arc<dyn AIProviderAdapter>, String>
            + Send
            + Sync
            + 'static,
    {
        Self {
            build: Box::new(build),
            builds: AtomicUsize::new(0),
        }
    }

    /// Every profile gets a fresh, zero-config
    /// `HeuristicInferenceAdapter`. The pre-#1517 `OkFactory` shape.
    pub fn heuristic() -> Self {
        Self::custom(|_profile| Ok(Arc::new(HeuristicInferenceAdapter::new())))
    }

    /// Heuristic adapter whose `generate_text` sleeps `delay_ms`
    /// before returning. Used by per-turn latency regression tests
    /// to verify the substrate's `turn_latency` metric reflects
    /// actual wall-clock.
    pub fn heuristic_with_delay_ms(delay_ms: u64) -> Self {
        Self::custom(move |_profile| {
            Ok(Arc::new(
                HeuristicInferenceAdapter::new().with_delay_ms(delay_ms),
            ))
        })
    }

    /// Heuristic adapter whose `warmup` returns Err. Used by
    /// supervisor tests to exercise the typed `AdapterWarmup` failure
    /// path per [[no-fallbacks-ever]].
    pub fn heuristic_with_warmup_failure(reason: impl Into<String>) -> Self {
        let reason = reason.into();
        Self::custom(move |_profile| {
            Ok(Arc::new(
                HeuristicInferenceAdapter::new().with_warmup_failure(reason.clone()),
            ))
        })
    }

    /// Always rejects with this message. The pre-#1517 `ErrFactory`
    /// shape — exercises `SupervisorError::AdapterFactory`.
    pub fn always_fails(reason: impl Into<String>) -> Self {
        let reason = reason.into();
        Self::custom(move |_profile| Err(reason.clone()))
    }

    /// Heuristic factory paired with shared warmup + generate counters.
    /// Each adapter built by this factory wires the same counters in
    /// via `HeuristicInferenceAdapter::with_warmup_observer` /
    /// `with_generate_observer`. Tests assert substrate-wide
    /// invocation counts (e.g., "warmup ran N times across N slots")
    /// without bespoke factory state.
    pub fn heuristic_with_counters() -> (Self, ObservedCounts) {
        let counts = ObservedCounts::new();
        let warmup = counts.warmups.clone();
        let generate = counts.generates.clone();
        let factory = Self::custom(move |_profile| {
            Ok(Arc::new(
                HeuristicInferenceAdapter::new()
                    .with_warmup_observer(warmup.clone())
                    .with_generate_observer(generate.clone()),
            ))
        });
        (factory, counts)
    }

    /// How many `build_adapter` calls landed against this factory.
    /// Tests assert exact counts to verify per-slot semantics.
    pub fn build_count(&self) -> usize {
        self.builds.load(Ordering::SeqCst)
    }
}

/// Observable counters returned alongside a factory built by
/// [`ScriptedPersonaAdapterFactory::heuristic_with_counters`]. Per
/// [[test-fixtures-are-system-primitives]] the counts are part of
/// the substrate's testability surface — every test asserting
/// "warmup ran N times" leases this same pair.
#[derive(Debug, Clone)]
pub struct ObservedCounts {
    pub warmups: Arc<AtomicUsize>,
    pub generates: Arc<AtomicUsize>,
}

impl ObservedCounts {
    fn new() -> Self {
        Self {
            warmups: Arc::new(AtomicUsize::new(0)),
            generates: Arc::new(AtomicUsize::new(0)),
        }
    }
    pub fn warmups(&self) -> usize {
        self.warmups.load(Ordering::SeqCst)
    }
    pub fn generates(&self) -> usize {
        self.generates.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl PersonaAdapterFactory for ScriptedPersonaAdapterFactory {
    async fn build_adapter(
        &self,
        profile: &PersonaInferenceProfile,
    ) -> Result<Arc<dyn AIProviderAdapter>, String> {
        self.builds.fetch_add(1, Ordering::SeqCst);
        (self.build)(profile)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persona::hw_tier_descriptor::HwTierCategory;
    use crate::persona::inference_profile::SamplingProfile;
    use uuid::Uuid;

    fn fake_profile(name: &str, model_id: &str) -> PersonaInferenceProfile {
        PersonaInferenceProfile {
            persona_id: Uuid::new_v4(),
            persona_name: name.to_string(),
            model_id: model_id.to_string(),
            gguf_local_path: None,
            tier_category: HwTierCategory::Compat,
            tier_id: "test".to_string(),
            context_length: 2048,
            n_ubatch: 512,
            n_batch: 512,
            n_seq_max: 1,
            n_gpu_layers: 0,
            sampling: SamplingProfile::chat_defaults(),
            chat_template: None,
            stop_sequences: vec![],
        }
    }

    #[tokio::test]
    async fn heuristic_returns_heuristic_adapter_and_counts_builds() {
        let f = ScriptedPersonaAdapterFactory::heuristic();
        assert_eq!(f.build_count(), 0);
        let a1 = f.build_adapter(&fake_profile("Paige", "model-a")).await;
        assert!(a1.is_ok());
        assert_eq!(f.build_count(), 1);
        let a2 = f.build_adapter(&fake_profile("Pax", "model-b")).await;
        assert!(a2.is_ok());
        assert_eq!(f.build_count(), 2);
    }

    #[tokio::test]
    async fn always_fails_returns_typed_error() {
        let f = ScriptedPersonaAdapterFactory::always_fails("nope");
        let result = f.build_adapter(&fake_profile("Paige", "model-a")).await;
        // Don't use `expect_err` — Arc<dyn AIProviderAdapter> doesn't
        // impl Debug, so unwrap_err's panic-message would fail to
        // compile. Pattern-match instead.
        match result {
            Err(msg) => assert!(msg.contains("nope")),
            Ok(_) => panic!("expected Err"),
        }
        assert_eq!(f.build_count(), 1);
    }

    #[tokio::test]
    async fn custom_per_profile_dispatch() {
        let f = ScriptedPersonaAdapterFactory::custom(|profile| {
            if profile.persona_name == "Pax" {
                Err("Pax refused".to_string())
            } else {
                Ok(Arc::new(HeuristicInferenceAdapter::new()))
            }
        });
        assert!(f.build_adapter(&fake_profile("Paige", "m")).await.is_ok());
        assert!(f.build_adapter(&fake_profile("Pax", "m")).await.is_err());
        assert_eq!(f.build_count(), 2);
    }
}
