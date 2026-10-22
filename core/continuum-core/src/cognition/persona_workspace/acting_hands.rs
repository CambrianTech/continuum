use super::*;

/// A persona's HANDS, lifted out of the cycle that owns them.
///
/// Exists because rooting a measurement fork's workspace is a PROCESS-GLOBAL side effect that
/// must be undone after the cycle it was taken for is gone. `code/create-workspace` keys the
/// file engine on the CALLER identity (`caller_id(ctx)` → the persona's peer id), and the
/// engines live in one `DashMap` for the whole runtime — while a measurement fork shares the
/// living persona's `Arc<dyn ToolExecutor>` and her id. So re-rooting "the fork's" hands
/// re-roots the LIVING persona's hands, and nothing put them back.
///
/// That is #312, measured 2026-08-04: after a SWE-bench solve on a flask clone, Anwen's LIVE
/// self ran `code/list(path=src)` → `flask/` and `code/read(src/flask/app.py)` in her ordinary
/// room turns, hours later. She had been left standing in the exam room.
///
/// Carrying `(id, name, executor)` separately from the cycle is what makes the restore
/// possible on every exit path: the cycle gets consumed by `with_capture`, moved into the
/// drive, or dropped on an error — the hands handle outlives all of it.
#[derive(Clone)]
pub(crate) struct ActingHands {
    pub(super) persona_id: Uuid,
    pub(super) persona_name: String,
    pub(super) executor: Arc<dyn crate::cognition::tool_executor::ToolExecutor>,
    /// Her working memory: the restore clears the receipt scope the rooting set.
    pub(super) working_memory: Arc<crate::cognition::working_memory::WorkingMemory>,
    /// Which of her hands these are: `None` her live ones, `Some` an eval's or a fork's own
    /// (`ToolExecutor::hands_context`). Rooting and restore act on THESE hands only.
    pub(super) context: Option<crate::identity::hands_context::HandsContext>,
}

impl ActingHands {
    /// The hands of a cycle that has them; `None` for a pure-cognition (handless) cycle.
    pub(crate) fn of(cycle: &WorkspaceCycle) -> Option<Self> {
        cycle.acting().map(|a| Self {
            persona_id: a.persona_id,
            persona_name: a.persona_name.clone(),
            executor: a.executor.clone(),
            working_memory: a.working_memory.clone(),
            context: a.executor.hands_context(),
        })
    }

    /// The same persona's hands in `context`, a context these hands don't own (a card
    /// claim's): `None` when her executor can't act in a context.
    pub(super) fn in_context(&self, context: crate::identity::hands_context::HandsContext) -> Option<Self> {
        Some(Self {
            executor: self.executor.in_hands(self.persona_id, context)?,
            context: Some(context),
            ..self.clone()
        })
    }

}

/// Drive `code/create-workspace` through the persona's OWN identity-bearing executor —
/// the single place either direction (root for a measurement, restore afterward) goes
/// through, so the ACL gate and the failure shape are identical for both
/// ([[the-compression-principle]]).
pub(super) async fn drive_create_workspace(
    hands: &ActingHands,
    root: &str,
    path_prepend: &[String],
    call_id: &str,
    refuse_inert_edits: bool,
) -> Result<(), crate::sdk_codegen::CommandError> {
    let ws_ctx = crate::cognition::tool_executor::ToolExecutionContext {
        persona_id: hands.persona_id,
        persona_name: hands.persona_name.clone(),
        session_id: Uuid::new_v4(),
        context_id: Uuid::new_v4(),
        caller_context: serde_json::Value::Null,
        persona_config: crate::cognition::tool_executor::PersonaMediaConfigLite {
            auto_load_media: false,
            supported_media_types: vec![],
        },
    };
    let ws_call = crate::ai::types::ToolCall {
        id: call_id.to_string(),
        name: "code/create-workspace".to_string(),
        input: serde_json::json!({
            "workspace_root": root,
            "path_prepend": path_prepend,
            "refuse_inert_edits": refuse_inert_edits,
        }),
    };
    let ws_out = hands
        .executor
        .execute_native_batch(std::slice::from_ref(&ws_call), &ws_ctx, 8000)
        .await
        .map_err(|e| {
            crate::sdk_codegen::CommandError::Internal(format!(
                "failed to root workspace at '{root}': {e}"
            ))
        })?;
    if let Some(r) = ws_out.results.first() {
        if r.is_error.is_some() {
            return Err(crate::sdk_codegen::CommandError::Internal(format!(
                "code/create-workspace rejected workspace_root '{root}': {} — refusing to run with \
                 the persona's hands rooted at the wrong directory (would score a false zero).",
                r.content
            )));
        }
    }
    Ok(())
}

