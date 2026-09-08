//! What a turn's generations ACTUALLY did — the receipts credit is traced through.
//!
//! Card 0d51573a. A cycle makes MULTIPLE generation calls, and there is no canonical
//! singular request among them, so a turn's provenance is an ORDERED COLLECTION of
//! receipts rather than one id. `TurnMetrics` aggregates across a cycle and is
//! therefore not a single-request owner; it cannot answer "which call produced this".
//!
//! ## A fault is a RECEIPT, not a missing one
//!
//! The tempting shape is `Option<GenerationReceipt>` — present on success, absent on
//! failure. That is wrong here and the reason is measured: `LlmDeliberationFaculty`
//! accepts `Ok(TextGenerationResponse)` even when `finish_reason` is
//! [`crate::ai::types::FinishReason::Error`], so partial failed text can become a decision (Astra,
//! 2026-09-08). **If a fault were modelled as an absence, a generation that failed and
//! a generation that never happened would be the same value** — and the turn's record
//! would then claim cleaner provenance than the turn actually had. Every dispatched
//! call leaves a receipt; the receipt says which way it went.
//!
//! ## Two ids, and only one of them is the key
//!
//! [`GenerationReceipt::submitted_request_id`] is `TextGenerationRequest::request_id`
//! as SUBMITTED — assigned once at dispatch and shared with the Mind capture's
//! `CycleId`, which is what makes playback and credit join on the same value. Some
//! adapters mint their own id and return it; that one lives in
//! [`Served::provider_request_id`] and **never replaces the submitted key**. Swapping
//! them hands playback an id the capture never saw, and it breaks precisely on a lane
//! substitution — the case the whole card exists for.

// `FinishReason` is deliberately NOT imported here: classification goes through
// `TextGenerationResponse::generation_error()` (#3917), the single shared
// definition of "did this call fail". Matching on the enum directly would grow a
// second, divergent rule.
use crate::ai::types::TextGenerationResponse;

/// ONE dispatched generation, and what became of it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationReceipt {
    /// **THE JOIN KEY.** `TextGenerationRequest::request_id` as submitted, assigned
    /// once at `LlmDeliberationFaculty` dispatch. Never overwritten by anything an
    /// adapter returns.
    pub submitted_request_id: String,
    /// Which way this call went.
    pub outcome: GenerationOutcome,
}

/// Served or faulted — both are outcomes, neither is an absence.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum GenerationOutcome {
    /// The call returned usable output.
    Served {
        /// What ANSWERED — `TextGenerationResponse::model`, not the configured
        /// `ctx.profile.model_id`. A fallback or lane substitution files the example
        /// under a base that never generated it if this is taken from config.
        model: String,
        /// Which adapter served it.
        provider: String,
        /// The provider's own id, when it returned one that differs from the
        /// submitted key. Recorded SEPARATELY and never in place of it.
        provider_request_id: Option<String>,
    },
    /// The call FAULTED. First-class, so a failure is distinguishable from a call
    /// that was never made.
    Faulted {
        /// What went wrong, in the adapter's own words where it gave any.
        detail: String,
        /// Who faulted, when known. A fault can still name its lane, and a fault
        /// whose lane is unknown is a different fact from one that named itself.
        model: Option<String>,
        provider: Option<String>,
    },
}

impl GenerationReceipt {
    /// Build a receipt from a response, classifying on the adapter's own verdict.
    ///
    /// `FinishReason::Error` means the adapter is telling us the call failed even
    /// though it handed back an `Ok`. Taking it at its word here is what stops
    /// partial failed text being recorded as a served generation.
    pub fn from_response(submitted_request_id: impl Into<String>, response: &TextGenerationResponse) -> Self {
        let submitted_request_id = submitted_request_id.into();
        // THE SHARED CLASSIFIER, not a local rule (#3917, canary 261b25fc). It covers
        // BOTH signals — `error` populated, or a bare `finish_reason=Error` — and it
        // is deliberately the ONLY definition of "did this call fail" in the tree.
        // An inline copy here would drift from the faculty that gates on the same
        // question, and the two disagreeing is exactly how a fault gets recorded as a
        // decision (Astra, 2026-09-08).
        let outcome = if let Some(detail) = response.generation_error() {
            GenerationOutcome::Faulted {
                detail: detail.to_string(),
                model: Some(response.model.clone()),
                provider: Some(response.provider.clone()),
            }
        } else {
            GenerationOutcome::Served {
                model: response.model.clone(),
                provider: response.provider.clone(),
                // Only worth recording when it actually differs; equal ids would be
                // the same fact stored twice.
                provider_request_id: (response.request_id != submitted_request_id)
                    .then(|| response.request_id.clone()),
            }
        };
        Self {
            submitted_request_id,
            outcome,
        }
    }

