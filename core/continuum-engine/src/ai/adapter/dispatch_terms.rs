//! How each adapter honours a mind's DISPATCH TERMS (`DispatchTerms`: her think budget and the
//! admission her request was rendered under). REQUIRED of every adapter by the compiler: it is a
//! supertrait of [`super::AIProviderAdapter`] with no default, so an adapter that has not decided
//! does not build. Each one either carries the terms or refuses them typed, never sends a turn
//! unbounded:
//!
//! - the managed engine (`OpenAICompatibleAdapter`) puts the think budget on the body and
//!   reserves her lease with the engine's exact count at dispatch;
//! - the grid lane (`AircRemoteInferenceAdapter`) forwards the think budget on the wire to a peer
//!   that advertises it (`CapabilityId::RemoteThinkBudget`), and refuses a peer that does not;
//! - an adapter whose requests never open a think (the cloud Anthropic adapter, which sends no
//!   `thinking` block; the heuristic test adapter) keeps any budget trivially and refuses an
//!   admission it cannot enforce ([`generate_without_thinking`]);
//! - the in-process llama adapter has no reasoning-budget sampler in our bindings (the fork's
//!   `common_reasoning_budget_init` is not reached from `llama`), so it refuses a think budget
//!   typed rather than let a turn think unbounded.

use super::GenerationChunk;
use crate::ai::inference_error::InferenceError;
use crate::ai::types::{DispatchTerms, PromptAdmission, TextGenerationRequest, TextGenerationResponse};

/// Generate a mind's turn under the terms her faculty rendered it with ([`DispatchTerms`]),
/// enforced at DISPATCH. No default: every adapter states how it honours them.
#[async_trait::async_trait]
pub trait DispatchTermsPolicy: Send + Sync {
    async fn generate_admitted(
        &self, request: TextGenerationRequest, terms: DispatchTerms,
        sink: tokio::sync::mpsc::UnboundedSender<GenerationChunk>,
    ) -> Result<TextGenerationResponse, InferenceError> {
        terms.validate_request(&request)?;
        let response = self.generate_with_terms(request, terms, sink).await?;
        response.dispatch.verify(terms)?;
        Ok(response)
    }

    async fn generate_with_terms(
        &self,
        request: TextGenerationRequest,
        terms: DispatchTerms,
        sink: tokio::sync::mpsc::UnboundedSender<GenerationChunk>,
    ) -> Result<TextGenerationResponse, InferenceError>;
}

/// The refusal of an admission this adapter cannot enforce (no exact prompt count, no grant to
/// reserve), when the terms carry one; `Ok` when they do not.
pub fn refuse_unenforceable_admission(request: &TextGenerationRequest, terms: &DispatchTerms) -> Result<(), InferenceError> {
    terms.validate_request(request)?;
    match matches!(terms.output, crate::ai::reply_room::OutputIntent::UseAdmittedRoom { .. })
        || terms.admission.is_some_and(|a| a.lease.is_some() || a.priced == PromptAdmission::EngineCounted) {
        true => Err(InferenceError::AdmissionUncounted { model: model_of(request) }),
        false => Ok(()),
    }
}

/// The model a refusal names: the request's, or empty for the adapter's default.
fn model_of(request: &TextGenerationRequest) -> String {
    request.model.clone().unwrap_or_default() // unwrap_or_default: a request naming no model is the adapter's default, said as empty
}

/// The terms of an adapter whose requests never open a think: any think budget is kept (it
/// thinks nothing), an admission it cannot enforce is refused typed.
pub async fn generate_without_thinking<A: super::AIProviderAdapter + ?Sized>(
    adapter: &A,
    request: TextGenerationRequest,
    terms: DispatchTerms,
    sink: tokio::sync::mpsc::UnboundedSender<GenerationChunk>,
) -> Result<TextGenerationResponse, InferenceError> {
    refuse_unenforceable_admission(&request, &terms)?;
    let crate::ai::reply_room::OutputIntent::Fixed(reply) = terms.output else {
        return Err(InferenceError::AdmissionUncounted { model: model_of(&request) });
    };
    let mut response = adapter.generate_stream_checked(request, sink).await?;
    response.dispatch = crate::ai::reply_room::DispatchReceipt::ProviderFixed { reply: reply.get() };
    Ok(response)
}

/// The refusal of a think budget an adapter cannot carry, when the terms carry one.
pub fn refuse_think_budget(request: &TextGenerationRequest, terms: &DispatchTerms) -> Result<(), InferenceError> {
    match terms.think {
        Some(_) => Err(InferenceError::ThinkUnenforceable { model: model_of(request) }),
        None => Ok(()),
    }
}

