use super::*;

impl WorkCreate {
    /// The card lands on the NAMED room's board under the caller's own airc identity.
    pub(super) async fn create(airc: &Airc, p: WorkCreateParams) -> Result<WorkCreateResult, CommandError> {
        // A blank room would reach resolve_room as "unnamed" and land in the current room,
        // the very default this field exists to refuse (Codex on #4550). Resolved FIRST:
        // the room is the second thing that can say which repo (below).
        if p.room.trim().is_empty() {
            return Err(CommandError::Invalid(
                "work/create: room is required: name the activity room whose board gets the card"
                    .into(),
            ));
        }
        let room = crate::modules::room_resolve::resolve_room(airc, Some(&p.room)).await?;
        // A named repo wins; then the card she holds (Kimi, 2026-09-28: the doc's example
        // named this repo, so her first career-wrangler slice card was filed against
        // continuum); then THE ROOM (Kimi, 2026-10-05: six refusals filing cards in a
        // project room, because a project IS a repo and the room already declared it on
        // its binding, which nothing read). A blank repo is a mistake to name, not a
        // request to infer (Codex on #4571).
        let repo = match p.repo.as_deref().map(str::trim) {
            Some("") => {
                return Err(CommandError::Invalid(
                    "work/create: repo is blank: name it (owner/name) or omit it to use the \
                     card you hold or the room's declared repo"
                        .into(),
                ))
            }
            Some(named) => RepoId::new(named.to_string())
                .map_err(|e| CommandError::Invalid(format!("invalid repo: {e:?}")))?,
            None => match held_repo(airc).await? {
                Some(held) => held,
                None => match room_repo(airc, &room).await? {
                    Some(declared) => declared,
                    // HER action leads (BigMama, from persona.act.refused: three refusals in the
                    // org room after #4762, whose text led with the OPERATOR's fix). Hers is one
                    // argument; the repos she can see are named so she need not guess one.
                    None => {
                        return Err(CommandError::Invalid(repo_refusal(
                            p.room.trim(),
                            &repos_she_holds(airc).await,
                        )))
                    }
                },
            },
        };
        let mut req = CreateWorkCard::new(
            repo,
            p.title,
            Priority::from(p.priority.unwrap_or(CardPriority::P2)), // unwrap_or: the documented default
        );
        req.body = p.body;
        let card_id = airc
            .create_work_card_in(&room, req)
            .await
            .map_err(|e| CommandError::Internal(e.to_string()))?;
        Ok(WorkCreateResult {
            card_id: card_id.as_uuid().to_string(),
        })
    }
}

