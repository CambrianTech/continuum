//! Persona Supervisor — slice 9 of #133.
//!
//! Turns a [`MaterializedPersonaPlan`](super::spawner_module::MaterializedPersonaPlan)
//! (airc identity bootstrapped + inference profile resolved) into a
//! [`HostedPersona`] — a row owning a constructed inference adapter,
//! ready for the per-persona service-loop (slice 10) to drive.
//!
//! ## What this layer owns
//!
//! The supervisor is where **adapter lifetime lives**. One persona →
//! one [`AIProviderAdapter`] today. Future slices change that
//! ownership shape:
//!
//! - **#122 (shared base + LoRA paging)**: the Llama backend
//!   underneath multiple personas is shared; the per-persona view is
//!   a (base_arc + lora_handle) pair, not a full adapter clone.
//!   `materialize_adapters` becomes the place where the supervisor
//!   asks the foundry "is this base already loaded?" before minting a
//!   fresh adapter.
//! - **#108 (cross-grid inference)**: some personas materialize as
//!   `AircRemoteInferenceAdapter` instead of `LlamaCppAdapter`. The
//!   factory trait — not the call site — picks which.
//!
//! Both refinements compose into the `PersonaAdapterFactory` trait
//! below without touching `materialize_adapters` itself. The trait
//! line is where #122 + #108 land their respective decisions.
//!
//! ## Doctrine
//!
//! - [[no-fallbacks-ever]]: each slot is materialized independently;
//!   failures are reported per-row. The supervisor never substitutes
//!   a "default" adapter — a `Err(SupervisorError)` row stays errored
//!   and the operator decides whether to refuse boot.
//! - [[commands-are-dumb-daemons-are-smart]]: the factory trait is
//!   trivial (one `build_adapter` method). Smart routing (which
//!   factory? which backend?) lives in the boot composition above
//!   this layer.
//! - [[intent-driven-api-not-hot-patches]]: callers hand in a
//!   `PersonaInferenceProfile` and get back a ready adapter. No magic
//!   constants, no env-var probes — the profile is the substrate's
//!   declared intent and the adapter materializes that.

use crate::ai::adapter::AIProviderAdapter;
use crate::persona::inference_profile::{InferenceProfileError, PersonaInferenceProfile};
use crate::persona::role_template::RoleId;
use crate::persona::spawner_module::MaterializedPersonaPlan;
use async_trait::async_trait;

/// Polymorphism rail for "given a profile, produce an adapter".
/// Production wiring uses [`LlamaCppPersonaAdapterFactory`]; future
/// slices add `AircRemoteFactory` for grid-routed personas, and
/// `SharedBaseFactory` (#122) for multiple personas riding one base.
///
/// Tests substitute a stub factory so adapter materialization is
/// exercisable without loading a real GGUF.
#[async_trait]
pub trait PersonaAdapterFactory: Send + Sync {
    /// Build an adapter for the given inference profile.
    ///
    /// Errors are surfaced as a free-text message — the caller wraps
    /// them in `SupervisorError::AdapterFactory { slot, role, ... }`
    /// so the operator sees which slot failed without the factory
    /// having to know about slot indices.
    async fn build_adapter(
        &self,
        profile: &PersonaInferenceProfile,
    ) -> Result<Box<dyn AIProviderAdapter>, String>;
}

/// Production factory: hands every profile to
/// `LlamaCppAdapter::for_persona`. Stateless — safe to share via
/// `Arc` across the supervisor's persona materialization loop.
pub struct LlamaCppPersonaAdapterFactory;

#[async_trait]
impl PersonaAdapterFactory for LlamaCppPersonaAdapterFactory {
    async fn build_adapter(
        &self,
        profile: &PersonaInferenceProfile,
    ) -> Result<Box<dyn AIProviderAdapter>, String> {
        let adapter = crate::inference::llamacpp_adapter::LlamaCppAdapter::for_persona(profile)
            .map_err(|e| format!("LlamaCppAdapter::for_persona failed: {e}"))?;
        Ok(Box::new(adapter))
    }
}