/// Implement [`DispatchTermsPolicy`] for an adapter whose requests never open a think (a test
/// fixture, a canned adapter): the one generic contract, [`generate_without_thinking`].
#[macro_export]
macro_rules! dispatch_without_thinking {
    ($adapter:ty) => {
        #[async_trait::async_trait]
        impl $crate::ai::adapter::DispatchTermsPolicy for $adapter {
            async fn generate_with_terms(
                &self,
                request: $crate::ai::types::TextGenerationRequest,
                terms: $crate::ai::types::DispatchTerms,
                sink: tokio::sync::mpsc::UnboundedSender<$crate::ai::adapter::GenerationChunk>,
            ) -> Result<$crate::ai::types::TextGenerationResponse, $crate::ai::inference_error::InferenceError> {
                $crate::ai::adapter::generate_without_thinking(self, request, terms, sink).await
            }
        }
    };
}

// The heuristic test adapter answers canned text: it never thinks. It exists only under the
// test-fixtures gate its module carries, so its policy does too.
#[cfg(any(test, feature = "test-fixtures"))]
dispatch_without_thinking!(crate::ai::heuristic_adapter::HeuristicInferenceAdapter);
// The cloud Anthropic adapter sends no `thinking` block, so its turns never think.
dispatch_without_thinking!(crate::ai::anthropic_adapter::AnthropicAdapter);

/// The in-process llama adapter: no reasoning-budget sampler reaches our bindings, so a think
/// budget is refused typed (the turn is never sent unbounded); an admission likewise.
#[async_trait::async_trait]
impl DispatchTermsPolicy for crate::inference::llamacpp_adapter::LlamaCppAdapter {
    async fn generate_with_terms(
        &self,
        request: TextGenerationRequest,
        terms: DispatchTerms,
        sink: tokio::sync::mpsc::UnboundedSender<GenerationChunk>,
    ) -> Result<TextGenerationResponse, InferenceError> {
        refuse_think_budget(&request, &terms)?;
        generate_without_thinking(self, request, terms, sink).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::types::{RequestAdmission, ThinkBudget};

    fn request() -> TextGenerationRequest {
        TextGenerationRequest { model: Some("m".into()), max_tokens: Some(32), ..Default::default() }
    }

    #[test]
    fn output_authority_is_explicit_and_cannot_disagree_with_fixed_wire() {
        let mut request = request();
        let fixed = DispatchTerms::fixed(&request, None).expect("positive fixture limit");
        assert!(fixed.validate_request(&request).is_ok());
        request.max_tokens = Some(31);
        assert!(fixed.validate_request(&request).is_err());
        request.max_tokens = None;
        assert!(DispatchTerms::fixed(&request, None).is_err());
        request.max_tokens = Some(0);
        assert!(DispatchTerms::fixed(&request, None).is_err());
        let adaptive = DispatchTerms {
            output: crate::ai::reply_room::OutputIntent::UseAdmittedRoom {
                required_reply: std::num::NonZeroU32::new(1).expect("positive fixture claim"),
            }, admission: None, think: None,
        };
        assert!(matches!(refuse_unenforceable_admission(&request, &adaptive), Err(InferenceError::AdmissionUncounted { .. })));
    }

    // what this catches: the generic contract once. An adapter that never thinks keeps any think
    // budget and generates; an admission it cannot enforce (a grant lease, a price below the
    // estimate) is refused typed before anything is generated.
    #[test]
    fn an_unthinking_adapter_keeps_a_budget_and_refuses_an_admission_it_cannot_enforce() {
        let think = DispatchTerms { output: crate::ai::reply_room::OutputIntent::Fixed(std::num::NonZeroU32::new(32).expect("fixture limit")), think: Some(ThinkBudget(0)), admission: None };
        assert_eq!(refuse_unenforceable_admission(&request(), &think), Ok(()));
        let counted = DispatchTerms {
            admission: Some(RequestAdmission { window: 4_096, lease: None, priced: PromptAdmission::EngineCounted }),
            ..think
        };
        assert_eq!(refuse_unenforceable_admission(&request(), &counted), Err(InferenceError::AdmissionUncounted { model: "m".into() }));
    }

    // what this catches: the in-process llama adapter's difference. Its bindings carry no
    // reasoning budget, so any think budget (zero included) is refused typed, never sent unbounded.
    #[tokio::test]
    async fn the_in_process_adapter_refuses_a_think_budget_typed() {
        // Never loaded: the refusal comes before any model is touched.
        let adapter = crate::inference::llamacpp_adapter::LlamaCppAdapter::for_launch(&crate::inference::llamacpp_adapter::LlamaCppLaunch {
            model_id: "m".into(),
            gguf_path: std::path::PathBuf::from("/nonexistent/never-loaded.gguf"),
            context_length: 2048,
            n_seq_max: 1,
            n_ubatch: 512,
            n_gpu_layers: 0,
        });
        let (sink, _rx) = tokio::sync::mpsc::unbounded_channel();
        for think in [ThinkBudget(0), ThinkBudget(338)] {
            let terms = DispatchTerms { output: crate::ai::reply_room::OutputIntent::Fixed(std::num::NonZeroU32::new(32).expect("fixture limit")), think: Some(think), admission: None };
            let refused = adapter.generate_admitted(request(), terms, sink.clone()).await;
            assert_eq!(refused.err(), Some(InferenceError::ThinkUnenforceable { model: "m".into() }), "{think:?}");
        }
    }
}
