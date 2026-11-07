//! Which submission a work verb reads, as a type: the card's newest, or one she names.
//!
//! What this ends (the M5, 2026-11-07, six hours of the four residents' captures): 19 calls of
//! `work/submission submission_id=latest` refused as "not a usable submission handle", 27 handles
//! refused on cards that had no submission at all, and 7 reviewers refused for naming the parent's
//! submission on the review card they held. "latest" is what she means, so it is a value of the
//! field, and the default; the card she names decides where its submissions live (a review card's
//! are its parent's, the one its review binds); a card with none answers that it has none.

use super::*;
use crate::cognition::work_preconditions::CardRole;

/// A submission as a verb's caller names it: the newest on the card (the default), or one id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SubmissionHandle {
    /// The card's newest submission (airc's `WorkCard::latest_submission`, the one owner of newest); on a review card, the one
    /// its review binds (`WorkBoardProjection::candidate_for_review`).
    #[default]
    Latest,
    /// One submission, by its full id or a unique leading prefix of it.
    Id(Shown<airc_work::SubmissionId>),
}

/// The wire spelling of [`SubmissionHandle::Latest`].
pub const LATEST: &str = "latest";

impl<'de> Deserialize<'de> for SubmissionHandle {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        if raw.trim() == LATEST {
            return Ok(Self::Latest);
        }
        Shown::try_from(raw.as_str()).map(Self::Id).map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for SubmissionHandle {
    fn schema_name() -> String {
        "SubmissionHandle".into()
    }

    // Inline at the field: the two forms are what she reads beside the field's name.
    fn is_referenceable() -> bool {
        false
    }

    fn json_schema(gen: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        use schemars::schema::{Metadata, Schema, SchemaObject, SubschemaValidation};
        let latest = SchemaObject { enum_values: Some(vec![Value::from(LATEST)]), ..Default::default() };
        let id = gen.subschema_for::<Shown<airc_work::SubmissionId>>();
        Schema::Object(SchemaObject {
            metadata: Some(Box::new(Metadata { default: Some(Value::from(LATEST)), ..Default::default() })),
            subschemas: Some(Box::new(SubschemaValidation {
                any_of: Some(vec![Schema::Object(latest), id]),
                ..Default::default()
            })),
            ..Default::default()
        })
    }
}

/// What a read of a card's submission found. `on` is the card that carries it: the card named, or
/// the parent a review card reviews.
#[derive(Debug, Clone, Copy)]
pub enum SubmissionRead<'a> {
    Found { submission: &'a airc_work::WorkSubmission, on: &'a WorkCard },
    /// The card that would carry it has no submission yet: a fact about the card, not a mistake.
    NoneYet { on: &'a WorkCard },
}

/// Why a named submission could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnreadSubmission {
    /// The prefix named none, or several, of the carrying card's submissions.
    Handle(ShownIdError),
    /// A full id that is not among the carrying card's submissions.
    NotOnCard { submission: airc_work::SubmissionId, card: WorkCardId },
    /// The review card's parent is absent from this board.
    ParentAbsent(WorkCardId),
    /// The review card's binding refused the submission named; `parent` is the card it reviews.
    Review { parent: WorkCardId, reason: airc_work::WorkReviewRejectionReason },
}

impl std::fmt::Display for UnreadSubmission {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let shown = airc_core::shown_id::shown_form;
        match self {
            Self::Handle(e) => write!(f, "{e}"),
            Self::NotOnCard { submission, card } => {
                write!(f, "submission {} is not on card {}", shown(submission.as_uuid()), shown(card.as_uuid()))
            }
            Self::ParentAbsent(parent) => write!(f, "the card under review, {}, is absent from this board", shown(parent.as_uuid())),
            Self::Review { reason, .. } => write!(f, "review candidate: {reason}"),
        }
    }
}

impl SubmissionHandle {
    /// The submission this handle names on `card`, read off `board`: the ONE resolver of a
    /// submission handle, for `work/submission` and `work/review` alike. A review card's
    /// submissions are its parent's, and the one its review binds wins; a card with none says so.
    pub fn read<'a>(&self, board: &'a airc_work::WorkBoardProjection, card: &'a WorkCard) -> Result<SubmissionRead<'a>, UnreadSubmission> {
        let role = CardRole::of(card);
        let on = match role {
            CardRole::Code => card,
            CardRole::Review { parent } => board.card(parent).ok_or(UnreadSubmission::ParentAbsent(parent))?,
        };
        let Some(latest) = on.latest_submission() else {
            return Ok(SubmissionRead::NoneYet { on });
        };
        let named = match self {
            Self::Latest => None,
            Self::Id(shown) => Some(shown.resolve(on.submissions.iter().map(|s| s.submission_id)).map_err(UnreadSubmission::Handle)?),
        };
        let submission = match (role, named) {
            (CardRole::Review { parent }, named) => {
                board.candidate_for_review(card.card_id, named).map_err(|reason| UnreadSubmission::Review { parent, reason })?
            }
            (CardRole::Code, None) => latest,
            (CardRole::Code, Some(id)) => on
                .submissions
                .iter()
                .find(|s| s.submission_id == id)
                .ok_or(UnreadSubmission::NotOnCard { submission: id, card: on.card_id })?,
        };
        Ok(SubmissionRead::Found { submission, on })
    }
}

