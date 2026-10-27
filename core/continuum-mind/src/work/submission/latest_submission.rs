use super::*;

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
pub(super) fn room_label(name: &str, channel: Uuid) -> String {
    format!("'{name}' ({})", short8(channel))
}

/// The same content-addressed reference the AIRC submission protocol carries.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/work/WorkArtifactReference.ts"
)]
pub struct WorkArtifactReference {
    /// SHA-256 of the patch bytes (64 hex), not a git sha.
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
    pub(super) fn into_artifact(self) -> Result<airc_work::SubmissionArtifact, CommandError> {
        // Feed the owned string directly into the protocol's existing hash
        // decoder; no second hash parser or JSON body round-trip.
        // A git commit sha is the value a citizen HAS in hand, so it is the value she tries
        // (Kimi, 2026-09-28). Name it and the fix rather than a parse error.
        if looks_like_git_sha(&self.hash) {
            return Err(CommandError::Invalid(format!(
                "artifact hash '{}' looks like a git commit sha, not the patch's SHA-256. Omit artifact: \
                 work/submit computes it from your card checkout.",
                self.hash.trim()
            )));
        }
        let hash = Deserialize::deserialize(serde::de::value::StringDeserializer::<
            serde::de::value::Error,
        >::new(self.hash))
        .map_err(|e| {
            CommandError::Invalid(format!(
                "artifact hash: {e}. Omit artifact and work/submit computes it from your checkout."
            ))
        })?;
        Ok(airc_work::SubmissionArtifact {
            hash,
            size_bytes: self.size_bytes,
            mime: self.mime,
        })
    }
}

/// PURE: a git object id as written (40 hex for SHA-1, 7..=39 for an abbreviation), which
/// is never a 64-hex SHA-256 artifact hash.
pub(super) fn looks_like_git_sha(raw: &str) -> bool {
    let raw = raw.trim();
    (7..=40).contains(&raw.len()) && raw.chars().all(|c| c.is_ascii_hexdigit())
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
    pub benchmark: Arc<dyn BenchmarkSubmissionAdapter>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/work/WorkSubmitParams.ts"
)]
pub struct WorkSubmitParams {
    /// Optional; the card's board decides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub room: Option<String>,
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
    /// Your claim; defaults to the board.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string")]
    pub claim_id: Option<String>,
    /// Instance; defaults to checkout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub instance: Option<String>,
    /// Patch base; defaults to the recorded base.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub base_sha: Option<String>,
    /// Omit: computed from your checkout. Not a git sha.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub artifact: Option<WorkArtifactReference>,
    /// Staged revision UUID; bind before publish or no credit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string")]
    pub staged_revision_id: Option<Uuid>,
    /// Generation that issued this work; not with staged_revision_id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub generation_request_id: Option<String>,
    /// Submodule paths you mean to move.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub submodules: Option<Vec<String>>,
}

/// Where the card she named actually is, when it is not on the room she passed
/// (Kimi, 2026-09-28: she passed her turn's room, the card lived on #cambriantech, and
/// the refusal said only "no card matches ... among 176"). Reads her subscribed boards
/// and names the one that holds the card; `None` when no other board does.
pub(super) async fn card_elsewhere(airc: &std::sync::Arc<airc_lib::Airc>, raw: &str, asked: Uuid) -> Option<String> {
    let horizon = super::super::board_horizon(airc).await.ok()?;
    let id = super::super::resolve_card_id_in_boards(&horizon, raw).ok()?;
    horizon
        .boards
        .iter()
        .find(|(r, b)| r.channel.as_uuid() != asked && b.card(id).is_some())
        .map(|(r, _)| {
            format!(
                "That card is on the board of room {}: call again with room '{}'",
                room_label(&r.name, r.channel.as_uuid()),
                r.name
            )
        })
}

