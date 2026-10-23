//! What a training submission came to, as the genome's own vocabulary: the destination's
//! acceptance receipt, the decision that held a fill without a job ([`Took`]), and the
//! submit outcome the trigger answers with. A persona decodes these from her own submits,
//! and the genome decides them, so they live with the genome; the trigger and job-create
//! commands answer with them.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::genome::competence::GeneRef;
use crate::genome::fine_tuning::types::JobHandle;

/// Present only after this destination has durably accepted the immutable batch.
/// This does not attest that training ran, or deduplicate a different destination.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/training_trigger/AcceptanceReceipt.ts"
)]
pub struct AcceptanceReceipt {
    #[ts(type = "string")]
    pub submission_id: Uuid,
    pub replayed: bool,
}

/// A decision's action that creates no job. The caller (the trigger) keeps the
/// examples in her bucket for every arm: they are the competence's evidence, no gene
/// was trained on them, and the fill after the pending thing settles decides again.
///
/// Deserialize is written by hand ([`TookWire`]): before #4794 (2026-10-05) this was the
/// bare id of the job she joined, and the trigger's durable dispatches
/// (`DispatchPhase::Held { took }`, collection `training_trigger_dispatches`) still hold
/// rows in that shape. Read as the tagged enum alone, every one of them failed with
/// `invalid type: string "cc4f33b7-…", expected internally tagged enum Took`, and on the
/// 5090 (2026-10-10, BigMama) Kimi's credit settlement refused all 13 staged revisions
/// behind it. A bare id reads as [`Took::Joined`], which is what it meant.
#[derive(Debug, Clone, PartialEq, Serialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/genome/Took.ts")]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Took {
    /// `Decision::Join`: a job of hers already trains this competence (its local id).
    Joined {
        #[ts(type = "string")]
        job: uuid::Uuid,
    },
    /// `Decision::Await`: a gene for this competence is on trial in her work; her cards
    /// decide it.
    Awaited {
        #[ts(type = "string")]
        trial: uuid::Uuid,
    },
    /// `Decision::Reuse`: a gene that already existed was adopted for trial, no training.
    Reused {
        #[ts(type = "string")]
        trial: uuid::Uuid,
        gene: GeneRef,
    },
    /// Her trial file could not be read: the bucket holds until it can, since nothing may
    /// dispatch beside a trial nobody can see. Set by the trigger's gate, never by a
    /// decision (job-create refuses with `TrialFileUnreadable` instead).
    TrialFileUnreadable,

    /// `Decision::Nothing { SurpriseLow }`: the room confirms her expectations here; her
    /// memories suffice and no gene is minted while that holds. The bucket keeps filling
    /// and decides again when her surprise rises.
    Unsurprised { s: f32 },
}

/// The wire shapes a [`Took`] is read from: the tagged enum it serializes as, and the
/// bare job id it was before #4794. The tagged half mirrors [`Took`] variant for variant
/// (serde's `remote`-style idiom: the one place the legacy shape is admitted).
#[derive(Deserialize)]
#[serde(untagged)]
enum TookWire {
    Tagged(TookTagged),
    LegacyJoinedJob(uuid::Uuid),
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum TookTagged {
    Joined { job: uuid::Uuid },
    Awaited { trial: uuid::Uuid },
    Reused { trial: uuid::Uuid, gene: GeneRef },
    TrialFileUnreadable,
    Unsurprised { s: f32 },
}

impl From<TookWire> for Took {
    fn from(wire: TookWire) -> Self {
        match wire {
            TookWire::LegacyJoinedJob(job) => Took::Joined { job },
            TookWire::Tagged(TookTagged::Joined { job }) => Took::Joined { job },
            TookWire::Tagged(TookTagged::Awaited { trial }) => Took::Awaited { trial },
            TookWire::Tagged(TookTagged::Reused { trial, gene }) => Took::Reused { trial, gene },
            TookWire::Tagged(TookTagged::TrialFileUnreadable) => Took::TrialFileUnreadable,
            TookWire::Tagged(TookTagged::Unsurprised { s }) => Took::Unsurprised { s },
        }
    }
}

impl<'de> Deserialize<'de> for Took {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        TookWire::deserialize(deserializer).map(Took::from)
    }
}

/// Outcome-as-data extends the legacy envelope with an acceptance receipt.
/// Receipt presence proves destination ownership independently of dispatch success;
/// expected domain/storage refusals retain their typed discriminator.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/training_trigger/SubmitOutcome.ts"
)]
pub struct SubmitOutcome {
    pub success: bool,
    /// Durable ownership receipt, independent of later dispatch success. Absent
    /// on persistence refusal. It attests neither training completion nor cross-node dedupe.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub acceptance: Option<AcceptanceReceipt>,
    /// BatchAppended, JobDispatched, or AlreadyAccepted (absent on rejections).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub outcome: Option<String>,
    /// BatchAppended: examples now pending in the bucket.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub current_count: Option<u32>,
    /// BatchAppended: the bucket's fire threshold.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub threshold: Option<u32>,
    /// JobDispatched: examples carried into the dispatched job.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub examples_used: Option<u32>,
    /// JobDispatched: the provider the coordinator selected.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub selected_provider: Option<String>,
    /// JobDispatched: the created job's handle.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub job_handle: Option<JobHandle>,
    /// BatchAppended after a fill that was HELD: what the examples wait for (a job of
    /// hers already training this competence, a trial judging a gene for it, or the
    /// gene this fill adopted for trial).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub held_by: Option<Took>,
    /// Rejections: the diagnostic message.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<String>,
    /// InconsistentBucket, SubmissionConflict, PersistenceUnavailable,
    /// PersistenceFailed, DispatchFailed, or RecoveryRequired.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error_kind: Option<String>,
}

impl SubmitOutcome {
    pub(crate) fn base(success: bool) -> Self {
        Self {
            success,
            acceptance: None,
            outcome: None,
            current_count: None,
            threshold: None,
            examples_used: None,
            selected_provider: None,
            job_handle: None,
            held_by: None,
            error: None,
            error_kind: None,
        }
    }

    pub(crate) fn batch_appended(current_count: u32, threshold: u32) -> Self {
        Self {
            outcome: Some("BatchAppended".into()),
            current_count: Some(current_count),
            threshold: Some(threshold),
            ..Self::base(true)
        }
    }

    /// A fill held without a job of its own: appended, and the outcome names what the
    /// examples wait for.
    pub(crate) fn batch_held(current_count: u32, threshold: u32, took: Took) -> Self {
        Self { held_by: Some(took), ..Self::batch_appended(current_count, threshold) }
    }

    pub(crate) fn job_dispatched(
        examples_used: u32,
        selected_provider: String,
        job_handle: JobHandle,
    ) -> Self {
        Self {
            outcome: Some("JobDispatched".into()),
            examples_used: Some(examples_used),
            selected_provider: Some(selected_provider),
            job_handle: Some(job_handle),
            ..Self::base(true)
        }
    }

    pub(crate) fn refused(kind: &str, error: String) -> Self {
        Self {
            error: Some(error),
            error_kind: Some(kind.into()),
            ..Self::base(false)
        }
    }
}
