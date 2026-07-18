//! `cognition/redact-memory` — the SURGICAL exam-hygiene verb: scrub a class of
//! content (a held-out answer key, a leaked secret) out of a persona's engrams
//! WITHOUT dropping the memories.
//!
//! Where [`cognition/forget-context`](super::forget_context) neuralyzes a whole
//! exam episode (blunt: every engram tagged with the exam room's `context_id`),
//! this rewrites each engram in place — it keeps her memory of *having been asked
//! and having answered* and excises only the crib sheet. That is what makes a
//! benchmark a legitimate proctored exam of a *continuously-learning* mind: she
//! keeps her whole autobiography of struggling and getting better across retakes;
//! she just can never memorize the literal answer key
//! ([[benchmarks-are-proctored-exams-of-the-natural-living-persona]]).
//!
//! The proctor holds the held-out answers (it grades against them), so it passes
//! them in — the command never reaches into eval internals to infer them, the
//! same way `forget-context` takes the episode id explicitly rather than guessing
//! it. `redact_secrets` additionally scrubs credential-shaped tokens.
//!
//! `access: Internal` — proctor/host-driven memory hygiene, not a persona
//! toolbelt verb (personas do not rewrite each other's memories).

use std::sync::Arc;

use uuid::Uuid;

use crate::modules::cognition::CognitionState;
use crate::persona::redaction::{
    ExamKeyDetector, RedactionClass, RedactionDetector, RedactionPolicy, SecretDetector,
};
use crate::sdk_codegen::CommandError;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/cognition/RedactMemoryParams.ts")]
#[serde(rename_all = "camelCase")]
pub struct RedactMemoryParams {
    /// Persona whose memory is being scrubbed.
    #[ts(type = "string")]
    pub persona_id: Uuid,

    /// Held-out answer keys to excise wherever they appear in her engrams (each
    /// `EvalTask.expect` the proctor just graded against). Occurrences are
    /// replaced with `[redacted:exam-key]`; the surrounding memory is untouched.
    /// Empty = no exam-key redaction.
    #[serde(default)]
    pub exam_answers: Vec<String>,

    /// Also scrub credential-shaped tokens (API keys) — secrets hygiene.
    #[serde(default)]
    pub redact_secrets: bool,

    /// Minimum answer length to be redactable (shorter answers are too generic
    /// to scrub safely — use `forget-context` for those episodes). Defaults to
    /// `ExamKeyDetector::DEFAULT_MIN_LEN`.
    #[serde(default)]
    #[ts(optional)]
    pub min_answer_len: Option<usize>,
}

/// What was scrubbed, per class, and how much memory remains intact.
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../../protocol/typescript/cognition/RedactMemoryResult.ts")]
#[serde(rename_all = "camelCase")]
pub struct RedactMemoryResult {
    /// Exam-key spans excised across all engrams.
    #[ts(type = "number")]
    pub exam_keys_redacted: usize,
    /// Secret/credential spans excised across all engrams.
    #[ts(type = "number")]
    pub secrets_redacted: usize,
    /// Total spans excised (all classes).
    #[ts(type = "number")]
    pub total_redacted: usize,
    /// Engrams remaining after the pass — she keeps ALL of them (redaction never
    /// drops a memory, only rewrites it). Present for symmetry with
    /// `forget-context`'s `engram_count`, and to confirm nothing was lost.
    #[ts(type = "number")]
    pub engram_count: usize,
}

crate::action_command! {
    /// Surgically scrub held-out answer keys / secrets out of a persona's
    /// engrams WITHOUT dropping the memories. Proctor-invoked; her experience of
    /// having answered stays, only the crib sheet is excised.
    pub struct RedactMemory { state: Arc<CognitionState> }
    name: "cognition/redact-memory",
    access: Internal,
    params: RedactMemoryParams,
    output: RedactMemoryResult,
    run(this, _ctx, p) => {
        // Build the policy from the requested classes. Fail loud if the caller
        // asked for nothing to do, or asked to scrub exam keys but none were
        // loadable — a silent no-op here would let un-redacted answers survive
        // under the illusion of hygiene ([[fallbacks-are-illegal-fail-loud]]).
        let min_len = p.min_answer_len.unwrap_or(ExamKeyDetector::DEFAULT_MIN_LEN);
        let mut detectors: Vec<Box<dyn RedactionDetector>> = Vec::new();

        if !p.exam_answers.is_empty() {
            let detector = ExamKeyDetector::new(p.exam_answers.iter().cloned(), min_len);
            if detector.answer_count() == 0 {
                return Err(CommandError::Invalid(format!(
                    "redact-memory: {} exam answer(s) supplied but none are redactable \
                     (all shorter than min_answer_len={} or non-ASCII) — nothing would be \
                     scrubbed; use cognition/forget-context to drop the whole episode instead",
                    p.exam_answers.len(),
                    min_len
                )));
            }
            detectors.push(Box::new(detector));
        }
        if p.redact_secrets {
            detectors.push(Box::new(SecretDetector::new()));
        }
        if detectors.is_empty() {
            return Err(CommandError::Invalid(
                "redact-memory: no policy — set redact_secrets or supply exam_answers".to_string(),
            ));
        }
        let policy = RedactionPolicy::new(detectors);

        // Same live-first / IPC-fallback handle resolution as forget-context:
        // LIVE personas register their mind in `persona_workspace::global()`;
        // the CognitionState map is the IPC-era registry. One command, both
        // worlds, fails loud when neither knows the persona. Synchronous
        // request/response over the handle — no polling, no timeout.
        let live = crate::cognition::persona_workspace::global()
            .get(&p.persona_id)
            .and_then(|cycle| cycle.acting().map(|a| a.admission.clone()));
        let admission = match live {
            Some(a) => a,
            None => this
                .state
                .personas
                .get(&p.persona_id)
                .map(|persona| persona.admission.clone())
                .ok_or_else(|| {
                    CommandError::NotFound(format!("No cognition for {}", p.persona_id))
                })?,
        };

        let report = admission.redact(&policy);

        Ok(RedactMemoryResult {
            exam_keys_redacted: report.count(RedactionClass::ExamKey),
            secrets_redacted: report.count(RedactionClass::Secret),
            total_redacted: report.total(),
            engram_count: admission.engram_count(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand};

    // what this catches: the name + access contract. redact-memory is
    // proctor-driven exam hygiene — Internal, never a persona toolbelt verb
    // (personas don't rewrite each other's memories, same as forget-context).
    #[test]
    fn name_and_access_are_the_contract() {
        assert_eq!(RedactMemory::NAME, "cognition/redact-memory");
        assert_eq!(RedactMemory::ACCESS, AccessLevel::Internal);
    }
}
