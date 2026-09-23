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
    /// The card you hold — board handle or full UUID.
    #[ts(type = "string")]
    pub card_id: String,
    /// Minted when omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string")]
    pub submission_id: Option<Uuid>,
    /// Your claim — handle or UUID; read off the board when omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string")]
    pub claim_id: Option<String>,
    /// Benchmark instance name; read from your checkout when omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub instance: Option<String>,
    /// Patch base; defaults to recorded creation/benchmark base. Required for legacy worktrees.
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

/// Expand an id the citizen was SHOWN into its canonical [`Uuid`].
///
/// These two verbs typed their id params as `Uuid`, so a short handle was refused by
/// SERDE — before the handler existed, before any message could name the problem, and
/// with no hint that a longer form was wanted. Every surface displays ids in the 8-char
/// short form (`card d61513e4`), which is the only form she has in front of her.
///
/// Three properties, and they are why this is not a tolerant deserializer:
/// - **Strict.** [`crate::id_resolve::resolve_handle`], not the repairing `resolve`. A
///   malformed id is refused, and every supplied digit is kept so a collision can be
///   disambiguated by typing more of it.
/// - **Scoped.** Candidates are supplied by the caller from the ONE room's board it has
///   already read, or the ONE card's submissions — never a global set. An ambiguity is
///   refused against a narrow, named population.
/// - **Not an authorization.** Resolving an id grants nothing; the holder and ownership
///   guards downstream are untouched and still run on the canonical id.
fn resolve_shown(raw: &str, candidates: &[uuid::Uuid], label: &str) -> Result<Uuid, CommandError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(CommandError::Invalid(format!("a {label} id is required")));
    }
    crate::id_resolve::resolve_handle(raw, candidates, label).map_err(CommandError::Invalid)
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

/// May this caller submit against this card, given the claim id SHE SUPPLIED? Pure, so
/// both refusals are pinned by a test rather than by a sentence nobody reads until it
/// fires.
///
/// THIS ONLY EVER JUDGES A CITIZEN WHO PASSED A CLAIM ID — the caller resolves the claim
/// off the board when `claim_id` is omitted, and refuses there with its own sentences. And
/// `claim_id` is documented optional ("read off the board when omitted"), which makes it a
/// fillable blank in her manual; citizens demonstrably fill those (ab76eb848 — they were
/// sending `<string>`, and `is_placeholder_hash` in this file exists for the same reason).
/// So the population here is "she supplied an id", and its likeliest defect is a STALE id
/// from a re-claim after a lapsed lease — routine, not exotic, since a lease dies in any
/// restart longer than its remaining TTL.
///
/// Before this the two halves shared one sentence ("submission requires your matching
/// claim on this card"), which told a citizen who OWNS the card and HOLDS a live lease
/// only that *something* did not match — the fourth opaque refusal in a row for the
/// citizen #4309 was written to unblock. Each half now says what it saw, and the stale-id
/// half names the way out that actually works: omitting `claim_id` makes the caller read
/// the live claim off the board, and this guard then passes.
fn holder_guard(
    owner: Option<airc_core::PeerId>,
    me: airc_core::PeerId,
    board_claim: Option<ClaimId>,
    supplied: ClaimId,
    card_id: Uuid,
) -> Result<(), CommandError> {
    if owner != Some(me) {
        return Err(CommandError::Denied(format!(
            "card {card_id} is held by {}, not by you — only the holder submits",
            owner.map(|o| short8(o.as_uuid())).unwrap_or_else(|| "nobody".to_string()), // unwrap_or_else: an unclaimed card names that absence
        )));
    }
    if board_claim != Some(supplied) {
        return Err(CommandError::Denied(format!(
            "the claim id you passed ({}) is not the live claim on card {card_id} — the \
             board's claim is {}. YOU DO HOLD THIS CARD; omit claim_id and submit again and \
             the live claim is read off the board for you.",
            short8(supplied.as_uuid()),
            board_claim.map(|c| short8(c.as_uuid())).unwrap_or_else(|| "none — the card is unclaimed".to_string()), // unwrap_or_else: says which absence it saw
        )));
    }
    Ok(())
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

/// Use the benchmark dataset base or the immutable repo-worktree creation anchor.
/// Legacy worktrees require an explicit base; remote refs and HEAD do not prove
/// where this work began and can include unrelated integration history.
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
    airc_lib::work_worktree::creation_base(checkout).map_err(CommandError::Invalid)
}