/// WHERE HER HANDS STAND: the root a work turn rooted a set of her hands at (a card's
/// checkout), or nothing when they stand in her own workspace. Keyed by persona AND acting
/// context, so an eval's or a fork's place is its own and never overwrites her live one
/// (docs/architecture/HER-HANDS.md). Her live place (`None` context) is the one truth for the
/// workspace map she perceives and the scope her receipts carry; set at the rooting seam,
/// cleared on restore.
pub(super) static ACTING_ROOTS: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<ActingKey, ActingPlace>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

/// Whose place: the persona, and which of her hands (`None` = live).
pub(super) type ActingKey = (uuid::Uuid, Option<crate::identity::hands_context::HandsContext>);

/// Where she stands: the checkout root, and the card whose checkout it is when
/// a held card rooted her (None for a plain workspace_root).
#[derive(Clone, Debug)]
pub(super) struct ActingPlace {
    pub(super) root: std::path::PathBuf,
    pub(super) card: Option<uuid::Uuid>,
}

/// The card-claim context her perception follows: the one her latest turn was bound to, or
/// none (her default hands, at home). Only WHICH hands she looks through changes; no hands'
/// place ever moves (card fc9e9fd7). Set by [`root_at_held_card`], cleared when that
/// claim's hands close.
pub(super) static FOCUS: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<uuid::Uuid, crate::identity::hands_context::HandsContext>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

pub(super) fn focus_of(persona_id: uuid::Uuid) -> Option<crate::identity::hands_context::HandsContext> {
    FOCUS.lock().unwrap_or_else(|e| e.into_inner()).get(&persona_id).copied() // poisoned lock = read the last state, same policy as every lock in this crate
}

/// Point her perception at `context` (or home). The map she perceives re-renders when it moved.
pub(super) fn set_focus(persona_id: uuid::Uuid, context: Option<crate::identity::hands_context::HandsContext>) {
    let moved = {
        let mut map = FOCUS.lock().unwrap_or_else(|e| e.into_inner()); // poisoned lock = read the last state, same policy as every lock in this crate
        let before = match context {
            Some(c) => map.insert(persona_id, c),
            None => map.remove(&persona_id),
        };
        before != context
    };
    if moved {
        crate::persona::grounding_invalidation::mark_workspace_map_dirty(persona_id);
    }
    if context.is_none() {
        crate::cognition::handoff::note_unbound(persona_id);
    }
}

/// Where the hands she is focused on stand (what she perceives and what her receipts are
/// scoped to): her current card claim's checkout, or nothing when she is at home.
pub fn acting_root_of(persona_id: uuid::Uuid) -> Option<std::path::PathBuf> {
    acting_root_in(persona_id, focus_of(persona_id))
}

/// Where one set of her hands stands: her live ones (`None`) or a context's.
pub(crate) fn acting_root_in(persona_id: uuid::Uuid, context: Option<crate::identity::hands_context::HandsContext>) -> Option<std::path::PathBuf> {
    ACTING_ROOTS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&(persona_id, context))
        .map(|p| p.root.clone()) // poisoned lock = read the last state, same policy as every lock in this crate
}

/// The card her hands are rooted at, if a held card rooted them. Her work
/// receipts radiate into THAT card's activity room, not the room whose line
/// happened to trigger the turn (2026-09-04: every holder's 💭/⚙ lines landed
/// in #academy — the base room and the agents' coordination room — because the
/// trigger arrived there; the run room stayed empty for its reviewer).
pub fn acting_card_of(persona_id: uuid::Uuid) -> Option<uuid::Uuid> {
    acting_card_in(persona_id, focus_of(persona_id))
}

/// The card one set of her hands is rooted at: her live ones (`None`) or a context's.
pub(crate) fn acting_card_in(persona_id: uuid::Uuid, context: Option<crate::identity::hands_context::HandsContext>) -> Option<uuid::Uuid> {
    ACTING_ROOTS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&(persona_id, context))
        .and_then(|p| p.card) // poisoned lock = read the last state, same policy as every lock in this crate
}

pub(super) fn note_acting_card_in(persona_id: uuid::Uuid, context: Option<crate::identity::hands_context::HandsContext>, card: uuid::Uuid) {
    if let Some(place) = ACTING_ROOTS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_mut(&(persona_id, context))
    {
        // poisoned lock = read the last state, same policy as every lock in this crate
        place.card = Some(card);
    }
}

