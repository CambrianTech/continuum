//! Public submission/review commands on the existing work owner. Artifact
//! references carry content identity; publication does not imply blob verification.

use super::*;
use crate::persona::training_producer::reviewed::{self, SubmissionSelection};
use crate::runtime::{CommandExecutor, InProcessTransport, LateBound};
use continuum_client::Connection;

/// How a room is NAMED in a refusal (Kimi's `work/submit`, 2026-09-21).
///
/// "card 31c241e2 is absent from this room" is TRUE and USELESS: she asked for one room
/// name, `resolve_room` turned it into a Room, and if that landed somewhere other than
/// where she meant, the sentence gives her no way to see the mismatch. She re-reads her
/// own card id, finds it correct, and concludes the substrate lost her card — the
/// substrate's resolution failing, presented as the citizen being wrong about her own work.
///
/// So: the RESOLVER's answer, never the caller's string — and the channel id beside the
/// name, because two rooms can share a name across scopes and the name alone cannot tell
/// them apart (Cormac's condition). The caller's input is reported too, since the gap
/// between what she asked for and what she got IS the diagnosis.
fn room_label(name: &str, channel: Uuid) -> String {
    format!("'{name}' ({})", short8(channel))
}


/// The same content-addressed reference the AIRC submission protocol carries.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/work/WorkArtifactReference.ts"
)]
pub struct WorkArtifactReference {
    /// Artifact SHA-256: 64 hex characters.
    pub hash: String,
    /// Artifact byte length.
    #[ts(type = "number")]
    pub size_bytes: u64,
    /// Optional media type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub mime: Option<String>,
}

impl WorkArtifactReference {
    fn into_artifact(self) -> Result<airc_work::SubmissionArtifact, CommandError> {
        // Feed the owned string directly into the protocol's existing hash
        // decoder; no second hash parser or JSON body round-trip.
        let hash = Deserialize::deserialize(serde::de::value::StringDeserializer::<
            serde::de::value::Error,
        >::new(self.hash))
        .map_err(|e| CommandError::Invalid(format!("artifact hash: {e}")))?;
        Ok(airc_work::SubmissionArtifact {
            hash,
            size_bytes: self.size_bytes,
            mime: self.mime,
        })
    }
}

impl From<&airc_work::SubmissionArtifact> for WorkArtifactReference {
    fn from(value: &airc_work::SubmissionArtifact) -> Self {
        Self {
            hash: value.hash.to_string(),
            size_bytes: value.size_bytes,
            mime: value.mime.clone(),
        }
    }
}

pub struct WorkSubmit {
    pub registry: PersonaAircRuntimeRegistry,
    pub executor_slot: Arc<LateBound<CommandExecutor>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/work/WorkSubmitParams.ts"
)]
pub struct WorkSubmitParams {
    /// The room the card lives in (id or name).
    pub room: String,
    // The card you hold. Everything below is DERIVED from it and your checkout when
    // omitted — the citizen's world has no verb that mints an artifact hash, and
    // before 2026-09-17 every submit she wrote by hand carried zeros and was refused
    // (56 on the M5 in one day; Kimi on the 5090 every turn for a night).
    /// The card you hold.
    #[ts(type = "string")]
    pub card_id: Uuid,
    /// Minted when omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string")]
    pub submission_id: Option<Uuid>,
    /// Your claim on the card; read off the board when omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string")]
    pub claim_id: Option<Uuid>,
    /// Benchmark instance name; read from your checkout when omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub instance: Option<String>,
    /// The commit your patch is against; read from your checkout when omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub base_sha: Option<String>,
    /// SHA-256 + size of your patch; computed when omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub artifact: Option<WorkArtifactReference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string")]
    pub staged_revision_id: Option<Uuid>,
}

/// What a submit is made of when the citizen only names her card: her checkout for
/// it, the base her patch stands on, and the patch's hash — read from her hands, never
/// typed.
struct DerivedSubmission {
    instance: String,
    base_sha: String,
    artifact: WorkArtifactReference,
}

/// The all-zero hash the AI manual's example showed and citizens copied verbatim, or a
/// manual blank (`<string>`) read back as a value.
fn is_placeholder_hash(h: &str) -> bool {
    let t = h.trim();
    t.is_empty() || t.chars().all(|c| c == '0') || t.starts_with('<')
}

