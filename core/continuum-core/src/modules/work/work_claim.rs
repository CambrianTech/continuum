use super::*;

#[async_trait]
impl ActionCommand for WorkClaim {
    const NAME: &'static str = "work/claim";
    const ALIASES: &'static [&'static str] = &["claim_task"];
    const NATIVE: bool = true; // core room workflow — claiming shared-board work as yourself
                               // AiSafe since 2026-07-10: claiming/working a card AS YOURSELF is the
                               // self-scoped cooperative act the shared board exists for. It was
                               // Privileged, so every persona claim all day was structurally impossible —
                               // narrated claims, then Atlas's honest real attempt bounced off the gate
                               // ("I don't have access to work/claim") while the board, the room, and the
                               // operators all urged them to claim. The lifecycle verbs (claim/release/
                               // state/heartbeat) act only on the caller's own identity + lease;
                               // work/create (board curation) stays Privileged.
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Claim a work card on the shared airc board as yourself, so others see you own it. \
         Pass the card_id from the board. Returns a claim_id.";
    type Params = WorkClaimParams;
    type Output = WorkClaimResult;

    async fn run(&self, ctx: &Ctx, p: WorkClaimParams) -> Result<WorkClaimResult, CommandError> {
        let airc = persona_airc(&self.registry, ctx, "work commands")?;
        let card_id = resolve_card_id(&airc, &p.card_id).await?;
        // A card whose instance this box cannot grade is refused at the VERB, the same
        // predicate the pull reads (`card_holder::standing_refusal_for_card`): the pull
        // skipped requests-1766, then a message-turn claim by id took it anyway (Atlas,
        // 2026-09-13, an hour of lane on work no grade could score).
        if let Ok(board) = airc
            .work_board_complete(airc_lib::WORK_BOARD_PROJECTION_PAGE_SIZE)
            .await
        {
            let board = board.snapshot();
            if let Some(card) = board.cards.iter().find(|c| c.card_id == card_id) {
                if let Some((instance, reason)) =
                    crate::persona::card_holder::standing_refusal_for_card(&card.title)
                {
                    continuum_base::probe!(
                        class = "work.claim.refused_ungradeable",
                        card_id = %card_id.as_uuid(),
                        instance = %instance,
                        "claim refused: this box cannot grade the instance"
                    );
                    return Err(CommandError::Invalid(format!(
                        "card {} names {instance}, which this box cannot grade ({}). It is                          withheld from claims until the environment changes (benchmark/validate                          re-checks it); take another card — work/list with claimable=true.",
                        short8(card_id.as_uuid()),
                        reason.trim_end_matches('.')
                    )));
                }
            }
        }
        let ttl_ms = p.ttl_ms.unwrap_or(DEFAULT_CLAIM_TTL_MS);
        let origin = p.origin.into();
        let mut claim_attempt = airc
            .claim_work_card_with_origin(ClaimWorkCard { card_id, ttl_ms }, origin)
            .await;
        // FOLLOW THE CARD TO ITS ROOM (#328 accept-or-redirect, live 2026-08-11):
        // Atlas's very first act on her dispatched SWE card was work/claim by full
        // uuid — refused with "not in current room general; switch to the card's
        // room", because her room pointer was still on #general while the bench
        // card lived on another board. The refusal burned the act, taught her the
        // substrate was broken, and set off a 4×code/list discovery spiral that
        // ended in an empty patch. A card id is unguessable and she can only see
        // boards of rooms she is SUBSCRIBED to, so an explicit claim-by-id is
        // unambiguous intent: find which of her rooms holds the card, switch her
        // there (room = the activity — being in the card's room IS correct), and
        // claim. airc's room-scoping semantics stay untouched; this is her own
        // client doing exactly what the refusal text instructs.
        if matches!(
            claim_attempt,
            Err(airc_lib::AircError::WorkCardNotInCurrentRoom { .. })
        ) {
            if let Some(retry) = claim_following_card_room(&airc, card_id, ttl_ms, origin).await {
                claim_attempt = retry;
            }
        }
        let claim_id = match claim_attempt {
            Ok(id) => id,
            Err(e) => {
                // Name the HOLDER, not just the fact (Joel, 2026-08-03, on
                // Atlas's first live contention bounce: "It should tell you the
                // peer"). A bare "already claimed" is a dead end; "claimed by
                // <peer>" is a coordination move — ask them, watch their
                // progress, or pick another card. Best-effort board read; the
                // original error stands alone if the board is unreadable.
                let mut msg = e.to_string();
                // Whether the refusal is a CONTENTION (someone holds it) or a real
                // fault decides the error CLASS below — a taken card is a normal
                // outcome of a shared board, and telling a citizen "[internal]" for
                // it teaches her the substrate is broken when the truth is "that one
                // is Anwen's". Observed live 2026-08-06.
                let mut contention = false;
                // Set when the live holder turns out to be the caller herself.
                let mut already_yours = None;
                let caller_short = ctx
                    .caller
                    .as_ref()
                    .map(|c| short8(c.peer_id.as_uuid()))
                    .unwrap_or_else(|| "-".to_string());
                if let Ok(board) = airc
                    .work_board_complete(airc_lib::WORK_BOARD_PROJECTION_PAGE_SIZE)
                    .await
                {
                    let board = board.snapshot();
                    if let Some(card) = board.cards.iter().find(|c| c.card_id == card_id) {
                        // The hold must be LIVE. `card.owner` alone is not contention:
                        // expiry never clears owner/claim_id (airc-work
                        // projection/apply.rs clears them only on release or on a NEW
                        // claim), so a card claimed once carries an owner forever. Keying
                        // off the bare field relabelled EVERY refusal — settled work,
                        // wrong-room, transport faults — as "held by peer X", and then
                        // `claim_rejections::record` below burned that fabrication into
                        // perception for ten minutes.
                        //
                        // Measured 2026-08-07: ~50 of 61 cards on #general and 4 of 12 on
                        // #k3-serving (a RETIRED subsystem-named room — do not copy the name;
                        // rooms are activities WITH LIFETIMES) carried a stale owner with a
                        // lease expired 134h+; three
                        // of the latter are in Review, where the true refusal is "settled
                        // work is not claimable". Two citizens spent the day quoting this
                        // string back — "expired leases or the cards being held by others"
                        // is THIS format string, both halves of it.
                        //
                        // `hold_of` is the same predicate `work/list` renders through
                        // (card_holder::holder, below) — one rule for "is someone on this",
                        // not two in one file.
                        let now_ms = EpochMs::now().millis();
                        // ASK WHO, NOT JUST WHETHER. `live_holder` says a person is on
                        // the card; `classify_refusal` says whether that person is YOU.
                        // Without the second question this arm told Anon her own card was
                        // a stranger's (2026-08-14) — see `ClaimRefusal`.
                        let caller_uuid = ctx.caller.as_ref().map(|c| c.peer_id.as_uuid());
                        let holder = live_holder(card, now_ms).map(|o| o.as_uuid());
                        match classify_refusal(holder, caller_uuid) {
                            ClaimRefusal::AlreadyYours => already_yours = card.claim_id,
                            ClaimRefusal::HeldByPeer(owner) => {
                                contention = true;
                                msg = format!(
                                    "card {} (\"{}\") is held by peer {} [{}]. Coordinate with \
                                     them in the room, or take another card — work/list with \
                                     claimable=true lists every card you can pick up right now \
                                     (most sit in the `claimed` column with a lapsed lease, so \
                                     filtering by state=\"open\" will not show them). \
                                     (claim error: {e})",
                                    short8(card_id.as_uuid()),
                                    card.title,
                                    short8(owner),
                                    state_str(&card.state),
                                );
                            }
                            // Nobody is on it — the original error stands as written.
                            ClaimRefusal::Fault => {}
                        }
                    }
                }
                // A claim you ALREADY HOLD is satisfied, not refused: the verb's goal
                // ("this citizen owns this card") is already true, so report success
                // with the live claim rather than inventing a rival. Deliberately does
                // NOT start anything: a claim starts no detached work.
                if let Some(claim_id) = already_yours {
                    if origin == airc_work::ClaimOrigin::Explicit {
                        airc.select_work_claim(card_id, claim_id).await
                            .map_err(|e| CommandError::Denied(e.to_string()))?;
                    }
                    continuum_base::probe!(
                        class = "work.claim",
                        card_id = %card_id.as_uuid(),
                        claimer = %caller_short,
                        "re-claim of a card the caller already holds — satisfied, no re-dispatch"
                    );
                    record_held_claim(&airc, card_id, claim_id).await;
                    return Ok(WorkClaimResult {
                        card_id: p.card_id,
                        claim_id: claim_id.as_uuid().to_string(),
                    });
                }
                // A rejection is WORK-STATE, not a transient tool result: the
                // raw receipt scrolls out of the persona's short window and the
                // intent narrative resurfaces as "I've claimed it" (glass-boxed
                // 2026-08-02, card 44ebaa41). Record it so ActiveWorkSource can
                // keep the fact in perception past the receipt's lifetime.
                if let Some(caller) = ctx.caller.as_ref() {
                    crate::persona::claim_rejections::record(
                        caller.peer_id.as_uuid(),
                        &p.card_id,
                        &msg,
                    );
                }
                // A card someone else holds is a legitimate refusal, not a fault:
                // `Denied` (the caller may not take THIS card), never `Internal`.
                return Err(if contention {
                    CommandError::Denied(msg)
                } else {
                    CommandError::Internal(msg)
                });
            }
        };
        // ON-CLAIM STAGING (the pull inversion, 2026-09-03): the CLAIMER's workspace is
        // prepared per the card's recipe HERE — whoever pulls a card gets its work where
        // her hands are. ONE seam serves a pulled card, a tool-call claim, and a
        // detached-solve round's directed assignee (`benchmark/dispatch` calls the same
        // function before firing her solve). Then the room's DRIVER decides what fires:
        // a citizen-driven round leaves the card to her held-work loop; a detached-solve
        // round starts her scored solve (#346 — claiming IS the start of the work) IN
        // THE CARD'S OWN ROOM (#425: boards are per-room, so the card's activity is the
        // room whose board holds it). Best-effort past the claim: the claim is hers
        // either way, and the probes say what happened. A card we cannot place gets NO
        // detached fallback — inventing invisible work is the failure #425 removed.
        // THE CLAIMER IS THE IDENTITY THE CLAIM WAS MADE AS — `airc.peer_id()`, never
        // `ctx.caller`. The uu operator carries no caller identity: her claim rode the
        // operator self-peer's airc and landed on the board as hers, then this block
        // skipped her (no caller) — no staging, no hands, a card she could not work.
        // 2026-09-12: the first agent to take a benchmark card through the verbs alone
        // found it. A persona's airc peer IS her caller peer, so nothing changes for her.
        let claimer_peer = airc.peer_id();
        record_held_claim(&airc, card_id, claim_id).await;
        {
            match card_in_subscribed_rooms(&airc, card_id).await {
                Some((_room, card)) => {
                    use crate::modules::card_staging::Staging;
                    let staging = match crate::paths::continuum_home() {
                        Ok(home) => {
                            crate::modules::card_staging::stage_for_card(
                                &home,
                                claimer_peer.as_uuid(),
                                &card,
                            )
                            .await
                        }
                        Err(e) => Staging::Failed {
                            stage: "home",
                            error: e.to_string(),
                        },
                    };
                    // A claim stages the claimer's workspace and nothing more: the
                    // work is hers to do in her own loop; no runner starts it for her.
                    continuum_base::probe!(
                        class = "work.claim.held",
                        card_id = %short8(card_id.as_uuid()),
                        claimer = %short8(claimer_peer.as_uuid()),
                        staging = ?staging,
                        "claim staged — the card is hers to work in her own loop; nothing detached fires"
                    );
                }
                None => continuum_base::probe!(
                    class = "work.claim.unplaceable_card",
                    card_id = %short8(card_id.as_uuid()),
                    claimer = %short8(claimer_peer.as_uuid()),
                    "claimed a card no subscribed room's board holds — the claim STANDS, \
                     but nothing is staged and no solve fires: work whose activity we \
                     cannot name is work no room can see (#425)"
                ),
            }
        }
        // A claim is a hold boundary for the governor, whichever path took it.
        crate::persona::work_burst::note_hold_boundary(airc.peer_id().as_uuid());
        Ok(WorkClaimResult {
            card_id: p.card_id,
            claim_id: claim_id.as_uuid().to_string(),
        })
    }
}

/// One-per-persona hands lease: a workspace reclaim takes the resident's hands so no
/// two reclaims (or a reclaim and anything else that takes them) work one tree at once.
/// RAII: dropping the lease (any exit path, panics included) frees the persona.
pub(crate) struct HandsLease(pub(super) uuid::Uuid);

pub(super) fn busy_hands() -> &'static std::sync::Mutex<std::collections::HashSet<uuid::Uuid>> {
    static BUSY: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<uuid::Uuid>>> =
        std::sync::OnceLock::new();
    BUSY.get_or_init(Default::default)
}

impl HandsLease {
    /// Take this persona's hands, or `None` while another holder has them.
    pub(crate) fn try_take(persona: uuid::Uuid) -> Option<Self> {
        let mut g = busy_hands().lock().expect("busy-hands lock never poisoned"); // expect: guards a HashSet op only
        if g.insert(persona) {
            Some(Self(persona))
        } else {
            None
        }
    }
}

impl Drop for HandsLease {
    fn drop(&mut self) {
        if let Ok(mut g) = busy_hands().lock() {
            g.remove(&self.0);
        }
    }
}