pub(super) fn note_acting_root_in(
    persona_id: uuid::Uuid,
    context: Option<crate::identity::hands_context::HandsContext>,
    root: Option<std::path::PathBuf>,
) {
    {
        let mut map = ACTING_ROOTS.lock().unwrap_or_else(|e| e.into_inner()); // poisoned lock = read the last state, same policy as every lock in this crate
        match root {
            Some(r) => {
                map.insert(
                    (persona_id, context),
                    ActingPlace {
                        root: r,
                        card: None,
                    },
                );
            }
            None => {
                map.remove(&(persona_id, context));
            }
        }
    }
    // The map she perceives is the place of the hands she is focused on; an eval or a fork
    // placing its own hands changes nothing she perceives.
    let marked = context == focus_of(persona_id) && crate::persona::grounding_invalidation::mark_workspace_map_dirty(persona_id);
    continuum_base::probe!(
        class = "workspace.acting_root",
        persona_id = %persona_id,
        context = %context.map(|c| c.id().to_string()).unwrap_or_else(|| "live".to_string()), // unwrap_or_else: no context = her live hands, named
        root = %acting_root_in(persona_id, context).map(|p| p.display().to_string()).unwrap_or_default(),  // unwrap_or: nothing recorded / a pre-epoch clock = 0, never a guess
        map_marked_dirty = marked,
        "where this set of her hands stands now; her live map re-renders only when her live hands moved"
    );
}

/// What the board says became of a card she believed she held, from the ONE holder
/// projection the board itself renders (`card_holder`). PURE, so every branch is pinned:
/// a live lease held by `her` → `None` (the card IS hers — a renewal or re-claim landed
/// between the wake's two reads, nothing is gone); a live lease that is not hers →
/// `HeldBy`; a card past the claimable states → `MovedOn` (its life advanced, not a
/// lapse); otherwise `Lapsed` — the last lease, whoever's, ran out and nobody holds it.
/// `owner` is never read as authority: an expired claim still names its last holder, and
/// that holder may be her.
pub(crate) fn claim_gone_verdict(
    card: &airc_lib::WorkCard,
    now_ms: u64,
    her: uuid::Uuid,
) -> Option<crate::cognition::working_memory::ClaimGone> {
    use crate::cognition::working_memory::ClaimGone;
    use crate::persona::card_holder::{hold_of, refused_by_claim, Hold};
    if refused_by_claim(card.state) {
        return Some(ClaimGone::MovedOn {
            state: crate::modules::work::state_str(&card.state),
        });
    }
    let expired_at_ms = card.claim_expires_at_ms;
    Some(match hold_of(card, now_ms) {
        Hold::Held => match card.owner {
            Some(peer) if peer.as_uuid() == her => return None,
            Some(peer) => ClaimGone::HeldBy {
                peer: peer.as_uuid(),
                expires_at_ms: expired_at_ms,
            },
            // A live hold with no owner cannot be rendered by the board either; say
            // lapsed-and-open rather than invent a holder.
            None => ClaimGone::Lapsed { expired_at_ms },
        },
        Hold::Lapsed | Hold::Unclaimed => ClaimGone::Lapsed { expired_at_ms },
    })
}

/// Bind THIS turn to the hands of her held card's claim, standing in the card's staged
/// checkout — any turn, not only the work turn. Freya (2026-09-05) edited a file during a
/// room turn with her hands at home: her home is a copy of the continuum repo, and the edit
/// landed there, not in her card's checkout. While a citizen holds a card she works at that
/// repo. Returns the admitted claim hands for the turn's burst ([`crate::cognition::workspace::Burst::in_hands`]) and for
/// [`end_card_turn`]; nothing is restored after the turn, because nothing moved (card
/// fc9e9fd7).
pub(crate) async fn root_at_held_card(
    cycle: &WorkspaceCycle,
    peer_id: uuid::Uuid,
    conversation: &dyn crate::persona::service_loop::PersonaConversation,
) -> HeldCardTurn {
    let turn = bind_held_card(cycle, peer_id, conversation).await;
    // A turn bound to no claim's hands acts with her default hands, at home: her
    // perception follows the same hands the turn acts with.
    if turn.hands.is_none() {
        set_focus(peer_id, None);
    }
    turn
}