/// The checkout her hands are rooted at for THIS card — the one place the patch can be
/// read from. A submit for a card she is not rooted at is refused with the verb that
/// roots her, never guessed from a workspace listing.
fn rooted_checkout_for(persona: Uuid, card_id: Uuid) -> Result<std::path::PathBuf, CommandError> {
    match (
        crate::cognition::persona_workspace::acting_card_of(persona),
        crate::cognition::persona_workspace::acting_root_of(persona),
    ) {
        (Some(card), Some(root)) if card == card_id => Ok(root),
        (Some(card), _) => Err(CommandError::Invalid(format!(
            "your hands are rooted at card {card}, not {card_id} — root at the card you are \
             submitting (work/get {card_id}) and submit again"
        ))),
        _ => Err(CommandError::Invalid(format!(
            "your hands are not rooted at a card's checkout — root at {card_id} first \
             (work/get {card_id}); work/submit reads the patch from that checkout"
        ))),
    }
}

/// `workspace/swe/<instance>` names a benchmark checkout; anything else is a repo
/// worktree and the card id is its instance name.
fn instance_of_checkout(checkout: &std::path::Path, card_id: Uuid) -> Option<String> {
    let parts: Vec<&str> = checkout.iter().filter_map(|c| c.to_str()).collect();
    parts
        .windows(3)
        .find(|w| w[0] == "workspace" && w[1] == "swe")
        .map(|w| w[2].to_string())
        .or_else(|| Some(card_id.to_string()))
}

fn git_stdout(checkout: &std::path::Path, args: &[&str]) -> Option<String> {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(checkout)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}

/// The commit her patch stands on. A benchmark checkout: the instance's base commit
/// from the dataset row. A repo worktree: the merge-base with the remote's default
/// branch (`origin/HEAD`), else the upstream's. Absent = a named refusal, never HEAD
/// (a diff against HEAD would hide her own commits — the sympy-12481 lesson in
/// `workspace_candidate_diff_from`).
async fn base_sha_of(checkout: &std::path::Path, instance: &str, is_swe: bool) -> Result<String, CommandError> {
    if is_swe {
        return crate::commands::benchmark::swe_base_commit_for(instance)
            .await
            .ok_or_else(|| {
                CommandError::Invalid(format!(
                    "no dataset row names instance {instance}, so its base commit is unknown — pass base_sha"
                ))
            });
    }
    for upstream in ["origin/HEAD", "@{upstream}"] {
        if let Some(base) = git_stdout(checkout, &["merge-base", "HEAD", upstream]) {
            return Ok(base);
        }
    }
    Err(CommandError::Invalid(
        "the worktree has no default-branch merge-base to diff against — pass base_sha".into(),
    ))
}

/// Read the submission off her checkout: instance, base, and the patch's hash + size.
/// Parts she supplied herself are kept; the artifact is always recomputed here, so a
/// placeholder can never reach the board.
async fn derive_submission(
    persona: Uuid,
    card_id: Uuid,
    given_instance: Option<String>,
    given_base: Option<String>,
) -> Result<DerivedSubmission, CommandError> {
    let checkout = rooted_checkout_for(persona, card_id)?;
    let is_swe = checkout.iter().any(|c| c == "swe");
    let instance = given_instance
        .or_else(|| instance_of_checkout(&checkout, card_id))
        .unwrap_or_else(|| card_id.to_string()); // unwrap_or: instance_of_checkout always yields the card id as its floor
    let base_sha = match given_base {
        Some(b) => b,
        None => base_sha_of(&checkout, &instance, is_swe).await?,
    };
    let ws = checkout.to_string_lossy().into_owned();
    let patch = crate::commands::benchmark::workspace_candidate_diff_from(&ws, Some(&base_sha))?;
    if patch.trim().is_empty() {
        return Err(CommandError::Invalid(format!(
            "nothing to submit: {} has no changes since {} (substrate paths excluded) — \
             a submit publishes a patch, not an intention",
            checkout.display(),
            &base_sha[..base_sha.len().min(9)]
        )));
    }
    Ok(DerivedSubmission {
        instance,
        base_sha,
        artifact: artifact_of_patch(&patch),
    })
}

