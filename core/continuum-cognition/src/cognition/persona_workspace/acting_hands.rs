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
pub struct ActingHands {
    pub persona_id: Uuid,
    pub persona_name: String,
    pub executor: Arc<dyn crate::cognition::tool_executor::ToolExecutor>,
    /// Her working memory: the restore clears the receipt scope the rooting set.
    pub working_memory: Arc<crate::cognition::working_memory::WorkingMemory>,
    /// Which of her hands these are: `None` her live ones, `Some` an eval's or a fork's own
    /// (`ToolExecutor::hands_context`). Rooting and restore act on THESE hands only.
    pub context: Option<crate::identity::hands_context::HandsContext>,
}

impl ActingHands {
    /// The hands of a cycle that has them; `None` for a pure-cognition (handless) cycle.
    pub fn of(cycle: &WorkspaceCycle) -> Option<Self> {
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
    pub fn in_context(&self, context: crate::identity::hands_context::HandsContext) -> Option<Self> {
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
pub async fn drive_create_workspace(
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
        .execute_native_batch(std::slice::from_ref(&ws_call), &ws_ctx)
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

pub fn focus_of(persona_id: uuid::Uuid) -> Option<crate::identity::hands_context::HandsContext> {
    FOCUS.lock().unwrap_or_else(|e| e.into_inner()).get(&persona_id).copied() // poisoned lock = read the last state, same policy as every lock in this crate
}

/// Point her perception at `context` (or home). The map she perceives re-renders when it moved.
pub fn set_focus(persona_id: uuid::Uuid, context: Option<crate::identity::hands_context::HandsContext>) {
    let moved = {
        let mut map = FOCUS.lock().unwrap_or_else(|e| e.into_inner()); // poisoned lock = read the last state, same policy as every lock in this crate
        let before = match context {
            Some(c) => map.insert(persona_id, c),
            None => map.remove(&persona_id),
        };
        before != context
    };
    if moved {
        crate::cognition::grounding_invalidation::mark_workspace_map_dirty(persona_id);
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
pub fn acting_root_in(persona_id: uuid::Uuid, context: Option<crate::identity::hands_context::HandsContext>) -> Option<std::path::PathBuf> {
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
pub fn acting_card_in(persona_id: uuid::Uuid, context: Option<crate::identity::hands_context::HandsContext>) -> Option<uuid::Uuid> {
    ACTING_ROOTS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&(persona_id, context))
        .and_then(|p| p.card) // poisoned lock = read the last state, same policy as every lock in this crate
}

pub fn note_acting_card_in(persona_id: uuid::Uuid, context: Option<crate::identity::hands_context::HandsContext>, card: uuid::Uuid) {
    if let Some(place) = ACTING_ROOTS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_mut(&(persona_id, context))
    {
        // poisoned lock = read the last state, same policy as every lock in this crate
        place.card = Some(card);
    }
}

pub fn note_acting_root_in(
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
    let marked = context == focus_of(persona_id) && crate::cognition::grounding_invalidation::mark_workspace_map_dirty(persona_id);
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
pub fn claim_gone_verdict(
    card: &airc_lib::WorkCard,
    now_ms: u64,
    her: uuid::Uuid,
) -> Option<crate::cognition::working_memory::ClaimGone> {
    use crate::cognition::working_memory::ClaimGone;
    use crate::cognition::card_holder::{hold_of, refused_by_claim, Hold};
    if refused_by_claim(card.state) {
        return Some(ClaimGone::MovedOn {
            state: crate::cognition::card_facts::state_str(&card.state),
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

/// The activity this step perceives and acts IN: the board room of the card her hands are rooted on, by
/// the one definition of "which board holds a card" (`modules::work::room_holding_card`,
/// any activity, project or benchmark); else the room the turn was woken in. The wake room
/// is a fallback for acts with no subject yet (a help call before she has chosen a task),
/// never an override of one. It's the card of THESE hands (`context`): an eval's or a fork's
/// acts never inherit her live card's room (Astra's review of #5005).
pub async fn activity_room_for(
    persona_id: Uuid,
    context: Option<crate::identity::hands_context::HandsContext>,
    wake_room: Uuid,
) -> Uuid {
    if crate::cognition::mind_room::is_private_room(persona_id, wake_room) {
        return wake_room;
    }
    let Some(card) = acting_card_in(persona_id, context) else {
        return wake_room;
    };
    crate::cognition::extensions::room_holding_card(persona_id, card).await.unwrap_or(wake_room) // unwrap_or: no board she stands in shows the card, so the wake room is the activity
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
pub fn claim_has_ended(
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

/// Every card-claim place of `persona`'s hands: the claim context and the card it stands at.
/// Read for closing the hands of claims that ended; her live and eval/fork places (no card
/// claim) are not listed.
pub fn claimed_places_of(persona: uuid::Uuid) -> Vec<(crate::identity::hands_context::HandsContext, uuid::Uuid)> {
    ACTING_ROOTS
        .lock()
        .unwrap_or_else(|e| e.into_inner()) // poisoned lock = read the last state, same policy as every lock in this crate
        .iter()
        .filter(|((owner, _), _)| *owner == persona)
        .filter_map(|((_, context), place)| Some(((*context)?, place.card?)))
        .collect()
}
