//! L2 — the producer that feeds the training trigger.
//!
//! Every completed live persona turn is unrealized training signal
//! ([[capability-is-driver-plus-genome]]): the `(stimulus → reply)` pair is a
//! `(context, completion)` example the genome loop can learn from. This module is
//! the SEAM that turns one completed live turn into one buffered training example
//! — scored for quality, classified into the right domain bucket, and submitted to
//! the standing `genome/training-trigger`, which auto-fires a `genome/job-create`
//! once a bucket fills ([`DEFAULT_MIN_EXAMPLES`] examples). It is the L2 layer of
//! the dev-task continuous-learning loop (`docs/genome/DEV-TASK-LOOP-CLOSURE-PLAN.md`):
//! the orchestration BREAK 1 — `submit` had zero non-test callers; this is the
//! call.
//!
//! ## Where it hooks (and why NOT a capture sweep)
//!
//! [`produce`] is called synchronously from the live turn-completion path
//! ([`crate::persona::service_loop::serve_persona_loop`], the `Spoke` arm, after
//! the reply is published). That path is structurally distinct from the eval forks
//! ([`crate::cognition::persona_workspace::fork_eval_cycle`] /
//! `drive_to_settle`), which never run the service loop — so the producer can never
//! train on an eval SIMULATION (a metric cheat). A background sweep of the
//! prompt-capture dir was rejected for exactly that reason: eval forks rebuild a
//! cycle with the SAME `persona_id` and write the SAME `<id>.jsonl`, so sweeping
//! captures would fold measurement sims into the training set.
//!
//! ## Capture and submission have different ownership
//!
//! Unlinked speech submission runs on a spawned task. Card-linked live work awaits
//! its periodic and final atomic snapshots so revision order follows the turn;
//! storage failure is reported without changing the persona's decision. The
//! command executor is late-bound (post-#224 [`LateBound`], installed at boot in
//! `ipc/mod.rs`); before install (tests, early boot) production is a logged no-op.
//!
//! ## The quality gate is a data filter, not a cognition heuristic
//!
//! [`MIN_TRAINING_QUALITY`] gates which turns enter the TRAINING corpus — the same
//! role as `dataset.rs` dropping empty/system-only captures. It does not read or
//! steer the persona's behavior or output ([[no-hardcoded-heuristics-to-steer-cognition]]
//! is about puppeting cognition; this is corpus hygiene).

use continuum_base::time::EpochMs;
use std::sync::{Arc, OnceLock};

use serde_json::json;
use uuid::Uuid;

use crate::genome::fine_tuning::types::TrainingExample;
use crate::persona::domain_classifier::{score_interaction_quality, DomainClassifier};
use crate::routing::CallerIdentity;
use crate::runtime::{CommandExecutor, InProcessTransport, LateBound};
use continuum_client::{ClientError, Connection, Transport};

use crate::genome::training_outcome::SubmitOutcome;
use crate::orm::types::{BatchOperation, BatchOperationType};
// `COLLECTION` is an associated const on the OrmEntity trait; the derive puts it
// there, so the trait must be in scope to name it.
use crate::orm::OrmEntity;

pub mod reviewed;
pub mod graded;
use crate::cognition::training_sources::*;
mod stamp;
pub use stamp::{CapturedCredit, ClaimReceipt, CreditRole, OutcomeStamp, ServedProvenance, SettlementOutcome};

mod executor;
pub use executor::*;
mod speech_discipline_trait;
pub use speech_discipline_trait::*;
mod redrive;
mod credit_acceptance_region;
mod settle_slots;
pub use credit_acceptance_region::{CreditAcceptanceRegion, Resolve};
mod claim_following;
pub use claim_following::{ClaimFollowingCapture, OpenCapture};
mod settle_card_credit_bound;
pub use settle_card_credit_bound::*;

#[cfg(test)]
pub(crate) mod tests;

/// The receipt fixtures this module's tests and core's integration tests share.
#[cfg(any(test, feature = "test-fixtures"))]
pub mod test_support {
    pub fn receipt(id: &str) -> crate::ai::provenance::GenerationReceipt {
        crate::ai::provenance::GenerationReceipt::faulted(id, "irrelevant to staging")
    }

    pub fn served_receipt(id: &str, model: &str) -> crate::ai::provenance::GenerationReceipt {
        crate::ai::provenance::GenerationReceipt {
            submitted_request_id: id.into(),
            outcome: crate::ai::provenance::GenerationOutcome::Served {
                model: model.into(),
                provider: "fixture-provider".into(),
                provider_request_id: None,
            },
            capture: None,
            genes: Vec::new(),
            gene_scales: Vec::new(),
            allocation: None,
        }
    }

    /// The call a served receipt records, as the capture holds it: her request and her
    /// response, linked to the receipt by its cursor (empty until one is captured).
    pub fn lived_call(receipt: &crate::ai::provenance::GenerationReceipt) -> crate::genome::fine_tuning::LivedCall {
        use crate::ai::types::{ChatMessage, TextGenerationRequest, TextGenerationResponse};
        crate::genome::fine_tuning::LivedCall {
            capture: receipt.capture.clone().unwrap_or_default(), // unwrap_or_default: a cursorless receipt reads back as no call
            request: TextGenerationRequest {
                messages: vec![ChatMessage::text("user", "card: verify the regression")],
                ..Default::default()
            },
            response: TextGenerationResponse {
                text: format!("read the source before answering ({})", receipt.submitted_request_id),
                finish_reason: crate::ai::FinishReason::ToolUse,
                model: "fixture-model".into(),
                provider: "fixture-provider".into(),
                usage: crate::ai::UsageMetrics::default(),
                response_time_ms: 0,
                request_id: receipt.submitted_request_id.clone(),
                content: None,
                tool_calls: Some(vec![crate::ai::ToolCall {
                    id: receipt.submitted_request_id.clone(),
                    name: "code/read".into(),
                    input: serde_json::json!({"path": format!("src/{}.rs", receipt.submitted_request_id)}),
                }]),
                reasoning: Some(format!("the card names {}; read it before guessing", receipt.submitted_request_id)),
                routing: None,
                error: None,
                timing: None,
            },
        }
    }

    /// A claimed card for the staging tests.
    ///
    // THE FIXTURE MUST CLEAR THE QUALITY GATE ON ITS OWN (the cc34ac0f lesson below):
    // a FAILED outcome scores `0.20 + 0.3*substance`, so only the ≥500-char substance
    // step reaches 0.47 and clears MIN_TRAINING_QUALITY 0.45. With a shorter fixture
    // the gate refuses the turn, `plan` returns `None` for the wrong reason, and the
    // failed-case assertion passes while proving nothing.
    pub fn claimed_card_fixture(
        owner: Option<airc_core::PeerId>,
        claim: Option<airc_work::ClaimId>,
    ) -> airc_work::WorkCard {
        airc_work::WorkCard {
            card_id: airc_work::WorkCardId::new(),
            repo: airc_work::RepoId::new("acme/continuum").expect("valid repo id in fixture"),
            title: "t".to_string(),
            body: None,
            priority: airc_work::Priority::P2,
            lane_id: None,
            state: airc_work::CardState::Claimed,
            owner,
            claim_provenance: None,
            claim_id: claim,
            claim_expires_at_ms: None,
            last_heartbeat_at_ms: None,
            pull_request: None,
            created_by: airc_core::PeerId::new(),
            created_at_ms: 1,
            updated_at_ms: 1,
            reviews: None,
            submissions: Vec::new(),
            last_submission_rejection: None,
        }
    }
}
