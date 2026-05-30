//! Typed errors for the shared-analysis pipeline.
//!
//! Replaces `Result<T, String>` at the analyze / run_analysis /
//! parse_model_output boundary so callers can pattern-match on the
//! failure mode instead of substring-matching error text. Same shape
//! as `cognition::host_capability_probe::ProbeError` (Joel's standing
//! "typed errors at IPC boundaries" rule, captured in
//! `feedback_two_ironclad_rules_tests_and_fallbacks.md`).
//!
//! ts-rs exports the discriminant + structured fields so the TS side
//! can `switch (err.kind)` rather than parse strings.
//!
//! Variants are deliberately narrow — every site that currently
//! returns a String error maps to exactly ONE variant. Adding a new
//! failure mode means adding a new variant, not stuffing more cases
//! into `Other`. There is no `Other`, no wildcard, no escape hatch.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Why the shared-analysis pipeline returned an error.
///
/// Surface to TS via ts-rs so callers can route on the discriminant.
#[derive(Debug, Clone, Serialize, Deserialize, TS, thiserror::Error)]
#[serde(rename_all = "camelCase", tag = "kind")]
#[ts(
    export,
    export_to = "../../../shared/generated/cognition/AnalysisError.ts"
)]
pub enum AnalysisError {
    /// Model output didn't contain a JSON envelope with the required
    /// `summary` field. Common causes: the model emitted prose only,
    /// truncated mid-output, or wrapped the JSON in a code-fence the
    /// stripper didn't catch. `raw_excerpt` is the leading 200 bytes
    /// of the response so the error log surfaces the actual text the
    /// parser saw.
    #[error("model output had no JSON envelope with 'summary'; got: {raw_excerpt}")]
    MissingEnvelope { raw_excerpt: String },

    /// JSON envelope was found but a required field is missing.
    /// Distinct from MissingEnvelope: at least the structural shape
    /// matched, but the model omitted this field.
    #[error("missing required field '{field}' in model output")]
    MissingField { field: String },

    /// Required field was present but an empty string. Treated as a
    /// failure because empty `summary` would cascade into empty
    /// persona renders downstream.
    #[error("required field '{field}' was empty")]
    EmptyField { field: String },

    /// The inference call itself failed (model unavailable, timeout,
    /// upstream API error, etc.). `reason` is the underlying
    /// provider's error string — opaque from cognition's perspective
    /// because the provider layer has its own typed-error space we
    /// don't want to leak through.
    #[error("inference call failed: {reason}")]
    InferenceFailed { reason: String },
}

impl AnalysisError {
    /// Helper for the inference-call site: wrap the provider's String
    /// error in `InferenceFailed` so the `?` operator does the right
    /// thing in `run_analysis`.
    pub fn from_inference(reason: impl Into<String>) -> Self {
        Self::InferenceFailed {
            reason: reason.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_includes_kind_payload() {
        // Validates the thiserror Display impl — the failure message
        // should include the field/reason so logs are diagnosable
        // without a separate type lookup.
        let err = AnalysisError::MissingField {
            field: "summary".to_string(),
        };
        let msg = err.to_string();
        assert!(
            msg.contains("summary"),
            "expected field name in message: {msg}"
        );
        assert!(
            msg.contains("missing required field"),
            "expected variant context in message: {msg}"
        );
    }

    #[test]
    fn serde_round_trip_preserves_discriminant() {
        // What this catches: ts-rs / serde rename drift between
        // Rust enum variants and TS discriminant tags. If anyone
        // changes `tag = "kind"` to `tag = "type"` or removes
        // `rename_all = "camelCase"`, this test fails — and so does
        // the TS side that reads `err.kind`.
        let err = AnalysisError::EmptyField {
            field: "summary".to_string(),
        };
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("\"kind\":\"emptyField\""), "json was: {json}");
        let round: AnalysisError = serde_json::from_str(&json).unwrap();
        match round {
            AnalysisError::EmptyField { field } => assert_eq!(field, "summary"),
            other => panic!("round-trip changed variant: {other:?}"),
        }
    }

    #[test]
    fn from_inference_helper_wraps_string() {
        let err = AnalysisError::from_inference("model timed out after 30s");
        match err {
            AnalysisError::InferenceFailed { reason } => {
                assert_eq!(reason, "model timed out after 30s");
            }
            other => panic!("expected InferenceFailed, got {other:?}"),
        }
    }
}