/// Release this persona's claim on a card.
pub struct WorkRelease {
    pub registry: PersonaAircRuntimeRegistry,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct WorkReleaseParams {
    /// The card id (UUID) whose claim to release.
    pub card_id: String,
    /// The claim_id returned by work/claim.
    pub claim_id: String,
    /// Optional reason (recorded on the release event).
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct WorkReleaseResult {
    pub released: bool,
}

#[async_trait]
impl ActionCommand for WorkRelease {
    const NAME: &'static str = "work/release";
    const ALIASES: &'static [&'static str] = &["release_task"];
    // core room workflow — HANDING BACK. Without it a citizen who realizes a card is
    // not hers has only one exit: hold it until the lease lapses. (#339)
    const NATIVE: bool = true;
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Release your claim on a work card (pass card_id + the claim_id from work/claim; \
         short 8-char ids from the board are accepted for both).";
    type Params = WorkReleaseParams;
    type Output = WorkReleaseResult;

    async fn run(
        &self,
        ctx: &Ctx,
        p: WorkReleaseParams,
    ) -> Result<WorkReleaseResult, CommandError> {
        let airc = persona_airc(&self.registry, ctx, "work commands")?;
        let card_id = resolve_card_id(&airc, &p.card_id).await?;
        let claim_id = resolve_claim_id(&airc, &p.claim_id).await?;
        // Released where the card lives (card e2aea0e6): the room-scoped form when a
        // subscribed board holds it, so her current room is never moved for a card she
        // can already see. The join-and-retry below stays ONLY for a card on no board she
        // stands in (a fresh bench round's run room, 2026-09-05) — there the join is the
        // way to the card, not a side effect.
        let mut attempt = match room_holding_card(&airc, card_id).await {
            Some(room) => {
                airc.release_work_claim_in(
                    &room,
                    ReleaseWorkClaim {
                        card_id,
                        claim_id,
                        reason: p.reason.clone(),
                    },
                )
                .await
            }
            None => {
                airc.release_work_claim(ReleaseWorkClaim {
                    card_id,
                    claim_id,
                    reason: p.reason.clone(),
                })
                .await
            }
        };
        if matches!(
            attempt,
            Err(airc_lib::AircError::WorkCardNotInCurrentRoom { .. })
        ) && follow_card_room(&airc, card_id, "work/release")
            .await
            .is_some()
        {
            attempt = airc
                .release_work_claim(ReleaseWorkClaim {
                    card_id,
                    claim_id,
                    reason: p.reason,
                })
                .await;
        }
        attempt.map_err(|e| CommandError::Internal(e.to_string()))?;
        crate::persona::work_burst::note_hold_boundary(airc.peer_id().as_uuid()); // a release is a hold boundary too
        crate::persona::held_claims::forget(airc.peer_id().as_uuid(), airc.home(), card_id.as_uuid());
        Ok(WorkReleaseResult { released: true })
    }
}

/// Move a card through its lifecycle (open→in_progress→review→closed, etc).
pub struct WorkState {
    pub registry: PersonaAircRuntimeRegistry,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct WorkStateParams {
    /// The card id (UUID) to transition.
    pub card_id: String,
    /// New state: open | claimed | in_progress | blocked | review | merged | closed.
    pub state: String,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct WorkStateResult {
    pub card_id: String,
    pub state: String,
}

#[async_trait]
impl ActionCommand for WorkState {
    const NAME: &'static str = "work/state";
    const ALIASES: &'static [&'static str] = &["update_task", "close_task"];
    // core room workflow — SAYING DONE. This is the missing half of #339: citizens were
    // offered claim/list/get and NOTHING that writes a card's lifecycle back, so every
    // claim could only end in a lapsed lease and a completed card was indistinguishable
    // from an abandoned one. The DESCRIPTION below has coached the whole lifecycle since
    // the day it was written — to a reader who was never shown it.
    const NATIVE: bool = true;
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Move a work card through its lifecycle: in_progress when you start, review when a PR is up, \
         blocked if stuck, closed when done. States: open|claimed|in_progress|blocked|review|merged|closed.";
    type Params = WorkStateParams;
    type Output = WorkStateResult;

    async fn run(&self, ctx: &Ctx, p: WorkStateParams) -> Result<WorkStateResult, CommandError> {
        let airc = persona_airc(&self.registry, ctx, "work commands")?;
        let card_id = resolve_card_id(&airc, &p.card_id).await?;
        let state = parse_state(&p.state)?;
        // A HOLDER NEVER "OPENS" HER OWN CARD. `open` from the live holder is a release
        // with no note and no hand-off — and it is almost never intent: Atlas, 2026-09-13
        // 12:58Z, "let me get the card" → `work/state` (open) on the card she held with a
        // real edit in her checkout; Joaquin pulled it and re-staged fresh. Reading is
        // `work/get`; handing back is `work/release` (say why). The operator and a
        // non-holder may still reopen a card — that is a board repair, not a slip.
        if state == CardState::Open {
            let held = crate::persona::airc_runtime::board_held_by(airc.as_ref())
                .await
                .map(|cards| cards.iter().any(|c| c.card_id == card_id))
                .unwrap_or(false); // unwrap_or: an unreadable board cannot prove she holds it — the reopen proceeds as before
            if held {
                return Err(CommandError::Invalid(format!(
                    "you HOLD card {} — `open` from its holder would drop your claim and leave your diff \
                     behind. To read the card: work/get. To hand it back: work/release (with a note for \
                     the next holder). To mark progress: work/state in_progress|review|done.",
                    short8(card_id.as_uuid())
                )));
            }
        }
        // A STATE THAT MEANS "I HOLD THIS" IS A CLAIM. `claimed` / `in_progress` are the
        // holder's columns; setting one is unambiguous intent to hold the card, and
        // there is ONE way to hold a card — `work/claim` (lease + owner + staged
        // checkout). Measured 2026-09-13 03:0xZ: Joaquin took matplotlib-21568 with
        // `work/state claimed`, which moved the column and set no owner; the sweep read
        // "claimed, owner none = held by nobody" and reopened it while she worked it
        // believing it hers. Idempotent for the holder (the claim verb's already-yours
        // arm), so `in_progress` on a held card costs one re-claim, never a refusal.
        // Revision resumes the owner's existing lease. Re-claiming first refuses
        // Review as settled, so the owner can never leave that column (Kimi,
        // 2026-09-30). Only explicit in_progress on her live review may skip the
        // claim path; it neither grants ownership nor marks review as passed.
        let resume_review = if state == CardState::InProgress {
            card_in_subscribed_rooms(&airc, card_id).await.is_some_and(|(_, card)| {
                resumes_held_review(&card, state, airc.peer_id(), EpochMs::now().millis())
            })
        } else {
            false
        };
        if matches!(state, CardState::Claimed | CardState::InProgress) && !resume_review {
            let claim = WorkClaim {
                registry: self.registry.clone(),
            }
            .run(
                ctx,
                WorkClaimParams {
                    origin: WorkClaimOrigin::Explicit,
                    card_id: p.card_id.clone(),
                    ttl_ms: None,
                },
            )
            .await?;
            crate::probe!(
                class = "work.state.held_through_claim",
                card_id = %card_id.as_uuid(),
                state = state_str(&state),
                claim_id = %claim.claim_id,
                "a holder's column set through the one claim path — owner + lease + staging, never a bare column"
            );
            if state == CardState::Claimed {
                return Ok(WorkStateResult {
                    card_id: p.card_id,
                    state: state_str(&state).to_string(),
                });
            }
        }
        let actor = ctx.caller.as_ref().map(|c| c.peer_id.as_uuid());
        let landed = advance_card_state_effective(&airc, card_id, state, "work-state-verb", actor)
            .await
            .map_err(CommandError::Internal)?;
        Ok(WorkStateResult {
            card_id: p.card_id,
            // The state that LANDED: under the review gate an owner's `done` reads
            // back as `review` — she must see that her card now waits on a reviewer.
            state: state_str(&landed).to_string(),
        })
    }
}

/// Transition a work card's lifecycle state AND feed the grade/lifecycle
/// subscribers in-process — the ONE place a card's state is written and
/// announced (the compression law: `work/state` the verb, the held-work settle
/// edge, and any future writer all call THIS, never re-implement the change +
/// emit pair). Writing without the direct emit is the #434 delivery-hole that
/// let finished work grade as NOTHING; the (card,state) ring in
/// `emit_card_state_changed` makes the direct feeder and the wire echo publish
/// exactly once when both fire, so callers may always emit.
///
/// `via` labels the feeder on the probe stream so two writers of the same
/// transition are distinguishable.
pub(crate) async fn advance_card_state(
    airc: &Arc<Airc>,
    card_id: WorkCardId,
    state: CardState,
    via: &'static str,
    actor: Option<Uuid>,
) -> Result<(), String> {
    advance_card_state_effective(airc, card_id, state, via, actor)
        .await
        .map(|_| ())
}

/// What a holder's checkout says about her work: something to review, nothing,
/// or a checkout that could not be read (never silently either of the others).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ChangeVerdict {
    Changed,
    NoChange,
    CouldNotLook(String),
}

/// Bound on each git read under the review gate. A checkout on a cold disk answers
/// in well under this; a hang is reported as CouldNotLook, never waited out.
pub(super) const CHECKOUT_READ_BOUND: std::time::Duration = std::time::Duration::from_secs(10);

/// Pure half of the write check: dirty tree, or a commit made after the checkout
/// was staged (HEAD's committer time vs the `.git` entry's mtime — base-free, so it
/// holds for a clone and for a worktree alike).
pub(crate) fn change_verdict(
    porcelain: &str,
    head_commit_ts: Option<u64>,
    staged_at_ts: Option<u64>,
) -> ChangeVerdict {
    if !porcelain.trim().is_empty() {
        return ChangeVerdict::Changed;
    }
    match (head_commit_ts, staged_at_ts) {
        (Some(head), Some(staged)) if head > staged => ChangeVerdict::Changed,
        (Some(_), Some(_)) => ChangeVerdict::NoChange,
        (None, _) => ChangeVerdict::CouldNotLook("HEAD commit time unreadable".into()),
        (_, None) => ChangeVerdict::CouldNotLook("staging time unreadable".into()),
    }
}

/// The plumbing half: two bounded git reads plus one stat, folded by [`change_verdict`].
pub(crate) fn checkout_change(root: &std::path::Path) -> ChangeVerdict {
    use crate::system_resources::bounded_command::probe;
    let root_s = root.to_string_lossy().to_string();
    let status = probe(
        "git",
        &["-C", &root_s, "status", "--porcelain"],
        CHECKOUT_READ_BOUND,
    );
    let Some(porcelain) = status.stdout_if_ok() else {
        return ChangeVerdict::CouldNotLook(format!("git status: {}", status.outcome()));
    };
    let head = probe(
        "git",
        &["-C", &root_s, "log", "-1", "--format=%ct"],
        CHECKOUT_READ_BOUND,
    );
    let head_ts = head
        .stdout_if_ok()
        .and_then(|t| t.trim().parse::<u64>().ok());
    let staged_ts = std::fs::metadata(root.join(".git"))
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| crate::time::EpochMs::of(t).ok())
        .map(crate::time::EpochMs::secs);
    change_verdict(porcelain, head_ts, staged_ts)
}