/// Read the submission off her checkout: instance, base, and the patch's hash + size.
/// Parts she supplied herself are kept; the artifact is always recomputed here, so a
/// placeholder can never reach the board.
async fn derive_submission(
    checkout: &std::path::Path,
    card_id: Uuid,
    given_instance: Option<String>,
    given_base: Option<String>,
) -> Result<DerivedSubmission, CommandError> {
    let is_swe = checkout.iter().any(|c| c == "swe");
    let instance = given_instance
        .or_else(|| instance_of_checkout(checkout, card_id))
        .unwrap_or_else(|| card_id.to_string()); // unwrap_or: instance_of_checkout always yields the card id as its floor
    let base_sha = match given_base {
        Some(b) => b,
        None => base_sha_of(checkout, &instance, is_swe).await?,
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
        "Submit own work for review; artifact/claim default from checkout. Publication is not success.";
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
        // Validate before allocating a durable binding. The SDK validates again
        // against its own latest projection immediately before publication.
        let board = airc
            .work_board_in(&room)
            .await
            .map_err(|e| CommandError::Internal(e.to_string()))?;
        // THE BOARD IS READ FIRST so the handle resolves against THIS room's cards and
        // nothing wider. A full UUID passes straight through and never consults the set.
        let snapshot = board.snapshot();
        let card_uuid = resolve_shown(
            &p.card_id,
            &snapshot.cards.iter().map(|c| c.card_id.as_uuid()).collect::<Vec<_>>(),
            "card",
        )?;
        let card_id = WorkCardId::from_uuid(card_uuid);
        let card = board.card(card_id).ok_or_else(|| {
            CommandError::NotFound(format!(
                "card {} is absent from the board of room {} — you asked for '{}'. If that \
                 is not the room you meant, the card is on another board and this submit \
                 went to the wrong one",
                card_uuid,
                room_label(&room.name, room.channel.as_uuid()),
                p.room
            ))
        })?;
        // The claim is HERS on THIS card, read off the board — a typed id she would
        // otherwise have to remember from a claim receipt three turns ago.
        let claim_id = match p.claim_id.as_deref() {
            // Scoped to THIS card's own claim — the narrowest set there is, so a handle
            // from a stale receipt is refused by name rather than silently accepted.
            Some(c) => ClaimId::from_uuid(resolve_shown(
                c,
                &card.claim_id.map(|id| id.as_uuid()).into_iter().collect::<Vec<_>>(),
                "claim",
            )?),
            None => match (card.owner, card.claim_id) {
                (Some(owner), Some(claim)) if owner == airc.peer_id() => claim,
                (Some(owner), _) => {
                    return Err(CommandError::Invalid(format!(
                        "card {} is held by {owner}, not by you — only the holder submits",
                        card_uuid
                    )))
                }
                _ => {
                    return Err(CommandError::Invalid(format!(
                        "card {} is not claimed — claim it (work/claim) before submitting",
                        card_uuid
                    )))
                }
            },
        };
        // A supplied claim id must not bypass ownership before we read local work
        // (#4309). Publication below still validates the current lease against the
        // latest board. The decision is pure so both refusals are pinned by a test.
        holder_guard(card.owner, airc.peer_id(), card.claim_id, claim_id, card_uuid)?;
        // A complete, real artifact she wrote herself is honoured as-is; anything less
        // — an omitted part, or the manual's zero hash read back as a value — is read
        // off her checkout, and a placeholder says so in the ledger.
        let typed_in_full = matches!(&p.artifact, Some(a) if !is_placeholder_hash(&a.hash))
            && p.base_sha.is_some()
            && p.instance.is_some();
        if matches!(&p.artifact, Some(a) if is_placeholder_hash(&a.hash)) {
            crate::probe!(
                class = "work.submit.placeholder_artifact",
                card = %card_uuid,
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
            // Claim-time staging owns this binding. Ambient turn focus can be home,
            // another card, or absent after restart; none changes this card's checkout.
            let checkout = crate::modules::card_staging::checkout_path_for(&runtime.persona_id(), card)
                .ok_or_else(|| CommandError::Invalid(format!(
                    "card {} has no staged checkout on this node, so there is no tree to read a \
                     patch from. Staging is CLAIM-TIME work, not a verb you can call: re-claim \
                     the card (work/claim) and it is staged for you. If a re-claim does not \
                     produce one, that is a substrate fault — say so in the room rather than \
                     re-doing the work, your patch is not the problem.",
                    card_uuid,
                )))?;
            let d = derive_submission(&checkout, card_uuid, p.instance.clone(), p.base_sha.clone()).await?;
            (d.instance, d.base_sha, d.artifact)
        };
        crate::probe!(
            class = "work.submit.shaped",
            card = %card_uuid,
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
                    card_id: card_uuid,
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
    /// Full UUID of your linked review card, not the task you authored.
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
    const DESCRIPTION: &'static str = "Verdict under a reviewer claim, not training completion. Own work: work/submit.";
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
    /// Parent card — board handle or full UUID.
    #[ts(type = "string")]
    pub card_id: String,
    /// The submission to read — handle or full UUID.
    #[ts(type = "string")]
    pub submission_id: String,
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
    const DESCRIPTION: &'static str = "Read submission_id. Task: work/get. Publish: work/submit.";
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
        // ROOM-SCOPED for the card, then CARD-SCOPED for the submission — each handle is
        // resolved against the narrowest population that can contain it, so an ambiguous
        // prefix names the candidates it actually collided with.
        let snapshot = board.snapshot();
        let card_uuid = resolve_shown(
            &p.card_id,
            &snapshot.cards.iter().map(|c| c.card_id.as_uuid()).collect::<Vec<_>>(),
            "card",
        )?;
        let card = board.card(WorkCardId::from_uuid(card_uuid)).ok_or_else(|| {
            CommandError::NotFound(format!(
                "card {card_uuid} is absent from the board of room {} — to read a card use \
                 work/get, to publish your own work use work/submit",
                room_label(&room.name, room.channel.as_uuid()),
            ))
        })?;
        let submission_uuid = resolve_shown(
            &p.submission_id,
            &card
                .submissions
                .iter()
                .map(|s| s.submission_id.as_uuid())
                .collect::<Vec<_>>(),
            "submission",
        )?;
        let submitted = card
            .submissions
            .iter()
            .find(|s| s.submission_id.as_uuid() == submission_uuid)
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
            reviewed::credit_status(&conn, owner.agent_name(), submission_uuid)
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
                submission_id: submission_uuid,
                card_id: card_uuid,
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
    use super::{artifact_of_patch, base_sha_of, holder_guard, instance_of_checkout, is_placeholder_hash, resolve_shown, room_label, short8};
    use std::path::Path;

    // what this catches (Astra, 2026-09-23): these two verbs typed their id params as
    // `Uuid`, so the 8-char handle the board SHOWS her was refused by serde — before the
    // handler, before any message could name the problem or the form it wanted. The
    // resolution must be the shared strict primitive against a SCOPED candidate set, so
    // that a handle expands, a collision is refused by name, and nothing is repaired.
    #[test]
    fn a_verb_accepts_the_handle_its_board_showed_and_refuses_an_ambiguous_one() {
        let a = uuid::Uuid::parse_str("d61513e4-a332-4cd3-a208-5bf0e8acd50d").expect("fixture id"); // expect: a literal id in a test
        let b = uuid::Uuid::parse_str("d61599ff-0000-4000-8000-000000000000").expect("fixture id"); // expect: a literal id in a test
        let other = uuid::Uuid::parse_str("7de5b691-0000-4000-8000-000000000000").expect("fixture id"); // expect: a literal id in a test

        // THE HANDLE SHE WAS SHOWN. This is the call that used to fail at deserialization.
        assert_eq!(resolve_shown("d61513e4", &[a, other], "card").expect("resolves"), a); // expect: asserted to resolve

        // A full UUID passes straight through and never consults the set — so an empty
        // board (an outage, a wrong room) cannot turn a correct id into a miss.
        assert_eq!(resolve_shown(&a.to_string(), &[], "card").expect("passes through"), a); // expect: asserted to pass

        // AMBIGUITY IS REFUSED, NOT GUESSED, and supplying more digits disambiguates —
        // the whole reason this uses the strict handle resolver and not the repairing one.
        let ambiguous = resolve_shown("d615", &[a, b], "card").expect_err("two cards share d615"); // expect_err: asserted ambiguous
        assert!(format!("{ambiguous:?}").contains("d615"), "the refusal names the prefix that collided");
        assert_eq!(resolve_shown("d61513e4", &[a, b], "card").expect("more digits decide"), a); // expect: asserted to resolve

        // A MISS IS A MISS. An id that matches nothing is refused against the named
        // population rather than silently reaching a lookup that reports the card absent.
        assert!(resolve_shown("deadbeef", &[a, other], "card").is_err());

        // Too short to disambiguate, and empty, are both named refusals — never a match.
        assert!(resolve_shown("d6", &[a, other], "card").is_err());
        assert!(resolve_shown("   ", &[a, other], "card").is_err());
    }

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
        assert!(run(&["init", "-q"]).status.success());
        assert!(run(&["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "root"]).status.success());
        // A remote default branch is not proof of a legacy worktree's creation base.
        assert!(run(&["update-ref", "refs/remotes/origin/HEAD", "HEAD"]).status.success());
        let err = base_sha_of(dir.path(), "card", false).await.expect_err("no recorded creation base");
        assert!(err.to_string().contains("pass base_sha"), "{err}");
        // Regression for Kimi's result5734: submitting an owned staged checkout
        // requires no temporary acting root (including after focus loss/restart).
        let base = String::from_utf8(run(&["rev-parse", "HEAD"]).stdout).expect("sha").trim().to_string();
        std::fs::write(dir.path().join("solution.py"), "print('verified work')\n").expect("write patch");
        let add = run(&["add", "solution.py"]);
        assert!(add.status.success());
        let card = uuid::Uuid::new_v4();
        let derived = super::derive_submission(dir.path(), card, None, Some(base.clone())).await.expect("derive without ambient hands");
        assert_eq!(derived.instance, card.to_string());
        assert_eq!(derived.base_sha, base);
        let expected = String::from_utf8(run(&["diff", "--binary", "--full-index", "--no-ext-diff", "--no-textconv", "--no-color", "--src-prefix=a/", "--dst-prefix=b/", "HEAD", "--", "solution.py"]).stdout).expect("diff");
        assert_eq!(derived.artifact.hash, artifact_of_patch(&expected).hash);
        assert!(derived.artifact.size_bytes > 0);
        assert!(run(&["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "-m", "solution"]).status.success());
        let clean_base = String::from_utf8(run(&["rev-parse", "HEAD"]).stdout).expect("sha").trim().to_string();
        // Regression for Kimi's oversized submissions: advancing the remote default
        // must neither swallow committed work nor change the stored creation base.
        assert!(run(&["update-ref", airc_lib::work_worktree::CREATION_BASE_REF, &base, ""]).status.success());
        assert!(run(&["update-ref", "refs/remotes/origin/HEAD", "HEAD"]).status.success());
        let recorded = super::derive_submission(dir.path(), card, None, None).await.expect("recorded base includes committed work");
        assert_eq!(recorded.base_sha, base);
        assert_eq!(recorded.artifact.hash, derived.artifact.hash);
        assert_eq!(recorded.artifact.size_bytes, derived.artifact.size_bytes);
        let empty = super::derive_submission(dir.path(), card, None, Some(clean_base)).await.err().expect("clean checkout refused");
        assert!(empty.to_string().contains("nothing to submit"));
    }

    // what this catches (Kimi, 2026-09-21): the two halves of the supplied-claim guard
    // shared one sentence — "submission requires your matching claim on this card" — so a
    // citizen who OWNED the card and HELD a live lease, but passed a STALE claim id from a
    // re-claim, was told only that something did not match. `claim_id` is documented
    // optional and therefore a fillable blank in her manual, and citizens fill those
    // (ab76eb848). Each half must now name what it saw, and the stale half must name the
    // way out that actually works — omitting claim_id, which makes the caller read the
    // live claim off the board so this guard passes.
    #[test]
    fn each_half_of_the_holder_guard_names_what_it_saw() {
        let me = airc_core::PeerId::new();
        let peer = airc_core::PeerId::new();
        let card = uuid::Uuid::new_v4();
        let mine = super::ClaimId::from_uuid(uuid::Uuid::new_v4());
        let stale = super::ClaimId::from_uuid(uuid::Uuid::new_v4());

        // The holder's own live claim passes.
        assert!(holder_guard(Some(me), me, Some(mine), mine, card).is_ok());

        // Someone else holds it: say WHO, and reuse the holder sentence.
        let held = holder_guard(Some(peer), me, Some(mine), mine, card).expect_err("not hers");
        let held = held.to_string();
        assert!(held.contains(&short8(peer.as_uuid())), "names the holder: {held}");
        assert!(held.contains("only the holder submits"), "{held}");

        // Hers, but the id she passed is stale: name BOTH ids and the way out.
        let bad = holder_guard(Some(me), me, Some(mine), stale, card).expect_err("stale id");
        let bad = bad.to_string();
        assert!(bad.contains(&short8(stale.as_uuid())), "names what she passed: {bad}");
        assert!(bad.contains(&short8(mine.as_uuid())), "names the live claim: {bad}");
        assert!(bad.contains("omit claim_id"), "names the corrective that works: {bad}");
        assert!(!bad.contains("only the holder submits"), "she IS the holder: {bad}");

        // An unclaimed card names THAT absence rather than printing an empty slot.
        let none = holder_guard(Some(me), me, None, stale, card).expect_err("no live claim");
        assert!(none.to_string().contains("unclaimed"), "{}", none);
    }
}