/// The evidence reference of a reviewer's own words: SHA-256 over the text, its length.
fn artifact_of_text(text: &str) -> WorkArtifactReference {
    use sha2::Digest;
    WorkArtifactReference {
        hash: format!("{:x}", sha2::Sha256::digest(text.as_bytes())),
        size_bytes: text.len() as u64,
        mime: Some("text/plain".to_string()),
    }
}

/// The artifact reference of a patch: SHA-256 over its bytes, its length, `text/x-patch`.
fn artifact_of_patch(patch: &str) -> WorkArtifactReference {
    use sha2::Digest;
    WorkArtifactReference {
        hash: format!("{:x}", sha2::Sha256::digest(patch.as_bytes())),
        size_bytes: patch.len() as u64,
        mime: Some("text/x-patch".to_string()),
    }
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/work/WorkSubmitResult.ts"
)]
pub struct WorkSubmitResult {
    #[ts(type = "string")]
    pub submission_id: Uuid,
    #[ts(type = "string")]
    pub card_id: Uuid,
    #[ts(type = "string")]
    pub room_id: Uuid,
    #[ts(type = "string")]
    pub publisher: Uuid,
    pub artifact: WorkArtifactReference,
    #[ts(type = "number")]
    pub submitted_at_ms: u64,
    /// Present only after the selected copy and reservations were persisted.
    #[ts(type = "string | null")]
    pub bound_staged_revision_id: Option<Uuid>,
}

#[async_trait]
impl ActionCommand for WorkSubmit {
    const NAME: &'static str = "work/submit";
    const NATIVE: bool = true;
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Publish a claimed artifact, optionally binding staged credit for independent review. Publication is not success.";
    type Params = WorkSubmitParams;
    type Output = WorkSubmitResult;

