//! Binding a turn to the hands of the card claim she works (card fc9e9fd7): the board half of
//! her hands, moved up out of cognition (mind split, Peel D). It reads her claims through her
//! airc citizen, stages the card's checkout, roots the claim's hands once through cognition's
//! hands (`cognition::persona_workspace`), carries the turn's credit beside them, and at the
//! turn's end carries the card's work. Cognition keeps where her hands stand; persona knows
//! which card they are for.

use continuum_base::time::EpochMs;
use uuid::Uuid;

use crate::cognition::persona_workspace::{
    acting_card_in, acting_root_in, claim_gone_verdict, close_acting_context, claim_has_ended, claimed_places_of, drive_create_workspace, focus_of,
    note_acting_card_in, note_acting_root_in, set_focus, ActingHands,
};
use crate::cognition::workspace::WorkspaceCycle;
use crate::identity::hands_context::HandsContext;
use crate::persona::service_loop::CitizenConversation;

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
    conversation: &dyn CitizenConversation,
) -> HeldCardTurn {
    let turn = bind_held_card(cycle, peer_id, conversation).await;
    // A turn bound to no claim's hands acts with her default hands, at home: her
    // perception follows the same hands the turn acts with.
    if turn.hands.is_none() {
        set_focus(peer_id, None);
    }
    turn
}