/// The room a card verb acts in: the board that holds the card. A card sits on exactly ONE
/// board, so the card names its room, and a room she names is consulted only when the card is
/// on no board she can see. Kimi on the 5090 (2026-10-06) was refused three times in an hour
/// for guessing a room ('swe-bench', 'default') while the card id in her hand already said
/// where it was: a refusal that asks her for what the card knows costs her a turn for nothing
/// (accept-or-redirect, the rule `follow_card_room` states for claim/state/release).
pub(super) async fn room_of_card(
    airc: &std::sync::Arc<airc_lib::Airc>,
    named: Option<&str>,
    raw_card: &str,
    verb: &'static str,
) -> Result<airc_lib::Room, CommandError> {
    let named = named.map(str::trim).filter(|s| !s.is_empty());
    let horizon = super::super::board_horizon(airc)
        .await
        .map_err(|e| CommandError::Internal(format!("{verb}: the rooms you are in could not be listed: {e}")))?;
    match super::super::resolve_card_id_in_boards(&horizon, raw_card) {
        Ok(id) => {
            let mut holders = horizon.boards.iter().filter(|(_, b)| b.card(id).is_some()).map(|(room, _)| room);
            if let Some(room) = holders.next() {
                if let Some(other) = holders.next() {
                    // a card is on exactly one board; two is a fault worth seeing, not a choice
                    continuum_base::probe!(
                        class = "work.room.card_on_two_boards",
                        verb,
                        card_id = %short8(id.as_uuid()),
                        first = %room.name,
                        second = %other.name,
                        "a card id found on two readable boards: acting in the first"
                    );
                }
                if let Some(asked) = named {
                    let wanted = asked.trim_start_matches('#');
                    if wanted != room.name && wanted != room.channel.as_uuid().to_string() {
                        continuum_base::probe!(
                            class = "work.room.from_card",
                            verb,
                            card_id = %short8(id.as_uuid()),
                            asked = %asked,
                            used = %room.name,
                            "a card verb named a room that does not hold the card: it acts in the card's room"
                        );
                    }
                }
                return Ok(room.clone());
            }
            // a card she HOLDS on a room she does not subscribe to names its own room through the
            // followed-claim record, the one the renewal and the work gate trust (card d6e1b7a2)
            if let Some(room) = followed_room(&crate::persona::held_claims::held(airc.peer_id().as_uuid()), raw_card) {
                return Ok(room);
            }
            // the id is whole but on no READABLE board: named, that room answers for itself;
            // unnamed, an unreadable board is a read failure (horizon's own story), never "absent"
            match named {
                Some(asked) => crate::persona::room_resolve::resolve_room(airc, Some(asked)).await,
                None if !horizon.unreadable.is_empty() => Err(horizon.not_found("card", raw_card)),
                None => Err(CommandError::Invalid(format!(
                    "{verb}: the card you named is on no board of a room you are in, so it cannot say its room: \
                     join the card's room (room/join) or name it with room=..."
                ))),
            }
        }
        // the handle did not resolve across her boards (a prefix on two cards, a malformed id,
        // or no board to search): a named room disambiguates, and its own resolution reports
        // its own truth; unnamed, the resolver's refusal IS the answer, never a guessed absence
        Err(refusal) => {
            // the handle she was shown may be a card she HOLDS on a room she does not
            // subscribe to: her own followed-claim record answers before any refusal
            if let Some(room) = followed_room(&crate::persona::held_claims::held(airc.peer_id().as_uuid()), raw_card) {
                return Ok(room);
            }
            match named {
                Some(asked) => crate::persona::room_resolve::resolve_room(airc, Some(asked)).await,
                None => Err(refusal),
            }
        }
    }
}

/// PURE: the room of a card she HOLDS, from her followed-claim records, when the handle she
/// typed (full id or a shown prefix, resolved exactly as the boards resolve it) names one of
/// them. Kimi held 017843bb on #cambriantech, a room she does not subscribe to: after #4836
/// her gate saw it, but `work/review 017843bb` still read "on no board of a room you are in"
/// (BigMama on #4836). See and act now read one record.
pub(super) fn followed_room(recorded: &[crate::persona::held_claims::HeldClaim], raw_card: &str) -> Option<airc_lib::Room> {
    let ids: Vec<Uuid> = recorded.iter().map(|h| h.card_id).collect();
    let id = continuum_base::id_resolve::resolve(raw_card, &ids, "card").ok()?;
    recorded.iter().find(|h| h.card_id == id).map(|h| h.room.clone())
}