pub(super) async fn bind_held_card(
    cycle: &WorkspaceCycle,
    peer_id: uuid::Uuid,
    conversation: &dyn crate::persona::service_loop::PersonaConversation,
) -> HeldCardTurn {
    // The card is UNKNOWN on each of these: there is no citizen, her claims could not be
    // read, or she holds nothing. Absent credit is the honest answer here — unlike the
    // workspace failures further down, where the card IS known.
    let Some(citizen) = conversation.stream_citizen() else {
        return HeldCardTurn::unheld();
    };
    let Ok(held) = citizen.active_claims().await else {
        return HeldCardTurn::unheld();
    };
    // THE WAKE'S FIRST READ OF THE BOARD answers what the checkpoint believed (card
    // c8303c32, Kimi 2026-09-21): her hands were rooted at a card when the snapshot was
    // written; if the board no longer lists it among her holds, she is told NOW — in her
    // window, before she resumes as its holder — instead of by a submit refusal hours on.
    // The verdict is the board's, through the SAME holder projection the board renders
    // (`card_holder`), never an owner field read as authority (Astra on #4315: an expired
    // claim still names its last holder). The belief is consumed only once the board has
    // ANSWERED; a card no board she stands in shows is UNKNOWN — said as such, and asked
    // again next turn — never "nobody holds it".
    if let Some(body) = cycle.acting() {
        if let Some(believed) = body.working_memory.peek_restored_acting_card() {
            if held.iter().any(|c| c.card_id.as_uuid() == believed) {
                // The board agrees with the checkpoint: nothing to say, nothing to keep.
                body.working_memory.take_restored_acting_card();
            } else {
                // The card's own row is a SECOND read; a renewal or re-claim can land
                // between it and `active_claims`, so the verdict is reconciled with HER
                // identity — a live hold that is hers is not gone (Astra, #4315 round 2).
                // Three outcomes: the row shows her holding it (settled, silent); the row
                // shows it gone (settled, said); no board she stands in shows the row
                // (unsettled — said as unknown, the belief kept for the next read).
                let now_ms = EpochMs::now().millis();
                let row = citizen.card(believed).await;
                let verdict = row.as_ref().map(|card| claim_gone_verdict(card, now_ms, peer_id));
                match verdict {
                    Some(None) => {
                        body.working_memory.take_restored_acting_card();
                    }
                    Some(Some(v)) => {
                        body.working_memory.take_restored_acting_card();
                        body.working_memory.note_claim_gone(believed, v);
                    }
                    None => body.working_memory.note_claim_gone(
                        believed,
                        crate::cognition::working_memory::ClaimGone::Unknown,
                    ),
                }
                if !matches!(verdict, Some(None)) {
                    continuum_base::probe!(
                        class = "persona.claim.gone_on_wake",
                        peer = %peer_id,
                        card = %believed,
                        verdict = ?verdict.flatten(),
                        "her checkpoint rooted her at a card the board no longer counts as hers — told in her window on the first held-card read"
                    );
                }
            }
        }
    }
    // The hands of every claim she no longer holds close, whether or not a card is
    // bound this turn.
    if let Some(live) = ActingHands::of(cycle) {
        let held_contexts: Vec<crate::identity::hands_context::HandsContext> = held
            .iter()
            .filter_map(|c| c.claim_id.map(|claim| crate::identity::hands_context::HandsContext::for_claim(peer_id, c.card_id.as_uuid(), claim.as_uuid())))
            .collect();
        close_ended_claims(&live, &held_contexts, citizen.as_ref(), peer_id).await;
    }
    if held.is_empty() {
        return HeldCardTurn::unheld();
    }
    // ONE card per turn here too (`work_focus`): rooting on every held title
    // was ambiguous for a two-card holder, so her message turns kept her hands
    // at home while her work turns rooted (`persona.work.staged_ambiguous` ×2
    // after the focus cut, 2026-09-04).
    let Some(focus) = crate::persona::work_focus::focus_workspace_card(held.iter()) else {
        return HeldCardTurn::unheld();
    };
    bind_card(cycle, peer_id, focus).await
}