/// The state-advance seam WITH the review gate (team-recipe Arm 2, 2026-09-03).
/// Returns the state that actually landed:
/// - a verdict on a REVIEW card: `done` = pass → the review retires and the PARENT
///   closes (its grade fires); `blocked` = send back → the review retires and the
///   parent returns to `in_progress` (the reviewer's critique is already in the room);
/// - an owner's `done` on a GATED parent → `review` + a sibling review card is
///   posted for a non-owner to pull;
/// - everything else advances as asked.
/// One seam for the `work/state` verb and the deterministic held-work settle, so
/// the gate cannot be bypassed by either.
/// The feeder label a recorded verdict uses — the one finisher that skips the
/// review gate, because the grade IS the review.
pub(crate) const VIA_VERDICT: &'static str = "verdict";

pub(crate) async fn advance_card_state_effective(
    airc: &Arc<Airc>,
    card_id: WorkCardId,
    state: CardState,
    via: &'static str,
    // The persona making the transition, when one is (the verb's caller, the
    // held-work settle's owner). The operator passes None and is never gated.
    actor: Option<Uuid>,
) -> Result<CardState, String> {
    use crate::cognition::bench_round as round;
    let finishing = is_concluded(&state);
    // THE GATE READS THE ROOM (card fa4aaaaa): a room that declares a review policy
    // decides; one that declares none keeps the round's gate (bench rooms, until their
    // recipes declare the policy). Read only on transitions that can finish.
    let room_gate = if finishing || matches!(state, CardState::Review | CardState::Blocked) {
        super::review_gate::room_review(airc, card_id).await
    } else {
        None
    };
    let gated = match &room_gate {
        Some(r) => r.outstanding(),
        None => round::review_required(card_id.as_uuid()),
    };
    // Card 381ccf3a: a citizen's `done` (or `review`) on a GATED card must carry a
    // write. Measured 2026-09-06/07: cards moved to REVIEW with zero edits as a
    // release valve — the reviewer then reviewed nothing and the grade ran on the
    // base commit. Her acting root is the checkout the card staged for her; no
    // change there since staging = nothing to review.
    if (finishing || state == CardState::Review)
        && gated
        && round::review_parent(card_id.as_uuid()).is_none()
    {
        if let Some(root) = actor.and_then(crate::cognition::persona_workspace::acting_root_of) {
            match checkout_change(&root) {
                ChangeVerdict::NoChange => {
                    crate::probe!(
                        class = "work.review.refused_no_write",
                        card = %short8(card_id.as_uuid()),
                        persona = %actor.map(|a| a.to_string()).unwrap_or_default(),  // unwrap_or: probe label; actor is Some on this branch
                        root = %root.display(),
                        "a done on a gated card with no change in her checkout — refused; \
                         write the fix or release the card"
                    );
                    return Err(format!(
                        "done refused: {} has no change since it was staged (clean tree, no new \
                         commit) — there is nothing for a reviewer to review. Write the fix \
                         (code/edit) and mark done again, or release the card (work/release) \
                         so another holder can take it.",
                        root.display()
                    ));
                }
                ChangeVerdict::Changed => {}
                ChangeVerdict::CouldNotLook(reason) => crate::probe!(
                    class = "work.review.write_check_unknown",
                    card = %short8(card_id.as_uuid()),
                    root = %root.display(),
                    reason = %reason,
                    "could not read her checkout — the transition proceeds; the reviewer decides"
                ),
            }
        }
    }
    let review_parent = match round::review_parent(card_id.as_uuid()) {
        Some(p) => Some(p),
        None if finishing || state == CardState::Blocked => native_review_parent(airc, card_id).await,
        None => None,
    };
    if let Some(parent) = review_parent {
        if finishing || state == CardState::Blocked {
            let passed = finishing;
            round::settle_review_card(card_id.as_uuid(), passed);
            raw_advance(airc, card_id, CardState::Closed, via).await?;
            let parent_id = WorkCardId::from_uuid(parent);
            // A room with a policy counts PASSING REVIEWS by distinct reviewers on the
            // parent's submission: one review card closing is not enough when it asks
            // for more, and the parent waits in Review (card fa4aaaaa).
            if passed {
                if let Some(parent_gate) = super::review_gate::room_review(airc, parent_id).await {
                    if parent_gate.outstanding() {
                        crate::probe!(
                            class = "work.review.awaiting_more",
                            parent = %short8(parent),
                            passed = parent_gate.passed as u64,
                            required = parent_gate.policy.required,
                            "a review closed, but the room's policy asks for more passing reviews — the parent waits"
                        );
                        return Ok(CardState::Review);
                    }
                }
            }
            // A PASS ON A CARD WITH AN OPEN PR LEAVES IT MERGEABLE. The card finishes through
            // its merge, as Merged; closing it here made `airc work merge` refuse it ("state
            // is Closed, but merge requires Review"). Kimi passed #4825's submission on
            // 2026-10-06 and the card closed with its PR open, so the merge took a hand move
            // back to Review. A card with no PR (a bench round's) still closes on the pass.
            if passed {
                if let Some((_, parent_card)) = card_in_subscribed_rooms(airc, parent_id).await {
                    if parent_card.pull_request.is_some() && parent_card.state != CardState::Merged {
                        if parent_card.state != CardState::Review {
                            raw_advance(airc, parent_id, CardState::Review, via).await?;
                        }
                        crate::probe!(
                            class = "work.review.passed_awaiting_merge",
                            review = %short8(card_id.as_uuid()),
                            parent = %short8(parent),
                            "reviewer's pass on a card with an open PR — it stays in Review for \
                             its merge, which moves it to Merged"
                        );
                        return Ok(CardState::Review);
                    }
                }
            }
            let next = if passed {
                CardState::Closed
            } else {
                CardState::InProgress
            };
            crate::probe!(
                class = "work.review.verdict",
                review = %short8(card_id.as_uuid()),
                parent = %short8(parent),
                passed,
                "reviewer's verdict — a pass closes the parent (grade fires), a send-back \
                 returns it to in_progress"
            );
            raw_advance(airc, parent_id, next, via).await?;
            return Ok(next);
        }
    }
    // A VERDICT is the review's outcome: it closes the parent outright. Every
    // other finisher on a gated card goes to review first.
    if finishing && via != VIA_VERDICT && gated {
        raw_advance(airc, card_id, CardState::Review, via).await?;
        match open_review_card(airc, card_id).await {
            Ok(review) => crate::probe!(
                class = "work.review.opened",
                parent = %short8(card_id.as_uuid()),
                review = %short8(review.as_uuid()),
                "owner's done became review — a sibling review card is on the deck for a \
                 non-owner; only the reviewer's done closes this card"
            ),
            Err(e) => crate::probe!(
                class = "work.review.open_failed",
                parent = %short8(card_id.as_uuid()),
                error = %e,
                "the card is in review but no review card could be posted — the next \
                 done retries the gate"
            ),
        }
        return Ok(CardState::Review);
    }
    raw_advance(airc, card_id, state, via).await?;
    Ok(state)
}