/// One row of the supervisor's roster: a persona with airc identity
/// allocated AND a usable inference adapter constructed. Slice 10's
/// per-persona subscribe-loop takes a `Vec<HostedPersona>` and binds
/// each to its room.
pub struct HostedPersona {
    /// Role identity (Helper / Coder / Sentinel / Custom).
    pub role: RoleId,
    /// airc identity allocation result — peer_id, agent_name, home,
    /// default room. Copied from the bootstrap step.
    pub instance: crate::modules::persona_instance_manager::PersonaInstanceInfo,
    /// The inference adapter, ready to receive `generate_text` calls.
    /// Owned by the supervisor; #122 changes this to a shared-base
    /// handle, not a Box.
    pub adapter: Box<dyn AIProviderAdapter>,
}

/// Structured error per failed slot. The two failure modes are:
///
/// - The slice-8 profile resolution already failed (bad model_id,
///   missing GGUF, etc.) — surface as `Profile`.
/// - The profile is fine, but the factory's adapter construction
///   failed (factory rejected the profile, model load failed during
///   `for_persona`, etc.) — surface as `AdapterFactory`.
///
/// Per [[no-fallbacks-ever]] the supervisor never substitutes a
/// default; the operator sees the structured failure with slot +
/// role and decides whether to refuse boot.
#[derive(Debug, thiserror::Error)]
pub enum SupervisorError {
    #[error("slot {slot_index} (role {role:?}): inference profile invalid: {source}")]
    Profile {
        slot_index: usize,
        role: RoleId,
        #[source]
        source: InferenceProfileError,
    },
    #[error("slot {slot_index} (role {role:?}): adapter factory rejected profile: {message}")]
    AdapterFactory {
        slot_index: usize,
        role: RoleId,
        message: String,
    },
}