/// The activity this step perceives and acts IN: the board room of the card her hands are rooted on, by
/// the one definition of "which board holds a card" (`modules::work::room_holding_card`,
/// any activity, project or benchmark); else the room the turn was woken in. The wake room
/// is a fallback for acts with no subject yet (a help call before she has chosen a task),
/// never an override of one. It's the card of THESE hands (`context`): an eval's or a fork's
/// acts never inherit her live card's room (Astra's review of #5005).
pub(crate) async fn activity_room_for(
    persona_id: Uuid,
    context: Option<crate::identity::hands_context::HandsContext>,
    wake_room: Uuid,
) -> Uuid {
    if crate::persona::mind_room::is_private_room(persona_id, wake_room) {
        return wake_room;
    }
    let Some(card) = acting_card_in(persona_id, context) else {
        return wake_room;
    };
    let Some(runtime) = crate::persona::airc_runtime_registry::PersonaAircRuntimeRegistry::try_global()
        .and_then(|registry| registry.get(persona_id))
    else {
        return wake_room;
    };
    match crate::modules::work::room_holding_card(runtime.airc(), airc_lib::WorkCardId::from_uuid(card)).await {
        Some(room) => room.channel.as_uuid(),
        None => wake_room,
    }
}

/// Bind a turn to the hands of `card`'s claim (card fc9e9fd7): THE one binding both the
/// room turn ([`root_at_held_card`]) and the held-work turn use. Resolves the card's
/// checkout through the one owner of where a card's work lives
/// (`card_staging::checkout_path_for`), admits the turn to the claim's hands, roots them
/// once, points her perception at them and tells her where she stands when that changed.
pub(crate) async fn bind_card(
    cycle: &WorkspaceCycle,
    peer_id: uuid::Uuid,
    focus: &airc_lib::WorkCard,
) -> HeldCardTurn {
    // ── FROM HERE THE CARD IS KNOWN ───────────────────────────────────────────────
    // Everything that can still fail below is a fact about the WORKSPACE — no checkout
    // on this node, no acting body, a rooting error. NONE of them is a fact about which
    // card she is working, so none of them may erase the credit.
    //
    // Captured AT SELECTION and carried out BESIDE the hands rather than on them
    // (Astra/S6 on 799b8fe9). The first cut hung credit off `ActingHands`, so every
    // workspace failure below silently produced `credit: None` — and the training
    // producer reads `None` as "ordinary unlinked conversation" and SUBMITS IT
    // IMMEDIATELY as positive SFT, skipping the evidence floor. That is a pre-existing
    // rooting absence being reused as a different fact, which is the substrate defect
    // shape: one value, two meanings. A turn whose card is known stays STAGED even when
    // her hands never reached it.
    //
    // `from_selected_card` is PURE — it reads the card and invents nothing, so a
    // claimless card yields a credit with no receipt rather than a fabricated one.
    let credit = crate::persona::training_producer::CapturedCredit::from_selected_card(focus);
    // Resolve through the SAME authority that staged the card at claim time
    // (`card_staging::checkout_path_for`), which keys a generic repo card on its
    // CARD ID via airc's per-card worktree.
    //
    // This used to ask `staged_workspace::workspace_for_held_cards` for a match on
    // the card's DISPLAY TITLE — and every path in that module is rooted at
    // `staging_root(peer)`, the SWE-BENCH staging area. Card work does not live
    // there, so a generic repo card never matched, the resolver returned `None`,
    // and the `?` here dropped the whole function: her hands silently stayed at the
    // resident root while she believed she was working the card. Kimi's build.rs
    // edit (eb5a2606, 2026-09-08) landed in the resident workspace that way. The
    // title was the symptom; asking a benchmark-shaped resolver about an ordinary
    // repo card was the defect.
    crate::modules::card_staging::restage_held_linked_checkout(peer_id, focus).await;
    let Some(ws) = crate::modules::card_staging::checkout_path_for(&peer_id, focus) else {
        // NOT a silent `?`. The ambiguous case has always been probed; the
        // not-found case was the one that said nothing, which is why this went
        // unnoticed until a citizen's commit turned up in the wrong tree.
        continuum_base::probe!(
            class = "persona.work.checkout_unresolved",
            peer = %peer_id,
            card = %focus.card_id.as_uuid(),
            repo = %focus.repo,
            "held card has no checkout on this node — her hands stay at the resident \
             root for this turn, and anything she writes lands there, NOT in the card"
        );
        // Her hands stay home, but she is still working THIS card — the credit rides out.
        return HeldCardTurn::unrooted(credit, peer_id, focus.card_id.as_uuid());
    };
    // A pure-cognition persona has no hands at all. Still her card.
    let Some(live) = ActingHands::of(cycle) else {
        return HeldCardTurn::unrooted(credit, peer_id, focus.card_id.as_uuid());
    };
    // THE CLAIM'S OWN HANDS (card fc9e9fd7): derived from the claim, the same for every turn
    // of it and new for any other claim, rooted once at the card's checkout and never moved.
    // Her default hands stay home; no turn moves another's hands, so concurrent turns can't
    // split her shell from her code tools (Kimi #11697, card 2d58192d).
    let Some(claim) = focus.claim_id else {
        continuum_base::probe!(
            class = "persona.work.claim_unknown",
            peer = %peer_id,
            card = %focus.card_id.as_uuid(),
            "the focus card carries no claim id, so no claim's hands can be bound; nothing acts on it this turn"
        );
        return HeldCardTurn::unrooted(credit, peer_id, focus.card_id.as_uuid());
    };
    let context = crate::identity::hands_context::HandsContext::for_claim(peer_id, focus.card_id.as_uuid(), claim.as_uuid());
    let Some(hands) = live.in_context(context) else {
        return HeldCardTurn::unrooted(credit, peer_id, focus.card_id.as_uuid());
    };
    let lease = match crate::identity::hands_gate::admit(context) {
        Ok(lease) => lease,
        Err(why) => {
            continuum_base::probe!(class = "persona.work.hands_closing", peer = %peer_id, card = %focus.card_id.as_uuid(), why = %why, "the claim's hands are closing; nothing acts on the card this turn");
            return HeldCardTurn::unrooted(credit, peer_id, focus.card_id.as_uuid());
        }
    };
    // Rooted ONCE: a context's place is fixed for its life, so a later turn finds it there.
    if acting_root_in(peer_id, Some(context)).as_deref() != Some(ws.as_path()) {
        if let Err(e) = drive_create_workspace(&hands, &ws.to_string_lossy(), &[], "bind-card-hands", false).await {
            tracing::warn!(error = %e, "could not root the card claim's hands at its checkout");
            // Nothing may act on the card, but the turn is still card-linked: it stages.
            return HeldCardTurn::unrooted(credit, peer_id, focus.card_id.as_uuid());
        }
        note_acting_root_in(peer_id, Some(context), Some(ws.clone()));
        note_acting_card_in(peer_id, Some(context), focus.card_id.as_uuid());
    }
    set_focus(peer_id, Some(context));
    // Both room and work turns stamp only after binding succeeds (81097858).
    crate::cognition::handoff::note_held(peer_id, None, focus, EpochMs::now().millis());
    if let Some(body) = cycle.acting() {
        body.working_memory.set_scope(Some(ws.to_string_lossy().to_string()));
        // SAY WHERE SHE NOW STANDS when it is not where she was last told (card 822c4253):
        // staging a card's checkout moved Kimi from the career-wrangler/ tree she had built
        // at home into a per-card worktree on a card branch, and her next code/read of
        // career-wrangler/docs/... failed with no word of why. Recorded as told only AFTER
        // the line is pinned (Cormac on #4555).
        if !already_told(peer_id, &ws) {
            let branch = crate::modules::card_staging::card_branch(focus);
            body.working_memory.pin_fact_for_turns("rooted", &rooted_notice(&focus.repo.to_string(), &ws, &branch), 3);
            mark_told(peer_id, &ws);
            continuum_base::probe!(
                class = "persona.work.root_moved",
                peer = %peer_id,
                card = %focus.card_id.as_uuid(),
                root = %ws.display(),
                branch = %branch,
                "she works a checkout she was not last told about; she is told now"
            );
        }
    }
    HeldCardTurn {
        credit: Some(credit),
        hands: Some(CardHands { context, hands, lease: std::sync::Arc::new(lease) }),
        unbound: None,
    }
}