    async fn run(&self, ctx: &Ctx, p: WorkSubmitParams) -> Result<WorkSubmitResult, CommandError> {
        if p.room.trim().is_empty() {
            return Err(CommandError::Invalid(
                "an explicit activity room is required".into(),
            ));
        }
        let runtime = persona_runtime(&self.registry, ctx, "work/submit")?;
        let airc = runtime.airc();
        let room = crate::modules::room_resolve::resolve_room(airc, Some(&p.room)).await?;
        let card_id = WorkCardId::from_uuid(p.card_id);
        // Validate before allocating a durable binding. The SDK validates again
        // against its own latest projection immediately before publication.
        let board = airc
            .work_board_in(&room)
            .await
            .map_err(|e| CommandError::Internal(e.to_string()))?;
        let card = board.card(card_id).ok_or_else(|| {
            CommandError::NotFound(format!(
                "card {} is absent from the board of room {} — you asked for '{}'. If that \
                 is not the room you meant, the card is on another board and this submit \
                 went to the wrong one",
                p.card_id,
                room_label(&room.name, room.channel.as_uuid()),
                p.room
            ))
        })?;
        // The claim is HERS on THIS card, read off the board — a typed id she would
        // otherwise have to remember from a claim receipt three turns ago.
        let claim_id = match p.claim_id {
            Some(c) => ClaimId::from_uuid(c),
            None => match (card.owner, card.claim_id) {
                (Some(owner), Some(claim)) if owner == airc.peer_id() => claim,
                (Some(owner), _) => {
                    return Err(CommandError::Invalid(format!(
                        "card {} is held by {owner}, not by you — only the holder submits",
                        p.card_id
                    )))
                }
                _ => {
                    return Err(CommandError::Invalid(format!(
                        "card {} is not claimed — claim it (work/claim) before submitting",
                        p.card_id
                    )))
                }
            },
        };
        // A complete, real artifact she wrote herself is honoured as-is; anything less
        // — an omitted part, or the manual's zero hash read back as a value — is read
        // off her checkout, and a placeholder says so in the ledger.
        let typed_in_full = matches!(&p.artifact, Some(a) if !is_placeholder_hash(&a.hash))
            && p.base_sha.is_some()
            && p.instance.is_some();
        if matches!(&p.artifact, Some(a) if is_placeholder_hash(&a.hash)) {
            crate::probe!(
                class = "work.submit.placeholder_artifact",
                card = %p.card_id,
                "artifact.hash was a placeholder (zeros / a manual blank) — deriving the real one from her checkout"
            );
        }
        let (instance, base_sha_text, artifact_ref) = if typed_in_full {
            (
                p.instance.clone().unwrap_or_default(), // unwrap_or: guarded by typed_in_full
                p.base_sha.clone().unwrap_or_default(), // unwrap_or: guarded by typed_in_full
                p.artifact.clone().unwrap_or(WorkArtifactReference { hash: String::new(), size_bytes: 0, mime: None }), // unwrap_or: guarded by typed_in_full
            )
        } else {
            let d = derive_submission(runtime.persona_id(), p.card_id, p.instance.clone(), p.base_sha.clone()).await?;
            (d.instance, d.base_sha, d.artifact)
        };
        crate::probe!(
            class = "work.submit.shaped",
            card = %p.card_id,
            instance = %instance,
            base = %&base_sha_text[..base_sha_text.len().min(9)],
            size_bytes = artifact_ref.size_bytes,
            derived = !typed_in_full,
            "the submission as it goes to the board"
        );
        let artifact = artifact_ref.into_artifact()?;
        let base_sha = airc_work::GitObjectId::new(base_sha_text)
            .map_err(|e| CommandError::Invalid(format!("base_sha: {e}")))?;
        let submission_id = p.submission_id.unwrap_or_else(Uuid::new_v4); // unwrap_or: minted here when she named none — the id is ours to give
        let candidate = airc_work::WorkSubmission {
            submission_id: airc_work::SubmissionId::from_uuid(submission_id),
            card_id,
            claim_id,
            instance,
            base_sha,
            artifact,
            publisher: airc.peer_id(),
            submitted_at_ms: chrono::Utc::now().timestamp_millis().max(0) as u64,
        };
        candidate
            .validate_for_card(card)
            .map_err(|e| CommandError::Invalid(format!("submission refused: {e}")))?;
        if let Some(revision) = p.staged_revision_id {
            if self.registry.get(runtime.persona_id()).is_none()
                || runtime.persona_id() != airc.peer_id().as_uuid()
            {
                return Err(CommandError::Invalid(
                    "selected credit must belong to the calling resident persona".into(),
                ));
            }
            let executor = self
                .executor_slot
                .cloned()
                .ok_or_else(|| CommandError::Internal("work executor is not ready".into()))?;
            let conn = Connection::new(InProcessTransport::new(
                executor,
                Some(crate::routing::CallerIdentity::local_persona(
                    crate::identity::PeerId::from_uuid(runtime.persona_id()),
                )),
            ));
            reviewed::bind_submission(
                &conn,
                runtime.agent_name(),
                runtime.persona_id(),
                SubmissionSelection {
                    submission_id,
                    room_id: room.channel.as_uuid(),
                    card_id: p.card_id,
                    claim_id: claim_id.as_uuid(),
                    staged_revision_id: revision,
                    instance: candidate.instance.clone(),
                    base_sha: candidate.base_sha.clone(),
                    artifact: candidate.artifact.clone(),
                },
            )
            .await
            .map_err(|e| match e {
                reviewed::CreditBindingError::Storage(source) => {
                    CommandError::Internal(source.to_string())
                }
                other => CommandError::Invalid(other.to_string()),
            })?;
        }
        let published = airc.submit_work_in(&room, airc_lib::SubmitWork {
            submission_id: candidate.submission_id, card_id, claim_id: candidate.claim_id,
            instance: candidate.instance, base_sha: candidate.base_sha, artifact: candidate.artifact,
        }).await.map_err(|e| CommandError::Internal(format!(
            "submission publication was not acknowledged; retry the same submission_id and selection: {e}")))?;
        Ok(WorkSubmitResult {
            submission_id: published.submission_id.as_uuid(),
            card_id: published.card_id.as_uuid(),
            room_id: room.channel.as_uuid(),
            publisher: published.publisher.as_uuid(),
            artifact: (&published.artifact).into(),
            submitted_at_ms: published.submitted_at_ms,
            bound_staged_revision_id: p.staged_revision_id,
        })
    }
}

crate::register_command!(WorkSubmit);

pub struct WorkReview {
    pub registry: PersonaAircRuntimeRegistry,
    pub executor_slot: Arc<LateBound<CommandExecutor>>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/work/ReviewOutcome.ts"
)]
pub enum ReviewOutcome {
    Passed,
    Failed,
    Unknown,
}