/// A handle goes back on the wire only as a next call names it: `latest`, or an id given in full.
/// A prefix is what she typed against one card's submissions and is never re-rendered.
impl Serialize for SubmissionHandle {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Latest => serializer.serialize_str(LATEST),
            Self::Id(shown) => shown_in_full(shown, serializer),
        }
    }
}

/// A shown id serialized as the full id it names; a prefix refuses to serialize.
pub(super) fn shown_in_full<T: ShownKind, S: serde::Serializer>(shown: &Shown<T>, serializer: S) -> Result<S::Ok, S::Error> {
    match shown.full() {
        Some(id) => serializer.serialize_str(&id.to_uuid().to_string()),
        None => Err(serde::ser::Error::custom("a prefix handle is read once, against its card, and never re-rendered")),
    }
}

/// The same, for an optional shown id (skipped when absent).
pub(super) fn optional_shown_in_full<T: ShownKind, S: serde::Serializer>(shown: &Option<Shown<T>>, serializer: S) -> Result<S::Ok, S::Error> {
    match shown {
        Some(shown) => shown_in_full(shown, serializer),
        None => serializer.serialize_none(),
    }
}

impl std::error::Error for UnreadSubmission {}

/// Why a submission read was refused: the typed cause her refusal carries.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SubmissionRefusal {
    /// The card that would carry it (the parent, for a review card) has no submission yet.
    #[error("card {} has no submission yet", airc_core::shown_id::shown_form(.card.as_uuid()))]
    NoneYet { card: WorkCardId },
    /// The submission she named could not be read.
    #[error(transparent)]
    Unread(UnreadSubmission),
}

impl crate::sdk_codegen::RefusalCause for SubmissionRefusal {
    fn category(&self) -> crate::sdk_codegen::Category {
        use crate::sdk_codegen::Category;
        match self {
            Self::NoneYet { .. } | Self::Unread(UnreadSubmission::NotOnCard { .. } | UnreadSubmission::ParentAbsent(_)) => Category::NotFound,
            Self::Unread(UnreadSubmission::Handle(_) | UnreadSubmission::Review { .. }) => Category::Invalid,
        }
    }
}

const WORK_SUBMIT: crate::sdk_codegen::VerbRef<WorkSubmit> = crate::sdk_codegen::VerbRef::in_hands();
const WORK_GET: crate::sdk_codegen::VerbRef<super::super::WorkGet> = crate::sdk_codegen::VerbRef::in_hands();
const WORK_SUBMISSION: crate::sdk_codegen::VerbRef<WorkSubmission> = crate::sdk_codegen::VerbRef::in_hands();

/// The refusal for a submission she could not read, with the call that moves her on, chosen by
/// cause and never the read that was just refused:
/// - a card with none: `work/submit` when the card is hers to submit (the precondition `work/submit`
///   executes under), else `work/get` on that card;
/// - an id she named that resolved to nothing (`Handle`, `NotOnCard`, or a review binding that
///   refused it): the card's newest, `work/submission` with the handle at its default;
/// - the newest itself refused by the review binding, or a parent absent from this board: `work/get`
///   on the parent, which reads it from any room she belongs to; asking for the newest again
///   would be the same refused read.
///
/// `handle` is what she asked for, `asked` the card she named, `on` the card that carries the
/// submissions (its parent, for a review card).
pub fn submission_refused(
    cause: SubmissionRefusal,
    handle: &SubmissionHandle,
    asked: WorkCardId,
    on: &WorkCard,
    actor: airc_core::PeerId,
    now_ms: u64,
) -> CommandError {
    use crate::cognition::work_preconditions::{check, WorkVerb};
    use crate::sdk_codegen::{NextCall, NextMoves, Refusal};
    let under_review = |parent: WorkCardId| {
        NextCall::new(WORK_GET, &super::super::WorkGetParams { card_id: parent.to_string() }, "the card under review, read from any room you belong to")
    };
    let call = match &cause {
        SubmissionRefusal::NoneYet { .. } if check(WorkVerb::Submit, on, actor, now_ms).is_ok() => NextCall::new(
            WORK_SUBMIT,
            &WorkSubmitParams { card_id: on.card_id.to_string(), ..Default::default() },
            "publishes your checkout's change as this card's submission",
        ),
        SubmissionRefusal::NoneYet { .. } => NextCall::new(
            WORK_GET,
            &super::super::WorkGetParams { card_id: on.card_id.to_string() },
            "the card's state now: its holder has not submitted",
        ),
        SubmissionRefusal::Unread(UnreadSubmission::ParentAbsent(parent)) => under_review(*parent),
        SubmissionRefusal::Unread(UnreadSubmission::Review { parent, .. }) if *handle == SubmissionHandle::Latest => under_review(*parent),
        SubmissionRefusal::Unread(_) => NextCall::new(
            WORK_SUBMISSION,
            &WorkSubmissionParams { card_id: asked.to_string(), ..Default::default() },
            "the card's newest submission",
        ),
    };
    match call {
        Ok(call) => Refusal::new(cause, NextMoves::call(call)).into(),
        Err(unwritable) => CommandError::Internal(unwritable.to_string()),
    }
}