/// The checkout each citizen was last TOLD her hands stand in (card 822c4253). Process
/// memory: after a restart she is told once more, which costs one pinned line.
pub(super) static ANNOUNCED_ROOTS: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<uuid::Uuid, std::path::PathBuf>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

/// Whether `root` is the checkout she was last told about. A per-turn rooting at the
/// same checkout says nothing.
pub(super) fn already_told(persona_id: uuid::Uuid, root: &std::path::Path) -> bool {
    ANNOUNCED_ROOTS
        .lock()
        .unwrap_or_else(|e| e.into_inner()) // poisoned lock = read the last state, same policy as every lock in this crate
        .get(&persona_id)
        .is_some_and(|r| r == root)
}

/// Record `root` as told, once the line naming it is pinned in her window.
pub(super) fn mark_told(persona_id: uuid::Uuid, root: &std::path::Path) {
    ANNOUNCED_ROOTS
        .lock()
        .unwrap_or_else(|e| e.into_inner()) // poisoned lock = read the last state, same policy as every lock in this crate
        .insert(persona_id, root.to_path_buf());
}

/// PURE: the line that tells her where her hands now stand and what did not move. The
/// branch is the card's EXPECTED branch (derived from the card), never asserted as the
/// checked-out HEAD, which an existing tree may have moved (Codex on #4555).
pub(super) fn rooted_notice(repo: &str, root: &std::path::Path, branch: &str) -> String {
    format!(
        "[workspace] For your held card on {repo}, your hands now stand in its own checkout: \
         {root}. The card's branch is {branch}; git status shows what is actually checked \
         out. Paths in code/* are relative to it (docs/x.md, not <repo>/docs/x.md). Your \
         home workspace is unchanged: a copy of this repo you keep there is a SEPARATE tree, \
         and anything not pushed from it lives only there. Commits here stay on the checked \
         out branch until you push them where the project wants them.",
        root = root.display()
    )
}