impl From<airc_work::WorkReviewOutcome> for ReviewOutcome {
    fn from(value: airc_work::WorkReviewOutcome) -> Self {
        match value {
            airc_work::WorkReviewOutcome::Passed => Self::Passed,
            airc_work::WorkReviewOutcome::Failed => Self::Failed,
            airc_work::WorkReviewOutcome::Unknown => Self::Unknown,
        }
    }
}

impl From<ReviewOutcome> for airc_work::WorkReviewOutcome {
    fn from(value: ReviewOutcome) -> Self {
        match value {
            ReviewOutcome::Passed => Self::Passed,
            ReviewOutcome::Failed => Self::Failed,
            ReviewOutcome::Unknown => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/work/WorkReviewParams.ts"
)]
pub struct WorkReviewParams {
    /// The room the review card lives in (id or name).
    pub room: String,
    // The REVIEW card she holds. The parent card, its latest submission and artifact,
    // and her claim on the review card are read off the board from it when the fields
    // below are omitted — a reviewer never had a way to know a submission id or an
    // artifact hash by hand (5204f4b5 sent all-zero ids, 2026-09-16).
    /// The review card you hold.
    #[ts(type = "string")]
    pub review_card_id: Uuid,
    /// Your verdict.
    pub outcome: ReviewOutcome,
    /// What you ran and saw; becomes the review's evidence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub evidence_text: Option<String>,
    /// Minted when omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string")]
    pub review_id: Option<Uuid>,
    /// The card under review; read from the review card when omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string")]
    pub card_id: Option<Uuid>,
    /// The submission reviewed; the latest when omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string")]
    pub submission_id: Option<Uuid>,
    /// Its artifact; read off the board when omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub artifact: Option<WorkArtifactReference>,
    /// Your claim on the review card; read off the board when omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string")]
    pub review_claim_id: Option<Uuid>,
    /// A typed evidence reference instead of evidence_text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub evidence: Option<WorkArtifactReference>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/work/WorkReviewResult.ts"
)]
pub struct WorkReviewResult {
    #[ts(type = "string")]
    pub review_id: Uuid,
    #[ts(type = "string")]
    pub submission_id: Uuid,
    #[ts(type = "string")]
    pub reviewer: Uuid,
    pub outcome: ReviewOutcome,
    #[ts(type = "number")]
    pub reviewed_at_ms: u64,
    #[ts(type = "string")]
    pub review_card_id: Uuid,
    #[ts(type = "string")]
    pub review_claim_id: Uuid,
    pub artifact: WorkArtifactReference,
    pub evidence: WorkArtifactReference,
    /// A review can publish even when its local learning transfer is deferred.
    pub credit: Option<reviewed::ReviewedCredit>,
    pub credit_error: Option<String>,
}

#[async_trait]
impl ActionCommand for WorkReview {
    const NAME: &'static str = "work/review";
    const NATIVE: bool = true;
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str = "Review an exact submission with evidence under your linked review claim. Does not imply training completion.";
    type Params = WorkReviewParams;
    type Output = WorkReviewResult;