/// Materialize a roster of `MaterializedPersonaPlan`s into
/// `HostedPersona`s by running each profile through the factory.
///
/// One adapter is built per `Ok` profile; `Err` profiles pass
/// through as `SupervisorError::Profile { ... }`. Factory failures
/// surface as `SupervisorError::AdapterFactory { ... }`. Per
/// [[no-fallbacks-ever]] there is no implicit retry, no substitution,
/// no "default adapter" for failed slots — the row stays errored and
/// the supervisor's caller decides policy.
///
/// Factories MAY be expensive (model load, network handshake to a
/// remote inference peer); the loop is sequential today so the
/// substrate doesn't kick off four ~500 MiB GGUF loads in parallel
/// on an 8 GiB Intel Mac. Slice 10+ can introduce parallel + capped
/// materialization once #122 (shared base) makes the per-persona
/// cost much smaller.
pub async fn materialize_adapters(
    plans: Vec<MaterializedPersonaPlan>,
    factory: &dyn PersonaAdapterFactory,
) -> Vec<Result<HostedPersona, SupervisorError>> {
    let mut out = Vec::with_capacity(plans.len());
    for (slot_index, plan) in plans.into_iter().enumerate() {
        let profile = match plan.profile {
            Ok(p) => p,
            Err(source) => {
                out.push(Err(SupervisorError::Profile {
                    slot_index,
                    role: plan.role,
                    source,
                }));
                continue;
            }
        };
        match factory.build_adapter(&profile).await {
            Ok(adapter) => out.push(Ok(HostedPersona {
                role: plan.role,
                instance: plan.instance,
                adapter,
            })),
            Err(message) => out.push(Err(SupervisorError::AdapterFactory {
                slot_index,
                role: plan.role,
                message,
            })),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::adapter::{AdapterCapabilities, ApiStyle};
    use crate::ai::types::{
        EmbeddingRequest, EmbeddingResponse, HealthStatus, ModelInfo, TextGenerationRequest,
        TextGenerationResponse,
    };
    use crate::modules::persona_instance_manager::PersonaInstanceInfo;
    use crate::persona::hw_tier_descriptor::HwTierCategory;
    use crate::persona::identity_provider::PersonaIdentitySource;
    use crate::persona::inference_profile::{PersonaInferenceProfile, SamplingProfile};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use uuid::Uuid;

    /// Minimal fake adapter — implements just enough of the trait to
    /// satisfy the trait object boundary. None of these methods get
    /// called from `materialize_adapters` itself, so the bodies are
    /// the simplest possible.
    struct FakeAdapter {
        provider_id: String,
    }

    #[async_trait]
    impl AIProviderAdapter for FakeAdapter {
        fn provider_id(&self) -> &str {
            &self.provider_id
        }
        fn name(&self) -> &str {
            "fake"
        }
        fn capabilities(&self) -> AdapterCapabilities {
            AdapterCapabilities::default()
        }
        fn api_style(&self) -> ApiStyle {
            ApiStyle::Local
        }
        fn default_model(&self) -> &str {
            "fake-model"
        }
        async fn initialize(&mut self) -> Result<(), String> {
            Ok(())
        }
        async fn shutdown(&mut self) -> Result<(), String> {
            Ok(())
        }
        async fn generate_text(
            &self,
            _request: TextGenerationRequest,
        ) -> Result<TextGenerationResponse, String> {
            Err("fake adapter does not generate".into())
        }
        async fn create_embedding(
            &self,
            _request: EmbeddingRequest,
        ) -> Result<EmbeddingResponse, String> {
            Err("fake adapter does not embed".into())
        }
        async fn health_check(&self) -> HealthStatus {
            HealthStatus::default()
        }
        async fn get_available_models(&self) -> Vec<ModelInfo> {
            vec![]
        }
    }

    /// Always-succeeds factory — returns a `FakeAdapter` tagged with
    /// the profile's `model_id` so tests can verify each persona got
    /// its own adapter (not one shared instance leaking).
    struct OkFactory {
        builds: AtomicUsize,
    }

    #[async_trait]
    impl PersonaAdapterFactory for OkFactory {
        async fn build_adapter(
            &self,
            profile: &PersonaInferenceProfile,
        ) -> Result<Box<dyn AIProviderAdapter>, String> {
            self.builds.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(FakeAdapter {
                provider_id: profile.model_id.clone(),
            }))
        }
    }

    /// Factory that always rejects — verifies AdapterFactory error
    /// path threading.
    struct ErrFactory;

    #[async_trait]
    impl PersonaAdapterFactory for ErrFactory {
        async fn build_adapter(
            &self,
            _profile: &PersonaInferenceProfile,
        ) -> Result<Box<dyn AIProviderAdapter>, String> {
            Err("simulated factory rejection".into())
        }
    }

    fn fake_instance(name: &str) -> PersonaInstanceInfo {
        PersonaInstanceInfo {
            persona_id: Uuid::new_v4(),
            agent_name: name.to_string(),
            peer_id: Uuid::new_v4(),
            home: PathBuf::from(format!("/tmp/fake-supervisor-test/{name}")),
            default_room: Uuid::nil(),
            source: PersonaIdentitySource::FreshlyMinted,
        }
    }

    fn fake_profile(persona_name: &str, model_id: &str) -> PersonaInferenceProfile {
        PersonaInferenceProfile {
            persona_id: Uuid::new_v4(),
            persona_name: persona_name.to_string(),
            model_id: model_id.to_string(),
            gguf_local_path: Some(PathBuf::from("/tmp/fake.gguf")),
            tier_category: HwTierCategory::Compat,
            tier_id: "mac_intel_metal_discrete".to_string(),
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

    /// Happy path: two materialized plans → two hosted personas. Each
    /// adapter's `provider_id` matches the profile's model_id, proving
    /// the factory ran once per persona (not once with shared state).
    #[tokio::test]
    async fn materializes_one_adapter_per_persona_via_factory() {
        let plans = vec![
            MaterializedPersonaPlan {
                role: RoleId::Helper,
                instance: fake_instance("Paige"),
                profile: Ok(fake_profile("Paige", "model-a")),
            },
            MaterializedPersonaPlan {
                role: RoleId::Coder,
                instance: fake_instance("Pax"),
                profile: Ok(fake_profile("Pax", "model-b")),
            },
        ];

        let factory = OkFactory {
            builds: AtomicUsize::new(0),
        };
        let hosted = materialize_adapters(plans, &factory).await;

        assert_eq!(hosted.len(), 2);
        assert_eq!(factory.builds.load(Ordering::SeqCst), 2);

        let helper = hosted[0].as_ref().expect("Helper hosted");
        assert_eq!(helper.role, RoleId::Helper);
        assert_eq!(helper.instance.agent_name, "Paige");
        assert_eq!(helper.adapter.provider_id(), "model-a");

        let coder = hosted[1].as_ref().expect("Coder hosted");
        assert_eq!(coder.role, RoleId::Coder);
        assert_eq!(coder.instance.agent_name, "Pax");
        assert_eq!(coder.adapter.provider_id(), "model-b");
    }

    /// A row that arrives with `Err(profile)` from slice 8 passes
    /// through as `SupervisorError::Profile` — the factory is NOT
    /// called for it (sibling rows still materialize normally).
    #[tokio::test]
    async fn forwards_profile_errors_without_calling_factory() {
        let bad_profile_err = InferenceProfileError::UnknownModel {
            model_id: "nonexistent/sentinel".to_string(),
            role_id: "coder".to_string(),
        };
        let plans = vec![
            MaterializedPersonaPlan {
                role: RoleId::Helper,
                instance: fake_instance("Paige"),
                profile: Ok(fake_profile("Paige", "model-a")),
            },
            MaterializedPersonaPlan {
                role: RoleId::Coder,
                instance: fake_instance("Pax"),
                profile: Err(bad_profile_err),
            },
        ];

        let factory = OkFactory {
            builds: AtomicUsize::new(0),
        };
        let hosted = materialize_adapters(plans, &factory).await;

        assert_eq!(hosted.len(), 2);
        // Factory called exactly once — for the Ok row only.
        assert_eq!(factory.builds.load(Ordering::SeqCst), 1);
        assert!(hosted[0].is_ok(), "Helper still materializes");
        match &hosted[1] {
            Err(SupervisorError::Profile {
                slot_index,
                role,
                source,
            }) => {
                assert_eq!(*slot_index, 1);
                assert_eq!(*role, RoleId::Coder);
                assert!(matches!(source, InferenceProfileError::UnknownModel { .. }));
            }
            Err(other) => panic!("expected Profile error at slot 1, got {other:?}"),
            Ok(_) => panic!("expected Profile error at slot 1, got Ok"),
        }
    }

    /// Factory rejection surfaces as `SupervisorError::AdapterFactory`
    /// with the slot index + role tagged. Sibling rows don't get
    /// affected when only one fails — the loop continues.
    #[tokio::test]
    async fn factory_rejection_surfaces_as_adapter_factory_error() {
        let plans = vec![MaterializedPersonaPlan {
            role: RoleId::Helper,
            instance: fake_instance("Paige"),
            profile: Ok(fake_profile("Paige", "model-a")),
        }];

        let factory = ErrFactory;
        let hosted = materialize_adapters(plans, &factory).await;

        assert_eq!(hosted.len(), 1);
        match &hosted[0] {
            Err(SupervisorError::AdapterFactory {
                slot_index,
                role,
                message,
            }) => {
                assert_eq!(*slot_index, 0);
                assert_eq!(*role, RoleId::Helper);
                assert!(message.contains("simulated factory rejection"));
            }
            Err(other) => panic!("expected AdapterFactory error, got {other:?}"),
            Ok(_) => panic!("expected AdapterFactory error, got Ok"),
        }
    }

    /// Empty input → empty output. The Vec allocation is sized but
    /// no factory calls fire.
    #[tokio::test]
    async fn empty_plans_yields_empty_hosted() {
        let factory = OkFactory {
            builds: AtomicUsize::new(0),
        };
        let hosted = materialize_adapters(vec![], &factory).await;
        assert!(hosted.is_empty());
        assert_eq!(factory.builds.load(Ordering::SeqCst), 0);
    }
}