/// Post the sibling review card for `parent` into the parent's own room, as the
/// caller's identity. The card BODY carries the reviewer's role prompt — recipe
/// data on the card, never a code branch in cognition.
pub(super) async fn open_review_card(airc: &Arc<Airc>, parent: WorkCardId) -> Result<WorkCardId, String> {
    let (room, card) = card_in_subscribed_rooms(airc, parent)
        .await
        .ok_or_else(|| "parent card is on no subscribed room's board".to_string())?;
    let Some((_, instance)) = crate::cognition::bench_title::parse_card_title(&card.title) else {
        // A room's own card (not a bench card): its review card comes from the parent
        // itself, under the room's policy (card fa4aaaaa). The bench branch below is
        // unchanged.
        return open_room_review_card(airc, &room, &card, parent, submission::latest_submission(&card).map(|s| s.submission_id)).await;
    };
    let owner = card
        .owner
        .and_then(|o| {
            crate::persona::airc_runtime_registry::PersonaAircRuntimeRegistry::try_global()
                .and_then(|reg| reg.get(o.as_uuid()))
                .map(|rt| rt.agent_name().to_string())
        })
        .unwrap_or_else(|| "the owner".to_string()); // unwrap_or: owner not resident = a neutral name in the review body
    let title = crate::cognition::bench_title::review_card_title(parent.as_uuid(), &instance, &owner);
    let p8 = parent.as_uuid().simple().to_string()[..8].to_string();
    let body = format!(
        "Review {owner}'s fix for `{instance}` (card {p8}). You are the fresh pair of \
         eyes: READ-ONLY. Do not edit their tree.\n\n\
         Their checkout is {owner}'s workspace at `swe/{instance}/` — your hands are rooted \
         there for this card. Run `git diff` to see the change; run the repo's tests for the \
         issue.\n\n\
         Judge the diff against the card's definition of done: does it fix the CAUSE with \
         the smallest edit, without touching the tests, and do the tests pass?\n\n\
         Verdict, with evidence — every claim cites a file:line or a test result:\n\
         - PASS: say `work/state <this card's id> done` — that closes {owner}'s card and \
         fires the grade.\n\
         - SEND BACK: say what must change (cited), then `work/state <this card's id> \
         blocked` — the card returns to {owner} in progress.\n\
         Report gaps, not style."
    );
    // The review lands on the PARENT's board, named, never through the shared current-room
    // pointer: parallel dones on two boards would otherwise race one join against the other
    // (IntelMac on #4838; the race #4838 removes from work/state).
    let mut req = CreateWorkCard::new(card.repo.clone(), title, Priority::P1).reviewing(parent);
    req.body = Some(body);
    let review = airc
        .create_work_card_in(&room, req)
        .await
        .map_err(|e| e.to_string())?;
    crate::cognition::bench_round::register_review_card(parent.as_uuid(), review.as_uuid())
        .ok_or_else(|| "parent left its round before the review was registered".to_string())?;
    Ok(review)
}