    async fn run(&self, ctx: &Ctx, p: WorkReviewParams) -> Result<Self::Output, CommandError> {
        if p.room.trim().is_empty() {
            return Err(CommandError::Invalid(
                "an explicit activity room is required".into(),
            ));
        }
        let runtime = persona_runtime(&self.registry, ctx, "work/review")?;
        let airc = runtime.airc();
        let room = crate::modules::room_resolve::resolve_room(airc, Some(&p.room)).await?;
        // Everything a reviewer cannot know by hand is read off the board from the
        // review card she holds: the parent, her claim, the parent's latest submission
        // and its artifact. Typed values are honoured; placeholders were already
        // refused at the executor seam.
        let board = airc
            .work_board_in(&room)
            .await
            .map_err(|e| CommandError::Internal(e.to_string()))?;
        let review_card = board
            .card(WorkCardId::from_uuid(p.review_card_id))
            .ok_or_else(|| {
                CommandError::NotFound(format!(
                    "review card {} is absent from the board of room {} — you asked for '{}'",
                    p.review_card_id,
                    room_label(&room.name, room.channel.as_uuid()),
                    p.room
                ))
            })?;
        let card_id = match p.card_id {
            Some(c) => WorkCardId::from_uuid(c),
            None => review_card.reviews.ok_or_else(|| {
                CommandError::Invalid(format!(
                    "card {} is not a review card (it reviews nothing) — pass card_id if you are reviewing out of band",
                    p.review_card_id
                ))
            })?,
        };
        let review_claim_id = match p.review_claim_id {
            Some(c) => ClaimId::from_uuid(c),
            None => match (review_card.owner, review_card.claim_id) {
                (Some(owner), Some(claim)) if owner == airc.peer_id() => claim,
                (Some(owner), _) => {
                    return Err(CommandError::Invalid(format!(
                        "review card {} is held by {owner}, not by you — only its holder reviews",
                        p.review_card_id
                    )))
                }
                _ => {
                    return Err(CommandError::Invalid(format!(
                        "review card {} is not claimed — claim it (work/claim) before reviewing",
                        p.review_card_id
                    )))
                }
            },
        };
        let parent = board
            .card(card_id)
            .ok_or_else(|| {
                CommandError::NotFound(format!(
                    "card {card_id} is absent from the board of room {} — you asked for '{}'",
                    room_label(&room.name, room.channel.as_uuid()),
                    p.room
                ))
            })?;
        let submission = match p.submission_id {
            Some(id) => parent
                .submissions
                .iter()
                .find(|s| s.submission_id.as_uuid() == id)
                .ok_or_else(|| CommandError::Invalid(format!("card {card_id} has no submission {id}")))?,
            None => parent
                .submissions
                .iter()
                .max_by_key(|s| s.submitted_at_ms)
                .ok_or_else(|| {
                    CommandError::Invalid(format!(
                        "card {card_id} has no submission to review yet — the holder submits first (work/submit)"
                    ))
                })?,
        };
        let artifact = match p.artifact.clone() {
            Some(a) => a.into_artifact()?,
            None => submission.artifact.clone(),
        };
        let evidence = match (p.evidence.clone(), p.evidence_text.as_deref()) {
            (Some(e), _) => e.into_artifact()?,
            (None, Some(text)) if !text.trim().is_empty() => artifact_of_text(text).into_artifact()?,
            _ => {
                return Err(CommandError::Invalid(
                    "a review carries evidence: pass evidence_text (what you ran and saw) — a verdict without evidence is not a verdict".into(),
                ))
            }
        };
        let review_id = p.review_id.unwrap_or_else(Uuid::new_v4); // unwrap_or: minted here when she named none — the id is ours to give
        crate::probe!(
            class = "work.review.shaped",
            review_card = %p.review_card_id,
            card = %card_id,
            submission = %submission.submission_id,
            derived = p.submission_id.is_none() || p.artifact.is_none() || p.review_claim_id.is_none() || p.card_id.is_none(),
            "the review as it goes to the board"
        );
        let review = airc
            .review_work_submission_in(
                &room,
                airc_lib::ReviewWorkSubmission {
                    review_id: airc_work::WorkReviewId::from_uuid(review_id),
                    card_id,
                    submission_id: submission.submission_id,
                    artifact,
                    review_card_id: WorkCardId::from_uuid(p.review_card_id),
                    review_claim_id,
                    outcome: p.outcome.into(),
                    evidence,
                },
            )
            .await
            .map_err(|e| {
                CommandError::Invalid(format!(
                    "review not acknowledged; retry the same review_id and judgement: {e}"
                ))
            })?;
        // Re-read accepted signed projection even on the local command path.
        // Publication alone is not permission to bypass replay validation.
        let credit = async {
            let board = airc
                .work_board_in(&room)
                .await
                .map_err(|e| CommandError::Internal(e.to_string()))?;
            let executor = self
                .executor_slot
                .cloned()
                .ok_or_else(|| CommandError::Internal("work executor is not ready".into()))?;
            reviewed::consume_as_publisher(
                &self.registry,
                executor,
                room.channel.as_uuid(),
                &board,
                review.review_id,
            )
            .await
        }
        .await;
        let (credit, credit_error) = match credit {
            Ok(value) => (Some(value), None),
            Err(error) => (None, Some(error.to_string())),
        };
        Ok(WorkReviewResult {
            review_id: review.review_id.as_uuid(),
            submission_id: review.submission_id.as_uuid(),
            reviewer: review.reviewer.as_uuid(),
            outcome: p.outcome,
            reviewed_at_ms: review.reviewed_at_ms,
            review_card_id: review.review_card_id.as_uuid(),
            review_claim_id: review.review_claim_id.as_uuid(),
            artifact: (&review.artifact).into(),
            evidence: (&review.evidence).into(),
            credit,
            credit_error,
        })
    }
}

