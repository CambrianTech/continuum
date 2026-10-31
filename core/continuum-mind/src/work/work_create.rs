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
        let room = crate::persona::room_resolve::resolve_room(airc, Some(&p.room)).await?;
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

// ─────────────────────────── work/release ────────────────────────
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

// ─────────────────────────── work/state ──────────────────────────
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
         blocked if stuck, closed when done (a card with a PR closes when it merges).";
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
                    airc_core::shown_id::shown_form(card_id.as_uuid())
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
            continuum_base::probe!(
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
