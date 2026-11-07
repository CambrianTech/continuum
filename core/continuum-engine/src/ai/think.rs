//! A generation's think term: what every generation the core makes states about its reasoning,
//! and the terms it is dispatched under. REQUIRED: [`DispatchTerms::think`] is a [`Think`], never an
//! `Option` and never a default, and the terms-less adapter paths take a witness only the engine
//! builds (`adapter::Dispatched`), so a core generation that does not state its think does not
//! compile.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::types::{RequestAdmission, TextGenerationRequest};

/// The terms a generation is DISPATCHED under, beside its request: what its caller decided when it
/// rendered the request, which the adapter enforces at dispatch
/// (`DispatchTermsPolicy::generate_admitted`). Not part of the wire request: an adapter that cannot
/// enforce them refuses typed rather than sending without them. No default: every caller states
/// its think.
///
/// What this catches (Cormac on #5808): a generation whose think was left to a side rule. Terms
/// without a think do not compile:
/// ```compile_fail,E0063
/// let _ = continuum_engine::ai::types::DispatchTerms { admission: None };
/// ```
/// nor does a caller outside the engine build the witness the terms-less adapter paths take, so
/// it cannot reach `generate_text` without stating terms:
/// ```compile_fail,E0423
/// use continuum_engine::ai::types::{DispatchTerms, Think};
/// let terms = DispatchTerms {
///     output: continuum_engine::ai::reply_room::OutputIntent::ProviderControlled,
///     admission: None, think: Think::ModelDefault,
/// };
/// let _ = continuum_engine::ai::adapter::Dispatched(terms);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DispatchTerms {
    pub output: crate::ai::reply_room::OutputIntent,
    /// The strict admission guard it was rendered under; `None` = the lane's own window only.
    pub admission: Option<RequestAdmission>,
    /// What its think may be: a mind's turn carries the bound sized from her MEASURED output need
    /// (her grant past her measured answer), so the answer keeps its room.
    pub think: Think,
}

impl DispatchTerms {
    /// General requests retain their explicit cap, or explicitly leave output to the provider.
    /// A bounded think still requires a caller output cap; no admission authority is inferred.
    pub fn for_request(request: &TextGenerationRequest, think: Think) -> Result<Self, crate::ai::inference_error::InferenceError> {
        if request.max_tokens.is_some() { return Self::fixed(request, think); }
        let terms = Self { output: crate::ai::reply_room::OutputIntent::ProviderControlled, admission: None, think };
        terms.validate_request(request)?;
        Ok(terms)
    }

    pub fn validate_request(&self, request: &TextGenerationRequest) -> Result<(), crate::ai::inference_error::InferenceError> {
        if matches!(self.output, crate::ai::reply_room::OutputIntent::ProviderControlled) && (request.max_tokens.is_some() || self.admission.is_some() || matches!(self.think, Think::Budget(_))) {
            return Err(crate::ai::inference_error::InferenceError::Protocol("provider-controlled generation cannot carry admission or output bounds".into()));
        }
        if let crate::ai::reply_room::OutputIntent::Fixed(reply) = self.output {
            if request.max_tokens != Some(reply.get()) {
                return Err(crate::ai::inference_error::InferenceError::Protocol("fixed dispatch limit disagrees with request".into()));
            }
        }
        Ok(())
    }
    /// Ordinary callers explicitly retain their positive requested limit. A
    /// missing limit is a contract refusal, never a guessed provider allowance.
    pub fn fixed(request: &TextGenerationRequest, think: Think) -> Result<Self, crate::ai::inference_error::InferenceError> {
        let reply = request.max_tokens.and_then(std::num::NonZeroU32::new)
            .ok_or_else(|| crate::ai::inference_error::InferenceError::Protocol("fixed dispatch requires a positive output limit".into()))?;
        Ok(Self { output: crate::ai::reply_room::OutputIntent::Fixed(reply), admission: None, think })
    }
}

/// What a generation's think may be. The one fact the body's thinking levers read
/// (`inference::request_body::apply_think`): a response format, a purpose or a gateway flag never
/// decide it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Think {
    /// Bounded at this budget; zero is the caller's explicit no-think.
    Budget(ThinkBudget),
    /// No think term: the model's own default. An external client's passthrough (the gateway,
    /// `ai/generate` without a `think`), or a core caller stating today's behaviour until its
    /// purpose has a measured budget (card: measured per-purpose think budgets, owner Fable).
    ModelDefault,
}

impl Think {
    /// The budget it carries, if any.
    pub fn budget(self) -> Option<ThinkBudget> {
        match self {
            Self::Budget(budget) => Some(budget),
            Self::ModelDefault => None,
        }
    }

    /// The think a wire request carries (`ai/generate`'s `think`, a grid lane's): absent is the
    /// sender's model default.
    pub fn from_wire(think: Option<ThinkBudget>) -> Self {
        think.map_or(Self::ModelDefault, Self::Budget)
    }
}

/// A think budget in tokens. ZERO is a budget, not "unbounded": it is no-think, said through both
/// levers (`inference::request_body::apply_think`). The template's `enable_thinking = false` holds
/// on every pin (deployed engine 0d7dda6ae, Qwen3.5-0.8B on the IntelMac, ~01:10Z 2026-11-07,
/// Cormac: empty reasoning, 96 tokens of schema JSON, a 56-token answer, finish=stop). The
/// sampler's `reasoning_budget_tokens: 0` holds only from the fork fix on (llama.cpp#95, card
/// c5fb32b7): on pinned 0d7dda6ae a zero budget does not bound the think. Only `-1` (never sent)
/// means unlimited (`common/sampling.cpp`); the budget applies where the model's chat template
/// declares thinking end tags (`server-common.cpp`, `thinking_end_tags`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/ai/ThinkBudget.ts")]
pub struct ThinkBudget(pub u32);
