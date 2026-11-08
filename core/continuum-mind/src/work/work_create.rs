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
        crate::persona::work_board::board_index::wrote(&airc, &room);
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
    const ALIASES: &'static [&'static str] = crate::cognition::work_preconditions::WorkVerb::Release.aliases();
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
        crate::persona::work_board::board_index::wrote_card(&airc, card_id);
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
    const ALIASES: &'static [&'static str] = crate::cognition::work_preconditions::WorkVerb::State.aliases();
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
        // THE PRECONDITION, at the mutation boundary, against the card as the board holds it now
        // (`work_preconditions::state`, the one owner the surface reads too):
        // - A HOLDER NEVER "OPENS" HER OWN CARD. `open` from the live holder is a release with no
        //   note and no hand-off, and almost never intent (Atlas, 2026-09-13 12:58Z: "let me get
        //   the card", then `work/state open` on the card she held with a real edit in her
        //   checkout). Reading is `work/get`; handing back is `work/release`. The operator and a
        //   non-holder may still reopen a card: a board repair, not a slip.
        // - A STATE THAT MEANS "I HOLD THIS" IS A CLAIM. `claimed` / `in_progress` are the holder's
        //   columns, and there is ONE way to hold a card, `work/claim` (lease, owner, staged
        //   checkout): Joaquin took a card with `work/state claimed` (2026-09-13 03:0xZ), which
        //   moved the column and set no owner, and the sweep reopened it under her.
        // - Her own live review returns to in_progress for a revision under the same claim
        //   (re-claiming refuses Review as settled, so the owner could never leave it: Kimi,
        //   2026-09-30). It grants no ownership and does not mark the review passed.
        // - ANOTHER'S CARD IS TAKEN OVER, ON THE RECORD (Joel, 2026-10-06: anyone may take any card at
        //   any time). Her claim releases theirs naming whom it was taken from, then the column moves.
        // A card on no board she stands in is the board's to decide; her column still goes
        // through the claim path.
        let now_ms = EpochMs::now().millis();
        let board_card = card_in_subscribed_rooms(&airc, card_id).await;
        let route = match &board_card {
            Some((_, card)) => work_preconditions::decide(work_preconditions::state(card, airc.peer_id(), state, now_ms), card, airc.peer_id(), now_ms)
                .map_err(super::refused)?,
            None if matches!(state, CardState::Claimed | CardState::InProgress) => work_preconditions::StateRoute::ThroughClaim,
            None => work_preconditions::StateRoute::Direct,
        };
        if let (work_preconditions::StateRoute::Takeover { from }, Some((room, _))) = (route, &board_card) {
            // airc's claim takes a held card over: `ClaimReleased { taken_over_from }` by her, then her claim.
            let claim_id = airc
                .claim_work_card_with_provenance_in(room, ClaimWorkCard { card_id, ttl_ms: DEFAULT_CLAIM_TTL_MS }, airc_work::ClaimOrigin::Explicit, None)
                .await
                .map_err(|e| CommandError::Internal(e.to_string()))?;
            crate::persona::work_board::board_index::wrote(&airc, room);
            continuum_base::probe!(
                class = "work.state.taken_over",
                card_id = %card_id.as_uuid(),
                from = %from.as_uuid(),
                claim_id = %claim_id.as_uuid(),
                state = state_str(&state),
                "another's card taken over on the record before its column moved"
            );
        }
        if route == work_preconditions::StateRoute::ThroughClaim {
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