/// PURE: where a submission handle comes from, said with the handles themselves. A
/// refused handle used to name only its SHAPE ("a full UUID or at least 4 leading hex
/// characters"), and Kimi, wanting a card's submissions, sent `placeholder` (2026-10-05
/// 12:01Z): a citizen with no handle cannot type one. The card's submissions are already
/// in hand where the handle is resolved, so the refusal lists them, each as the leading
/// eight hex characters the resolver accepts as a prefix.
pub(super) fn submissions_on_card(on_card: &[(Uuid, Uuid)]) -> String {
    if on_card.is_empty() {
        return "this card has no submissions yet; publish yours with work/submit".into();
    }
    let lines: Vec<String> = on_card
        .iter()
        .map(|(id, publisher)| {
            format!("{} by {}", &id.simple().to_string()[..8], &publisher.simple().to_string()[..8])
        })
        .collect();
    format!("submissions on this card: {}", lines.join(", "))
}

/// PURE: `e` with the where-it-is hint appended, keeping its error class.
pub(super) fn with_hint(e: CommandError, hint: Option<String>) -> CommandError {
    match (e, hint) {
        (CommandError::Invalid(m), Some(h)) => CommandError::Invalid(format!("{m}. {h}")),
        (CommandError::NotFound(m), Some(h)) => CommandError::NotFound(format!("{m}. {h}")),
        (e, _) => e,
    }
}

/// What makes two submissions on one card the same work. The claim is deliberately not
/// part of it: a renewed or re-taken claim has a new id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SubmissionKey {
    pub(super) id: Uuid,
    pub(super) publisher: Uuid,
    pub(super) artifact_hash: String,
    pub(super) base_sha: String,
    pub(super) instance: String,
}

/// PURE: the id of an earlier submission that is the same work as `this` (same publisher,
/// patch, base and instance), if one exists.
pub(super) fn same_submission(earlier: impl IntoIterator<Item = SubmissionKey>, this: &SubmissionKey) -> Option<Uuid> {
    earlier
        .into_iter()
        .find(|k| {
            k.publisher == this.publisher
                && k.artifact_hash == this.artifact_hash
                && k.base_sha == this.base_sha
                && k.instance == this.instance
        })
        .map(|k| k.id)
}

/// Expand an id the citizen was SHOWN into its canonical [`Uuid`].
///
/// These two verbs typed their id params as `Uuid`, so a short handle was refused by
/// SERDE — before the handler existed, before any message could name the problem, and
/// with no hint that a longer form was wanted. Every surface displays ids in the 8-char
/// short form (`card d61513e4`), which is the only form she has in front of her.
///
/// Three properties, and they are why this is not a tolerant deserializer:
/// - **Strict.** [`continuum_base::id_resolve::resolve_handle`], not the repairing `resolve`. A
///   malformed id is refused, and every supplied digit is kept so a collision can be
///   disambiguated by typing more of it.
/// - **Scoped.** Candidates are supplied by the caller from the ONE room's board it has
///   already read, or the ONE card's submissions — never a global set. An ambiguity is
///   refused against a narrow, named population.
/// - **Not an authorization.** Resolving an id grants nothing; the holder and ownership
///   guards downstream are untouched and still run on the canonical id.
pub(super) fn resolve_shown(raw: &str, candidates: &[uuid::Uuid], label: &str) -> Result<Uuid, CommandError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(CommandError::Invalid(format!("a {label} id is required")));
    }
    continuum_base::id_resolve::resolve_handle(raw, candidates, label).map_err(CommandError::Invalid)
}

/// What a submit is made of when the citizen only names her card: her checkout for
/// it, the base her patch stands on, and the patch's hash — read from her hands, never
/// typed.
pub(super) struct DerivedSubmission {
    pub(super) instance: String,
    pub(super) base_sha: String,
    pub(super) artifact: WorkArtifactReference,
}