/// Post the review card for a ROOM's own card (not a bench card): titled and described
/// from the parent, linked natively (`reviewing(parent)`), never registered with a round.
/// Whoever the room's policy admits reviews it with `work/review` (card fa4aaaaa).
pub(super) async fn open_room_review_card(
    airc: &Arc<Airc>,
    room: &airc_lib::Room,
    card: &airc_lib::WorkCard,
    parent: WorkCardId,
    submission: Option<airc_work::SubmissionId>,
) -> Result<WorkCardId, String> {
    let owner = card
        .owner
        .and_then(|o| {
            crate::persona::airc_runtime_registry::PersonaAircRuntimeRegistry::try_global()
                .and_then(|reg| reg.get(o.as_uuid()))
                .map(|rt| rt.agent_name().to_string())
        })
        .unwrap_or_else(|| "the owner".to_string()); // unwrap_or: owner not resident = a neutral name in the review body
    let p8 = parent.as_uuid().simple().to_string()[..8].to_string();
    let title: String = format!("review: {}", card.title).chars().take(160).collect();
    let body = format!(
        "Review {owner}'s work on card {p8} (\"{}\"). Read the latest submission          (work/submission) and the change it carries, judge it against the card, then give          your verdict with work/review (passed or failed, with evidence). This room's review          policy decides how many passing reviews finish the card and whether the author may          review her own work.",
        card.title
    );
    // on the parent's board, named: never through the shared current-room pointer (#4838)
    let mut req = CreateWorkCard::new(card.repo.clone(), title, Priority::P1).reviewing(parent);
    req.body = Some(body);
    match submission {
        Some(candidate) => airc.ensure_submission_review_in(room, parent, candidate, req).await,
        None => airc.create_work_card_in(room, req).await,
    }.map_err(|e| e.to_string())
}