    /// Build a receipt for a call that failed before any response existed — a
    /// transport error, a refusal, a timeout.
    pub fn faulted(submitted_request_id: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            submitted_request_id: submitted_request_id.into(),
            outcome: GenerationOutcome::Faulted {
                detail: detail.into(),
                model: None,
                provider: None,
            },
        }
    }

    /// Did this call produce usable output? Named rather than matched inline so a
    /// call site reads as a decision about provenance, not a pattern match.
    pub fn served(&self) -> bool {
        matches!(self.outcome, GenerationOutcome::Served { .. })
    }

    /// What actually answered, when something did.
    pub fn served_model(&self) -> Option<&str> {
        match &self.outcome {
            GenerationOutcome::Served { model, .. } => Some(model),
            GenerationOutcome::Faulted { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::types::{FinishReason, TextGenerationResponse, UsageMetrics};

    fn response(finish: FinishReason, request_id: &str) -> TextGenerationResponse {
        TextGenerationResponse {
            text: "partial output before the adapter gave up".to_string(),
            finish_reason: finish,
            model: "qwen3.8-27b".to_string(),
            provider: "local".to_string(),
            usage: UsageMetrics::default(),
            response_time_ms: 12,
            request_id: request_id.to_string(),
            content: None,
            tool_calls: None,
            reasoning: None,
            routing: None,
            error: None,
            timing: None,
        }
    }

    fn errored_via_field(request_id: &str, message: &str) -> TextGenerationResponse {
        let mut r = response(FinishReason::Stop, request_id);
        r.error = Some(message.to_string());
        r
    }

    // what this catches: a FAULT recorded as a served generation. LlmDeliberationFaculty
    // accepts Ok(TextGenerationResponse) even when finish_reason=Error, so partial failed
    // text can become a decision (Astra, measured 2026-09-08). If `from_response` ignored
    // finish_reason, that failed call would enter a citizen's provenance as a normal
    // generation — and the turn's record would claim cleaner provenance than the turn had.
    // The fault must be a RECEIPT, not an absence, or a failed call and a call that never
    // happened become the same value.
    #[test]
    fn a_finish_reason_error_is_a_fault_receipt_not_a_served_generation() {
        let errored = GenerationReceipt::from_response("req-submitted", &response(FinishReason::Error, "req-submitted"));
        assert!(
            !errored.served(),
            "finish_reason=Error must not be recorded as served, even though text came back"
        );
        assert_eq!(
            errored.served_model(),
            None,
            "a faulted call served no model — reporting one would credit a lane that failed"
        );
        match &errored.outcome {
            GenerationOutcome::Faulted { model, provider, .. } => {
                assert_eq!(model.as_deref(), Some("qwen3.8-27b"), "a fault still names its lane when known");
                assert_eq!(provider.as_deref(), Some("local"));
            }
            GenerationOutcome::Served { .. } => panic!("classified a finish_reason=Error as Served"),
        }

        // THE SECOND FAULT SIGNAL: an adapter can report failure by populating
        // `error` while finish_reason still reads Stop. Astra named both conditions;
        // implementing only finish_reason would leave this one recorded as served.
        let via_field = GenerationReceipt::from_response(
            "req-submitted",
            &errored_via_field("req-submitted", "upstream refused: context length exceeded"),
        );
        assert!(
            !via_field.served(),
            "error=Some is a fault even when finish_reason reads Stop"
        );
        match &via_field.outcome {
            GenerationOutcome::Faulted { detail, .. } => assert_eq!(
                detail, "upstream refused: context length exceeded",
                "the adapter's own words are kept, not replaced by a generic description"
            ),
            GenerationOutcome::Served { .. } => panic!("error=Some classified as Served"),
        }

        // POSITIVE CONTROL: an ordinary Stop MUST be served. Without this the assertions
        // above would pass identically if `from_response` faulted everything.
        let ok = GenerationReceipt::from_response("req-submitted", &response(FinishReason::Stop, "req-submitted"));
        assert!(ok.served(), "an ordinary completion is a served generation");
        assert_eq!(ok.served_model(), Some("qwen3.8-27b"));
    }

    // what this catches: the provider's id replacing the submitted one. The submitted id
    // is the join key shared with the Mind capture's CycleId; if an adapter that mints its
    // own id overwrote it, playback would be handed a key the capture never saw — and it
    // would break precisely on a lane substitution, the case this card exists for.
    #[test]
    fn a_provider_id_is_recorded_beside_the_submitted_key_never_in_place_of_it() {
        let differing = GenerationReceipt::from_response("req-submitted", &response(FinishReason::Stop, "provider-minted-99"));
        assert_eq!(
            differing.submitted_request_id, "req-submitted",
            "the submitted key survives an adapter that minted its own id"
        );
        match &differing.outcome {
            GenerationOutcome::Served { provider_request_id, .. } => assert_eq!(
                provider_request_id.as_deref(),
                Some("provider-minted-99"),
                "a differing provider id is kept, separately"
            ),
            GenerationOutcome::Faulted { .. } => panic!("Stop should be served"),
        }

        // And when they agree, the same fact is not stored twice.
        let same = GenerationReceipt::from_response("req-submitted", &response(FinishReason::Stop, "req-submitted"));
        match &same.outcome {
            GenerationOutcome::Served { provider_request_id, .. } => assert!(
                provider_request_id.is_none(),
                "an identical provider id is not worth recording twice"
            ),
            GenerationOutcome::Faulted { .. } => panic!("Stop should be served"),
        }
    }
}