/// The all-zero hash the AI manual's example showed and citizens copied verbatim, or a
/// manual blank (`<string>`) read back as a value.
pub(super) fn is_placeholder_hash(h: &str) -> bool {
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
/// Stage the owner's checkout at submit time, with the same path claim time takes
/// (`card_staging::stage_for_card`), so a submit never depends on a verb she cannot
/// call. `Ordinary` (a card with no repo step) is still "no tree to read a patch
/// from", said plainly; a staging failure names its stage.
pub(super) async fn stage_for_owner_on_demand(
    runtime: &std::sync::Arc<crate::persona::PersonaAircRuntime>,
    card: &airc_lib::WorkCard,
) -> Result<std::path::PathBuf, CommandError> {
    use crate::work::card_staging::Staging;
    let home = crate::paths::continuum_home()
        .map_err(|e| CommandError::Internal(format!("continuum home: {e}")))?;
    let card_uuid = card.card_id.as_uuid();
    match crate::work::card_staging::stage_for_card(&home, runtime.persona_id(), card).await {
        Staging::Ready { path } => {
            continuum_base::probe!(
                class = "work.submit.staged_on_demand",
                card_id = %short8(card_uuid),
                path = %path.display(),
                "the owner's checkout was missing at submit time — staged it here, the way her claim would have"
            );
            Ok(path)
        }
        Staging::Ordinary => Err(CommandError::Invalid(format!(
            "card {card_uuid} has no repo to stage a checkout from on this node, so there is no \
             tree to read a patch from: pass the artifact (hash, size_bytes) you are submitting \
             explicitly, or say so in the room — your patch is not the problem.",
        ))),
        Staging::Failed { stage, error } => Err(CommandError::Internal(format!(
            "staging card {card_uuid}'s checkout failed at `{stage}`: {error}. That is a substrate \
             fault on this node, not your work; say so in the room."
        ))),
    }
}

/// Renew the owner's lease as part of her act. Best effort: the wire's refusal
/// (a superseded claim, a node race) is a probe, not a reason to stop her submit,
/// because `holder_guard` already established she is the owner.
pub(super) async fn renew_owner_lease(airc: &airc_lib::Airc, card: &airc_lib::WorkCard, claim_id: ClaimId) {
    let lapsed = card.claim_expires_at_ms.is_none_or(|e| e <= EpochMs::now().millis());
    if !lapsed {
        return;
    }
    match airc
        .heartbeat_work_claim(airc_lib::HeartbeatWorkClaim {
            card_id: card.card_id,
            claim_id,
            ttl_ms: OWNER_ACT_LEASE_TTL_MS,
        })
        .await
    {
        Ok(()) => continuum_base::probe!(
            class = "work.submit.lease_renewed_by_act",
            card_id = %short8(card.card_id.as_uuid()),
            "the owner's lease had lapsed between wakes — her submit renewed it"
        ),
        Err(error) => continuum_base::probe!(
            class = "work.submit.lease_renewal_refused",
            card_id = %short8(card.card_id.as_uuid()),
            error = %error,
            "the wire refused the renewal; the submit proceeds on her ownership"
        ),
    }
}

/// The lease an owner's act renews. The claim default is 30 min; an act is a stronger
/// presence signal than a heartbeat, and a Review card waits on others for hours.
pub(super) const OWNER_ACT_LEASE_TTL_MS: u64 = 4 * 60 * 60 * 1000;

pub(super) fn holder_guard(
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
pub(super) fn instance_of_checkout(checkout: &std::path::Path, card_id: Uuid) -> Option<String> {
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
pub(super) async fn base_sha_of(benchmark: &dyn BenchmarkSubmissionAdapter, checkout: &std::path::Path, instance: &str, is_swe: bool) -> Result<String, CommandError> {
    if is_swe {
        return benchmark.base_commit(instance)
            .await?
            .map(|base| base.to_string())
            .ok_or_else(|| {
                CommandError::Invalid(format!(
                    "no dataset row names instance {instance}, so its base commit is unknown — pass base_sha"
                ))
            });
    }
    airc_lib::work_worktree::creation_base(checkout).map_err(CommandError::Invalid)
}

/// A repo worktree's creation base, advanced past any upstream history she merged in.
///
/// Kimi, 2026-09-26: her worktree was created at b06ae8430, she fast-forwarded it to the
/// canary tip to work on current code, and `work/submit` diffed from the creation base, so
/// the artifact was 177 KB of other people's merged commits (and she had written nothing
/// yet). What is hers is what no remote carries: the base is the newest merge-base of her
/// HEAD with any remote-tracking ref that still descends from the creation base. A worktree
/// that merged nothing upstream keeps its creation base. Unproven ancestry or an unreadable
/// remote-ref inventory refuses automatic derivation: a wider fallback can publish unrelated
/// upstream work as her candidate. An explicitly supplied base remains caller-owned.
pub(super) fn past_upstream(checkout: &std::path::Path, created: &str, repo: Option<&airc_work::RepoId>, repository_url: Option<&str>) -> Result<String, CommandError> {
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .current_dir(checkout)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    };
    // A shallow boundary can retain the creation object but hide its relation to HEAD.
    // Reproduced in Kimi's checkout after c82b5f9c (179 upstream files): every candidate
    // was skipped and the old base silently won. Do not turn missing proof into authorship.
    if git(&["merge-base", "--is-ancestor", created, "HEAD"]).is_none() {
        return Err(CommandError::Invalid(format!(
            "cannot derive submission base: creation anchor {created} is not a proven ancestor of HEAD \
             (history may be shallow or rewritten); restore the relevant history or pass a verified base_sha"
        )));
    }
    // Full ref names: `%(refname:short)` abbreviates refs/remotes/origin/HEAD to "origin",
    // so a HEAD alias could not be recognised by its suffix (Cormac on #4442).
    let Some(refs) = git(&["for-each-ref", "--format=%(refname) %(objectname)", "refs/remotes"]) else {
        return Err(CommandError::Invalid(
            "cannot derive submission base: remote refs could not be read; repair Git access or pass a verified base_sha".into()
        ));
    };
    // Her OWN branch on a remote is not upstream (Cormac on #4442): the runtime pushes her
    // branch after each act, so refs/remotes/<remote>/<her branch> is her own tip, the ref
    // furthest past the creation base, and choosing it would make every submission empty.
    // Skip current-branch copies and a tracked publishing ref; validated integration
    // authority below distinguishes a feature that tracks the repository's base.
    let branch = git(&["symbolic-ref", "--short", "-q", "HEAD"]).unwrap_or_default(); // unwrap_or_default: a detached HEAD has no branch to exclude
    let tracked = git(&["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{upstream}"]).unwrap_or_default(); // unwrap_or_default: no upstream configured
    let integration = submission_integration_branch(checkout, repo, &branch)?;
    let mut best: Option<(u64, String)> = None;
    for line in refs.lines().filter(|l| !l.is_empty()) {
        let Some((name, r)) = line.split_once(' ') else { continue };
        // refs/remotes/<remote>/<branch...>
        let short = name.strip_prefix("refs/remotes/").unwrap_or(name); // unwrap_or: for-each-ref under refs/remotes always carries the prefix
        let remote_branch = short.split_once('/');
        // Own publishing copies are always excluded, even if configuration names them.
        let ours = !branch.is_empty() && remote_branch.is_some_and(|(_, b)| b == branch);
        let tracked_integration = !tracked.is_empty() && short == tracked
            && remote_branch.is_some_and(|(remote, b)| {
                integration.as_deref() == Some(b) && repo.is_some_and(|repo| {
                    git(&["remote", "get-url", remote]).is_some_and(|url| {
                        let identity = crate::code::repo_registry::network_repo_identity(&url);
                        let authority = repository_url.and_then(crate::code::repo_registry::network_repo_identity);
                        identity.as_ref().is_some_and(|(_, _, name)| name == repo.as_str())
                            && identity.is_some() && identity == authority
                    })
                })
            });
        if ours || (!tracked.is_empty() && short == tracked && !tracked_integration) || name.ends_with("/HEAD") {
            continue;
        }
        let Some(mb) = git(&["merge-base", "HEAD", r]) else { continue };
        if git(&["merge-base", "--is-ancestor", created, &mb]).is_none() {
            continue; // this remote does not descend from where her work began
        }
        let ahead = git(&["rev-list", "--count", &format!("{created}..{mb}")])
            .and_then(|n| n.parse::<u64>().ok())
            .unwrap_or(0); // unwrap_or: an unreadable count ranks lowest, never wins
        if best.as_ref().map_or(true, |(b, _)| ahead > *b) {
            best = Some((ahead, mb));
        }
    }
    match best {
        Some((ahead, mb)) if ahead > 0 => {
            continuum_base::probe!(
                class = "work.submit.base_past_upstream",
                created = %&created[..created.len().min(9)],
                base = %&mb[..mb.len().min(9)],
                upstream_commits = ahead,
                "the worktree merged upstream history since it was created — the submission \
                 diffs from the newest upstream commit she holds, so only her change is published"
            );
            Ok(mb)
        }
        _ => Ok(created.to_string()),
    }
}

/// Immutable allocation evidence is authoritative when present; absence is the only
/// legacy case allowed to consult the shared repository policy.
pub(super) fn submission_integration_branch(
    checkout: &std::path::Path, repo: Option<&airc_work::RepoId>, branch: &str,
) -> Result<Option<String>, CommandError> {
    // Allocation intent is immutable; a legacy tree can instead use the SAME repository
    // policy as AIRC PR creation. A malformed/mismatched record is not a legacy tree.
    Ok(match repo {
        Some(repo) => match airc_lib::work_worktree::allocation_evidence(checkout)
            .map_err(|e| CommandError::Invalid(format!("submission allocation evidence: {e}")))? {
            Some(evidence) => {
                if evidence.repo != *repo || evidence.requested_branch != branch {
                    return Err(CommandError::Invalid("submission allocation evidence does not match the card repository and checkout branch".into()));
                }
                evidence.integration_base
            }
            None => airc_lib::work_repository::configured_base_branch(repo),
        },
        None => None,
    })
}

/// Only the tracked-integration exception needs host authority. Ordinary automatic
/// submissions and unknown policies retain their existing local behavior.
pub(super) async fn submission_repository_url(checkout: &std::path::Path, repo: &airc_work::RepoId) -> Result<Option<String>, CommandError> {
    let git = |args: &[&str]| std::process::Command::new("git").args(args).current_dir(checkout)
        .output().ok().filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
    let branch = git(&["symbolic-ref", "--short", "-q", "HEAD"]).unwrap_or_default(); // unwrap_or_default: detached has no own branch
    let integration = submission_integration_branch(checkout, Some(repo), &branch)?;
    let tracked = git(&["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{upstream}"]).unwrap_or_default(); // unwrap_or_default: no tracked ref needs no exception
    let needs_authority = tracked.split_once('/').is_some_and(|(_, name)| name != branch && integration.as_deref() == Some(name));
    if !needs_authority { return Ok(None); }
    // Conservative metadata-only network bound approved for this owner: not a latency
    // estimate or a new background retry policy. Failure leaves the submission unmade.
    let args = vec!["repo".into(), "view".into(), repo.to_string(), "--json".into(), "url".into(), "--jq".into(), ".url".into()];
    let url = continuum_hands::commands::code::github::run_gh_within(checkout, &args, std::time::Duration::from_secs(30))
        .await.map_err(CommandError::from)?;
    if !crate::code::repo_registry::network_repo_identity(&url).is_some_and(|(_, _, name)| name == repo.as_str()) {
        return Err(CommandError::Invalid("GitHub repository authority returned an invalid or mismatched URL".into()));
    }
    Ok(Some(url))
}

/// Read the submission off her checkout: instance, base, and the patch's hash + size.
/// Parts she supplied herself are kept; the artifact is always recomputed here, so a
/// placeholder can never reach the board.
pub(super) async fn derive_submission(
    benchmark: &dyn BenchmarkSubmissionAdapter,
    checkout: &std::path::Path,
    card_id: Uuid,
    given_instance: Option<String>,
    given_base: Option<String>,
    repo: Option<&airc_work::RepoId>,
    listed_submodules: &[String],
) -> Result<DerivedSubmission, CommandError> {
    let is_swe = checkout.iter().any(|c| c == "swe");
    let instance = given_instance
        .or_else(|| instance_of_checkout(checkout, card_id))
        .unwrap_or_else(|| card_id.to_string()); // unwrap_or: instance_of_checkout always yields the card id as its floor
    let base_sha = match given_base {
        Some(b) => b,
        None => {
            let created = base_sha_of(benchmark, checkout, &instance, is_swe).await?;
            if is_swe {
                created
            } else {
                let repository_url = match repo {
                    Some(repo) => submission_repository_url(checkout, repo).await?,
                    None => None,
                };
                past_upstream(checkout, &created, repo, repository_url.as_deref())?
            }
        }
    };
    // Only the submodule moves she lists ride: one that came in from her tree unasked is
    // refused by name, with how to list it and the restore, before any patch is read or
    // retained (card b3ecd1ac; Codex on #5402).
    let excludes = crate::persona::staged_workspace::SOLUTION_PATH_EXCLUDES;
    let moves = super::submodule_drift::submodule_moves_in(checkout, &base_sha, excludes).map_err(CommandError::Internal)?;
    if let Some(refusal) = super::submodule_drift::refuse_unlisted(&moves, listed_submodules, &base_sha) {
        return Err(CommandError::Invalid(refusal));
    }
    let base = airc_work::GitObjectId::new(&base_sha)
        .map_err(|e| CommandError::Invalid(format!("candidate base must name a single git commit: {e}")))?;
    let patch = benchmark.candidate_diff(checkout, Some(&base))?;
    if patch.trim().is_empty() {
        return Err(CommandError::Invalid(format!(
            "nothing to submit: {} has no changes since {} (substrate paths excluded) — \
             a submit publishes a patch, not an intention",
            checkout.display(),
            &base_sha[..base_sha.len().min(9)]
        )));
    }
    // Retain the exact candidate before publishing its identity: the checkout can
    // change after this call, so a later git diff is not the submitted artifact.
    let store = crate::media::artifact::store().map_err(CommandError::Internal)?;
    let artifact = retain_patch(&store, &patch)?;
    Ok(DerivedSubmission {
        instance,
        base_sha,
        artifact,
    })
}

pub(super) fn retain_patch(store: &airc_blobs::FsStore, patch: &str) -> Result<WorkArtifactReference, CommandError> {
    use airc_blobs::ContentAddressedStore;
    let hash = store.put(patch.as_bytes()).map_err(|e| {
        CommandError::Internal(format!("could not retain submission patch: {e}"))
    })?;
    Ok(WorkArtifactReference {
        hash: hash.to_hex(),
        size_bytes: patch.len() as u64,
        mime: Some("text/x-patch".to_string()),
    })
}

/// The evidence reference of a reviewer's own words: SHA-256 over the text, its length.
pub(super) fn artifact_of_text(text: &str) -> WorkArtifactReference {
    use sha2::Digest;
    WorkArtifactReference {
        hash: format!("{:x}", sha2::Sha256::digest(text.as_bytes())),
        size_bytes: text.len() as u64,
        mime: Some("text/plain".to_string()),
    }
}

/// The artifact reference of a patch: SHA-256 over its bytes, its length, `text/x-patch`.
#[cfg(test)]
pub(super) fn artifact_of_patch(patch: &str) -> WorkArtifactReference {
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
    /// Exact published claim, not the card's current (possibly renewed) claim.
    #[ts(type = "string")]
    pub claim_id: Uuid,
    /// The artifact is a patch against this base in this activity instance.
    pub base_sha: String,
    pub instance: String,
    pub artifact: WorkArtifactReference,
    #[ts(type = "number")]
    pub submitted_at_ms: u64,
    /// Present only after the selected copy and reservations were persisted.
    #[ts(type = "string | null")]
    pub bound_staged_revision_id: Option<Uuid>,
    /// What happens next, in one line: after a submit, nothing more on this card (card aff659aa).
    pub next: String,
}