/// What a turn knows about the card being worked, kept SEPARATE from whether her hands
/// reached it.
///
/// These are two different facts and collapsing them is what card 0d51573a's review
/// caught: a missing checkout, an absent acting body, or a rooting error all left the
/// credit `None`, and [`crate::persona::training_producer::produce`] reads `None` as
/// ordinary unlinked conversation — submitting the turn immediately as positive SFT
/// instead of staging it behind the evidence floor. A workspace failure must never be
/// readable as "she was not working a card".
pub(crate) struct HeldCardTurn {
    /// The card the work store selected for this turn, present whenever one was
    /// selected — regardless of what happened to the workspace afterwards.
    pub(crate) credit: Option<crate::persona::training_producer::CapturedCredit>,
    /// The hands of the card claim this turn was admitted to, standing in its checkout.
    /// `None` means nothing may act on the card this turn; it says nothing about which
    /// card it is.
    pub(crate) hands: Option<CardHands>,
    /// When the card is known but its hands could not be bound: the never-rooted context the
    /// turn acts with instead, so its code tools refuse rather than act at home.
    pub(super) unbound: Option<crate::identity::hands_context::HandsContext>,
}

/// A turn's admission to the hands of the card claim it works (card fc9e9fd7). The context
/// is derived from the claim, rooted ONCE at the card's checkout and never moved; the lease
/// holds the hands open until the turn ends, so closing them (the claim ended) waits the
/// turn out instead of dropping its engine or shell under it.
pub(crate) struct CardHands {
    pub(crate) context: crate::identity::hands_context::HandsContext,
    pub(super) hands: ActingHands,
    pub(super) lease: std::sync::Arc<crate::identity::hands_gate::HandsLease>,
}

impl CardHands {
    /// Where these hands stand: the claim's checkout, rooted once.
    pub(crate) fn root(&self) -> Option<std::path::PathBuf> {
        acting_root_in(self.hands.persona_id, Some(self.context))
    }
}

/// The end of one turn on a card: the act's work is WIP-committed on the card's branch and
/// pushed (card 73eefbbb), so a node she is next staged on fetches it and a seat change waits
/// on it. Her hands stay where they are; nothing is restored, because nothing moved.
pub(crate) async fn end_card_turn(card: &CardHands, acts: usize) {
    let hands = &card.hands;
    // A turn that took no act changed nothing in the checkout: nothing to commit, nothing to
    // carry. Measured on the M5 (2026-10-16, lane gated at 6,144): every resident's turn
    // ended without reaching the model and the carry still ran git on each one, 374 ms
    // a time, 33 times per resident per 5 min, committed=false pushed=false throughout.
    if acts == 0 {
        continuum_base::probe!(
            class = "workspace.push.skipped_no_act",
            persona = %hands.persona_name,
            "turn took no act: the card's checkout is as it was, nothing to carry"
        );
        return;
    }
    if let (Some(root), Some(card_id)) = (acting_root_in(hands.persona_id, hands.context), acting_card_in(hands.persona_id, hands.context)) {
        // under the turn's own admission (see sync_after_act_for)
        crate::persona::workspace_transfer::sync_after_act_for(std::sync::Arc::clone(&card.lease), hands.persona_id, &hands.persona_name, root, card_id).await;
    }
    // Her other held cards' committed work rides the carry owner, not this card's turn.
    crate::modules::card_carry::request();
}