pub(crate) async fn bind_held_card(
    cycle: &WorkspaceCycle,
    peer_id: uuid::Uuid,
    conversation: &dyn CitizenConversation,
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
        let held_contexts: Vec<HandsContext> = held
            .iter()
            .filter_map(|c| c.claim_id.map(|claim| HandsContext::for_claim(peer_id, c.card_id.as_uuid(), claim.as_uuid())))
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

/// Bind a turn to the hands of `card`'s claim (card fc9e9fd7): THE one binding both the
/// room turn ([`root_at_held_card`]) and the held-work turn use. Resolves the card's
/// checkout through the one owner of where a card's work lives
/// (`card_staging::checkout_path_for`), admits the turn to the claim's hands, roots them
/// once, points her perception at them and tells her where she stands when that changed.
pub async fn bind_card(
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
    crate::persona::card_checkout::restage_held_linked_checkout(peer_id, focus).await;
    let Some(ws) = crate::persona::card_checkout::checkout_path_for(&peer_id, focus) else {
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
    let context = HandsContext::for_claim(peer_id, focus.card_id.as_uuid(), claim.as_uuid());
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
            let branch = crate::code::card_branch::card_branch(focus);
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

/// Close the hands of every card claim whose claim ENDED ([`claim_has_ended`], read from
/// each card's own row): released, terminal, or re-acquired as a new claim. A claim missing
/// from her active claims is only a candidate; a lapsed lease or an unreadable row keeps its
/// hands. Each close runs in the background, refusing new turns and waiting out admitted
/// ones, then releases its engine and shells.
pub(crate) async fn close_ended_claims(
    live: &ActingHands,
    held: &[HandsContext],
    citizen: &dyn crate::persona::airc_citizen::AircCitizen,
    her: uuid::Uuid,
) {
    let candidates: Vec<(HandsContext, uuid::Uuid)> = claimed_places_of(live.persona_id)
        .into_iter()
        .filter(|(context, _)| !held.contains(context))
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

/// The checkout each citizen was last TOLD her hands stand in (card 822c4253). Process
/// memory: after a restart she is told once more, which costs one pinned line.
pub(crate) static ANNOUNCED_ROOTS: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<uuid::Uuid, std::path::PathBuf>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

/// Whether `root` is the checkout she was last told about. A per-turn rooting at the
/// same checkout says nothing.
pub(crate) fn already_told(persona_id: uuid::Uuid, root: &std::path::Path) -> bool {
    ANNOUNCED_ROOTS
        .lock()
        .unwrap_or_else(|e| e.into_inner()) // poisoned lock = read the last state, same policy as every lock in this crate
        .get(&persona_id)
        .is_some_and(|r| r == root)
}

/// Record `root` as told, once the line naming it is pinned in her window.
pub(crate) fn mark_told(persona_id: uuid::Uuid, root: &std::path::Path) {
    ANNOUNCED_ROOTS
        .lock()
        .unwrap_or_else(|e| e.into_inner()) // poisoned lock = read the last state, same policy as every lock in this crate
        .insert(persona_id, root.to_path_buf());
}

/// PURE: the line that tells her where her hands now stand and what did not move. The
/// branch is the card's EXPECTED branch (derived from the card), never asserted as the
/// checked-out HEAD, which an existing tree may have moved (Codex on #4555).
pub(crate) fn rooted_notice(repo: &str, root: &std::path::Path, branch: &str) -> String {
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
pub struct HeldCardTurn {
    /// The card the work store selected for this turn, present whenever one was
    /// selected — regardless of what happened to the workspace afterwards.
    pub(crate) credit: Option<crate::persona::training_producer::CapturedCredit>,
    /// The hands of the card claim this turn was admitted to, standing in its checkout.
    /// `None` means nothing may act on the card this turn; it says nothing about which
    /// card it is.
    pub hands: Option<CardHands>,
    /// When the card is known but its hands could not be bound: the never-rooted context the
    /// turn acts with instead, so its code tools refuse rather than act at home.
    pub(crate) unbound: Option<HandsContext>,
}

/// A turn's admission to the hands of the card claim it works (card fc9e9fd7). The context
/// is derived from the claim, rooted ONCE at the card's checkout and never moved; the lease
/// holds the hands open until the turn ends, so closing them (the claim ended) waits the
/// turn out instead of dropping its engine or shell under it.
pub struct CardHands {
    pub context: HandsContext,
    pub hands: ActingHands,
    pub(crate) lease: std::sync::Arc<crate::identity::hands_gate::HandsLease>,
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
    crate::persona::card_carry::request();
}

/// A held card's end-of-turn sync (commit and push, up to `SYNC_BOUND`), handed back to the
/// turn's owner instead of awaited inside the drive, so her reply is published first and the
/// sync still runs before anything else touches the checkout (LOCKS-AND-FIFOS-AUDIT.md row 11).
#[must_use = "run the card sync before her next act touches the checkout"]
pub(crate) struct CardSync {
    hands: Option<CardHands>,
    acts: usize,
}

impl CardSync {
    /// No sync owed: the turn held no card, or its sync already ran.
    pub(crate) fn done() -> Self {
        Self { hands: None, acts: 0 }
    }

    /// Commit and push what the turn did in the card's checkout.
    pub(crate) async fn run(self) {
        if let Some(hands) = &self.hands {
            end_card_turn(hands, self.acts).await;
        }
    }
}

/// One turn in the hands of the card she holds, start to end (card 959f3d5c): rooted at
/// the held card, its credit captured on her act, driven in the card claim's hands, and its
/// card sync returned for the caller to run after it publishes what she said. A room turn and her own-time turn are the same turn
/// here; with no card held it drives in her default hands with no capture. The credit is
/// returned from the CARD, not her hands (see [`HeldCardTurn::credit`]).
pub(crate) async fn drive_in_held_card(
    cycle: &WorkspaceCycle,
    persona_id: Uuid,
    persona_name: &str,
    stimulus: &str,
    burst: crate::cognition::workspace::Burst,
    framing: crate::cognition::workspace::TurnFraming,
    max_acts: usize,
    conversation: &mut dyn CitizenConversation,
) -> (
    crate::cognition::act_observe::SettleOutcome,
    Option<crate::persona::training_producer::CapturedCredit>,
    CardSync,
) {
    let held = root_at_held_card(cycle, persona_id, &*conversation).await;
    // Her credit follows the card she holds through the turn (card 12ddd78f): a turn that
    // begins holding nothing and claims a card stages from the claim on.
    let mut capture = crate::persona::training_producer::ClaimFollowingCapture::new(
        held.credit.as_ref(),
        conversation.stream_citizen(),
        crate::persona::training_producer::TurnCreditCapture::opener(persona_id, persona_name, stimulus),
    );
    let outcome = crate::cognition::act_observe::drive_to_settle_with_credit(
        cycle,
        burst.in_hands(held.turn_hands()),
        max_acts,
        framing,
        conversation,
        Some(&mut capture as &mut dyn crate::cognition::act_observe::TurnCredit),
    )
    .await;
    let credit = capture.into_credit();
    let sync = CardSync { hands: held.hands, acts: outcome.acts };
    (outcome, credit, sync)
}

impl HeldCardTurn {
    /// The hands this turn acts with: its card claim's, the card's never-rooted stand-in when
    /// those could not be bound, or her default hands when she works no card.
    pub(crate) fn turn_hands(&self) -> Option<HandsContext> {
        self.hands.as_ref().map(|h| h.context).or(self.unbound)
    }

    /// No card was selected: no citizen, unreadable claims, or nothing held.
    pub(crate) fn unheld() -> Self {
        Self {
            credit: None,
            hands: None,
            unbound: None,
        }
    }

    /// The card is known and her hands did not reach it.
    pub(crate) fn unrooted(credit: crate::persona::training_producer::CapturedCredit, persona: Uuid, card: Uuid) -> Self {
        Self {
            credit: Some(credit),
            hands: None,
            unbound: Some(HandsContext::unbound(persona, card)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (card 822c4253): her hands moving to a card's own checkout with no
    // word (Kimi's code/read of career-wrangler/docs/... failing in a per-card worktree),
    // or the notice repeating every turn at the same checkout. Told on a new root, silent
    // on the same one, told again when it moves; the notice names root, branch and the
    // home tree that did not move.
    #[test]
    fn she_is_told_when_her_hands_move_and_only_then() {
        let kimi = Uuid::new_v4();
        let card = std::path::Path::new("/w/worktrees/af4e2cea");
        assert!(!already_told(kimi, card), "first rooting is news");
        assert!(!already_told(kimi, card), "checking does not record: only a pinned line does");
        mark_told(kimi, card);
        assert!(already_told(kimi, card), "the same checkout next turn is not news");
        assert!(!already_told(kimi, std::path::Path::new("/w/worktrees/b0b0b0b0")));
        assert!(!already_told(Uuid::new_v4(), card), "per citizen, never shared");
        let line = rooted_notice("CambrianTech/career-wrangler", card, "af4e2cea/a-real-product");
        for want in ["/w/worktrees/af4e2cea", "af4e2cea/a-real-product", "docs/x.md", "home workspace is unchanged"] {
            assert!(line.contains(want), "{want}: {line}");
        }
    }
}