crate::register_command!(WorkReview);

pub struct WorkSubmission {
    pub registry: PersonaAircRuntimeRegistry,
    pub executor_slot: Arc<LateBound<CommandExecutor>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/work/WorkSubmissionParams.ts"
)]
pub struct WorkSubmissionParams {
    /// Submission's activity room.
    pub room: String,
    /// Parent work-card UUID in this room.
    #[ts(type = "string")]
    pub card_id: Uuid,
    /// Accepted submission UUID.
    #[ts(type = "string")]
    pub submission_id: Uuid,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/work/WorkSubmissionResult.ts"
)]
pub struct WorkSubmissionResult {
    pub submission: WorkSubmitResult,
    #[ts(type = "string")]
    pub claim_id: Uuid,
    pub base_sha: String,
    pub instance: String,
    pub reviews: Vec<WorkReviewResult>,
    pub credit: Option<reviewed::ReviewedCredit>,
    pub credit_error: Option<String>,
}

#[async_trait]
impl ActionCommand for WorkSubmission {
    const NAME: &'static str = "work/submission";
    const NATIVE: bool = true;
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str = "Inspect a submission, signed reviews and local credit receipts. No private prompts or training.";
    type Params = WorkSubmissionParams;
    type Output = WorkSubmissionResult;

    async fn run(&self, ctx: &Ctx, p: WorkSubmissionParams) -> Result<Self::Output, CommandError> {
        if p.room.trim().is_empty() {
            return Err(CommandError::Invalid(
                "an explicit activity room is required".into(),
            ));
        }
        let runtime = persona_runtime(&self.registry, ctx, "work/submission")?;
        let room =
            crate::modules::room_resolve::resolve_room(runtime.airc(), Some(&p.room)).await?;
        let board = runtime
            .airc()
            .work_board_in(&room)
            .await
            .map_err(|e| CommandError::Internal(e.to_string()))?;
        let submitted = board
            .card(WorkCardId::from_uuid(p.card_id))
            .and_then(|card| {
                card.submissions
                    .iter()
                    .find(|s| s.submission_id.as_uuid() == p.submission_id)
            })
            .ok_or_else(|| {
                CommandError::NotFound(
                    "submission is absent from this card and room — this verb inspects a SUBMISSION by \
                     submission_id; to read the card itself use work/get {card_id}, to publish yours \
                     use work/submit"
                        .into(),
                )
            })?;
        let credit = async {
            let Some(owner) = self.registry.get(submitted.publisher.as_uuid()) else {
                return Ok(reviewed::ReviewedCredit::pending(
                    reviewed::ReviewedCreditState::PublisherNotResident,
                ));
            };
            if owner.airc().peer_id() != submitted.publisher {
                return Err(CommandError::Invalid(
                    "registered publisher identity differs".into(),
                ));
            }
            let executor = self
                .executor_slot
                .cloned()
                .ok_or_else(|| CommandError::Internal("work executor is not ready".into()))?;
            let conn = Connection::new(InProcessTransport::new(
                executor,
                Some(crate::routing::CallerIdentity::local_persona(
                    crate::identity::PeerId::from_uuid(owner.persona_id()),
                )),
            ));
            reviewed::credit_status(&conn, owner.agent_name(), p.submission_id)
                .await
                .map_err(|e| CommandError::Internal(e.to_string()))
        }
        .await;
        let (credit, credit_error) = match credit {
            Ok(value) => (Some(value), None),
            Err(e) => (None, Some(e.to_string())),
        };
        let reviews = board
            .submission_reviews_for(submitted.submission_id)
            .filter(|r| r.card_id == submitted.card_id)
            .map(|r| WorkReviewResult {
                review_id: r.review_id.as_uuid(),
                submission_id: r.submission_id.as_uuid(),
                reviewer: r.reviewer.as_uuid(),
                outcome: r.outcome.into(),
                reviewed_at_ms: r.reviewed_at_ms,
                review_card_id: r.review_card_id.as_uuid(),
                review_claim_id: r.review_claim_id.as_uuid(),
                artifact: (&r.artifact).into(),
                evidence: (&r.evidence).into(),
                credit: None,
                credit_error: None,
            })
            .collect();
        Ok(WorkSubmissionResult {
            submission: WorkSubmitResult {
                submission_id: p.submission_id,
                card_id: p.card_id,
                room_id: room.channel.as_uuid(),
                publisher: submitted.publisher.as_uuid(),
                artifact: (&submitted.artifact).into(),
                submitted_at_ms: submitted.submitted_at_ms,
                bound_staged_revision_id: credit.as_ref().and_then(|c| c.staged_revision_id),
            },
            claim_id: submitted.claim_id.as_uuid(),
            base_sha: submitted.base_sha.to_string(),
            instance: submitted.instance.clone(),
            reviews,
            credit,
            credit_error,
        })
    }
}