/// PURE: has the claim behind `context` ENDED, by the card's own row? Closed hands never
/// reopen (`code::hands`), so only an end may close them: the card is TERMINAL (Merged,
/// Closed), the claim was released (the board clears `claim_id`), or the card's claim is now
/// a different one (re-claimed, or another holder's). Not an end: a lease that LAPSED (same
/// claim id, renewed by her next heartbeat); Review or Blocked, which keep the claim (airc
/// `apply_card_state_changed` changes state and renews the lease, `claim_id` untouched), so a
/// send-back returns the card to InProgress under the SAME claim (Codex on #5247: reusing
/// `refused_by_claim`, which counts Review, would lock a resident out after her first
/// submission); and a row the board couldn't give. Closing on mere absence from her active claims locked Kimi out of
/// 0c69aa07 for hours (2026-10-21: `persona.work.hands_closing` on the 5090 every turn while
/// the board showed her claim live).
pub(crate) fn claim_has_ended(
    context: crate::identity::hands_context::HandsContext,
    card: Option<&airc_lib::WorkCard>,
    her: uuid::Uuid,
) -> bool {
    let Some(card) = card else { return false };
    if matches!(card.state, airc_work::model::CardState::Merged | airc_work::model::CardState::Closed) {
        return true;
    }
    match card.claim_id {
        None => true,
        Some(current) => crate::identity::hands_context::HandsContext::for_claim(her, card.card_id.as_uuid(), current.as_uuid()) != context,
    }
}

/// Close the hands of every card claim whose claim ENDED ([`claim_has_ended`], read from
/// each card's own row): released, terminal, or re-acquired as a new claim. A claim missing
/// from her active claims is only a candidate; a lapsed lease or an unreadable row keeps its
/// hands. Each close runs in the background, refusing new turns and waiting out admitted
/// ones, then releases its engine and shells.
pub(super) async fn close_ended_claims(
    live: &ActingHands,
    held: &[crate::identity::hands_context::HandsContext],
    citizen: &dyn crate::persona::airc_citizen::AircCitizen,
    her: uuid::Uuid,
) {
    let candidates: Vec<(crate::identity::hands_context::HandsContext, uuid::Uuid)> = ACTING_ROOTS
        .lock()
        .unwrap_or_else(|e| e.into_inner()) // poisoned lock = read the last state, same policy as every lock in this crate
        .iter()
        .filter(|((persona, context), _)| *persona == live.persona_id && context.is_some_and(|c| !held.contains(&c)))
        .filter_map(|((_, context), place)| Some(((*context)?, place.card?)))
        .collect();
    let mut ended = Vec::new();
    for (context, card) in candidates {
        let row = citizen.card(card).await;
        if claim_has_ended(context, row.as_ref(), her) {
            ended.push(context);
        } else {
            continuum_base::probe!(
                class = "persona.work.hands_kept",
                peer = %her,
                card = %card,
                row_read = row.is_some(),
                "a claim missing from her active claims has not ENDED (lapsed lease or unread row); its hands stay open"
            );
        }
    }
    for context in ended {
        let Some(hands) = live.in_context(context) else { continue };
        tokio::spawn(async move {
            // The release closes the hands to new work and waits out admitted work itself.
            let root = acting_root_in(hands.persona_id, Some(context));
            match close_acting_context(&hands).await {
                Ok(()) => {
                    if let Some(root) = root {
                        crate::persona::workspace_transfer::forget_checkout(&root);
                    }
                    if focus_of(hands.persona_id) == Some(context) {
                        set_focus(hands.persona_id, None);
                    }
                }
                Err(e) => tracing::warn!(error = %e, context = %context.id(), "a card claim's hands could not be released; they are kept closed for a retry"),
            }
        });
    }
}

impl HeldCardTurn {
    /// The hands this turn acts with: its card claim's, the card's never-rooted stand-in when
    /// those could not be bound, or her default hands when she works no card.
    pub(crate) fn turn_hands(&self) -> Option<crate::identity::hands_context::HandsContext> {
        self.hands.as_ref().map(|h| h.context).or(self.unbound)
    }

    /// No card was selected: no citizen, unreadable claims, or nothing held.
    pub(super) fn unheld() -> Self {
        Self {
            credit: None,
            hands: None,
            unbound: None,
        }
    }

    /// The card is known and her hands did not reach it.
    pub(super) fn unrooted(credit: crate::persona::training_producer::CapturedCredit, persona: Uuid, card: Uuid) -> Self {
        Self {
            credit: Some(credit),
            hands: None,
            unbound: Some(crate::identity::hands_context::HandsContext::unbound(persona, card)),
        }
    }
}