/// The card a review card reviews, by airc's native link (`reviews`), for review cards no
/// round registered (a room's own cards).
pub(super) async fn native_review_parent(airc: &Arc<Airc>, card_id: WorkCardId) -> Option<Uuid> {
    card_in_subscribed_rooms(airc, card_id)
        .await
        .and_then(|(_, card)| card.reviews.map(|p| p.as_uuid()))
}

/// The raw advance + the bus event — what [`advance_card_state`] was before the gate.
pub(super) async fn raw_advance(
    airc: &Arc<Airc>,
    card_id: WorkCardId,
    state: CardState,
    via: &'static str,
) -> Result<(), String> {
    // THE CARD NAMES ITS ROOM. A board she can see holds the card: mutate it THERE, without
    // reading or moving the scope's one current-room pointer. Kimi on the 5090 (2026-10-06
    // 22:5xZ) closed 8 duplicate cards in ONE turn, in parallel, across #continuum and
    // #cambriantech: every call that refused in the current room "followed" its card by
    // switching that shared pointer, and the parallel follows flipped it under each other, so
    // 7 retries landed in the wrong room again and refused. Only a card on no visible board
    // (a bench round's run room) still takes the current room + follow path below.
    let mut attempt = match room_holding_card(airc, card_id).await {
        Some(room) => {
            airc.change_work_card_state_in(&room, ChangeWorkCardState { card_id, state })
                .await
        }
        None => {
            airc.change_work_card_state(ChangeWorkCardState { card_id, state })
                .await
        }
    };
    if matches!(
        attempt,
        Err(airc_lib::AircError::WorkCardNotInCurrentRoom { .. })
    ) && follow_card_room(airc, card_id, "work/state")
        .await
        .is_some()
    {
        attempt = airc
            .change_work_card_state(ChangeWorkCardState { card_id, state })
            .await;
    }
    attempt.map_err(|e| e.to_string())?;
    // A REOPEN MEANS NOBODY HOLDS IT. The column moved to Open but the claim ledger
    // kept the holder's lease alive: her roster row still listed the card, her
    // work turns kept acting on it, and the deck offered a card with a live claim
    // to everyone else (2026-09-12, three coders on a paused round for 2.5 h).
    // The reopen releases the claim through the same verb the holder would use.
    if state == CardState::Open {
        release_live_claim_on_reopen(airc, card_id, via).await;
        // ...and the round tracker's settled mark, or the deck never offers it again.
        crate::cognition::bench_round::reopen_card(card_id.as_uuid());
    }
    // The CARD's room, never `current_room()` — boards are per-room and the grade
    // subscriber refuses an event with no room.
    let room_id = room_holding_card(airc, card_id)
        .await
        .map(|r| r.channel.as_uuid().to_string())
        .unwrap_or_default(); // unwrap_or: a card we cannot place carries an empty room on the bus
    emit_card_state_changed(
        serde_json::json!({
            "card_id": card_id.as_uuid().to_string(),
            "state": serde_json::to_value(state).unwrap_or(serde_json::Value::Null),  // unwrap_or: an unserializable state = null on the bus, never a panic on the verb
            "room_id": room_id,
        }),
        via,
    )
    .await;
    Ok(())
}