crate::register_command!(WorkSubmission);

#[cfg(test)]
mod tests {
    // what this catches (2026-09-17): a submit the citizen could not satisfy by hand —
    // the manual's zero hash is a placeholder and never an artifact; the artifact is
    // the SHA-256 of her patch; a benchmark checkout names its instance from its path
    // and a repo worktree names the card; a worktree with no base to diff against is a
    // named refusal, never a diff against HEAD.
    use super::{artifact_of_patch, base_sha_of, instance_of_checkout, is_placeholder_hash, room_label, short8};
    use std::path::Path;

    // what this catches (Kimi's work/submit, 2026-09-21): "card X is absent from this room"
    // is true and useless — she asked for one room name, `resolve_room` turned it into a
    // Room, and if that landed elsewhere the sentence gives her no way to see the mismatch,
    // so the substrate's resolution failing reads as her being wrong about her own card.
    // The refusal must name the RESOLVER's answer, and carry the channel id beside it
    // because two rooms can share a name across scopes (Cormac's condition).
    #[test]
    fn a_room_is_named_by_resolver_answer_and_channel_never_by_name_alone() {
        let a = uuid::Uuid::from_u128(0xabcdef12_3456_7890_abcd_ef1234567890);
        let b = uuid::Uuid::from_u128(0x12345678_90ab_cdef_1234_567890abcdef);
        let seed = room_label("continuum", a);
        assert!(seed.contains("'continuum'"), "the resolved NAME is quoted: {seed}");
        assert!(seed.contains(&short8(a)), "and the channel id rides beside it: {seed}");
        // Two rooms sharing a name are distinguishable — the whole point of carrying the id.
        assert_ne!(room_label("continuum", a), room_label("continuum", b));
    }

    #[test]
    fn placeholders_are_never_artifacts_and_a_patch_hashes_to_its_sha256() {
        assert!(is_placeholder_hash(""));
        assert!(is_placeholder_hash(&"0".repeat(64)));
        assert!(is_placeholder_hash("<replace-with-string>"));
        assert!(!is_placeholder_hash("9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"));
        let a = artifact_of_patch("test");
        assert_eq!(a.hash, "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08");
        assert_eq!(a.size_bytes, 4);
        assert_eq!(a.mime.as_deref(), Some("text/x-patch"));
    }

    #[test]
    fn a_benchmark_checkout_names_its_instance_and_a_worktree_names_the_card() {
        let card = uuid::Uuid::from_u128(7);
        assert_eq!(
            instance_of_checkout(Path::new("/x/peers/p/workspace/swe/sympy__sympy-23413"), card).as_deref(),
            Some("sympy__sympy-23413")
        );
        assert_eq!(
            instance_of_checkout(Path::new("/x/.airc/worktrees/1f58fc16"), card),
            Some(card.to_string())
        );
    }

    #[tokio::test]
    async fn a_worktree_with_no_base_is_refused_by_name() {
        let dir = tempfile::tempdir().expect("tempdir");
        let run = |args: &[&str]| {
            std::process::Command::new("git").args(args).current_dir(dir.path()).output().expect("git")
        };
        run(&["init", "-q"]);
        run(&["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "root"]);
        let err = base_sha_of(dir.path(), "card", false).await.expect_err("no remote, no upstream");
        assert!(err.to_string().contains("pass base_sha"), "{err}");
    }
}
