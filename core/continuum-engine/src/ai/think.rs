//! A generation's think term: what every generation the core makes states about its reasoning,
//! and the terms it is dispatched under. REQUIRED: [`DispatchTerms::think`] is a [`Think`], never an
//! `Option` and never a default, and the terms-less adapter paths take a witness only the engine
//! builds (`adapter::Dispatched`), so a core generation that does not state its think does not
//! compile.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::types::RequestAdmission;

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
/// let _ = continuum_engine::ai::adapter::Dispatched(DispatchTerms::thinking(Think::ModelDefault));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DispatchTerms {
    /// The strict admission guard it was rendered under; `None` = the lane's own window only.
    pub admission: Option<RequestAdmission>,
    /// What its think may be: a mind's turn carries the bound sized from her MEASURED output need
    /// (her grant past her measured answer), so the answer keeps its room.
    pub think: Think,
}

impl DispatchTerms {
    /// Terms that state only a think: no admission beyond the lane's own window.
    pub fn thinking(think: Think) -> Self {
        Self { admission: None, think }
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
