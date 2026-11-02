//! The roster's projection for a mind: one unit per member (the one [`RagRenderable`] kind so far).
//! Kept beside the generic adapter in [`super`], so the adapter and each kind it renders are
//! separate concerns.

use super::{RagRenderUnit, RagRenderable};

/// Who is present, rendered for a mind from the SAME `RosterViewState` the web
/// roster renders.
///
/// This is the measured defect's cure: a citizen's prompt contained a live peer's
/// name **zero times** while the framing prose promised "who is present", because
/// her roster came from a different reader than the browser's
/// ([[citizens-cannot-see-each-other-the-prompt-promises-presence-and-delivers-nothing]]).
///
/// Unit = one member, because a half-rendered person is not a person. Ordering
/// follows the projection's own order (presence order), so the citizen and the
/// browser list the room the same way.
impl RagRenderable for continuum_positron::RosterViewState {
    const KIND: &'static str = continuum_positron::RosterViewState::KIND;
    const BLOCK: &'static str = "who is here";
    const EXPAND: Option<&'static str> = Some("room/members");
    fn units(&self, viewer: uuid::Uuid) -> Vec<RagRenderUnit> {
        // STABLE ORDER for the mind (2026-09-01, Benchy hit_rate=0.0): this
        // block renders at ~0.4% prompt depth, and "presence order" rotates
        // as heartbeats land — his consecutive prompts diverged at char ~226
        // on exactly this rotation, re-prefilling the whole ~50k tail every
        // turn while peers cached 0.4-0.8. The browser may sort presence-first
        // for human eyes; the MIND's copy sorts by name so the same people are
        // the same bytes. Set parity with the screen is unchanged — only the
        // line order is pinned ([[a-mutating-system-prompt-destroys-kv-reuse]]).
        let mut ordered: Vec<_> = self.roster.iter().collect();
        ordered.sort_by(|a, b| {
            a.display_name
                .cmp(&b.display_name)
                .then_with(|| a.member_id.cmp(&b.member_id))
        });
        ordered
            .into_iter()
            .map(|slot| member_unit(slot.member_id, &slot.display_name, slot.kind, slot.role_label.as_deref(), viewer))
            .collect()
    }

    /// The roster describes ONE room, so it rides the shared room gate.
    fn room(&self) -> Option<uuid::Uuid> {
        Some(self.room_id)
    }
}

/// One member's line in "who is here": the ONE renderer for a member row, used by the roster and
/// by her own row where no shared roster exists (her mind room).
///
/// WHICH ONE IS HER (2026-09-06, card bb387821): a citizen appeared in her own roster formatted
/// identically to her peers and answered with a peer's name, both directions; marking the reader
/// costs four characters and removes the ambiguity. She stays IN the list: a citizen is present
/// in her own room. NO liveness flag (2026-09-04): `[ready]`/`[busy]` flipped with every
/// heartbeat and forfeited the cached prefix; who, kind and role are the stable facts.
/// Key = peer id (stable across turns); pointer = the bare name, so a collapsed row still says
/// WHO, and `room/members` yields the rest.
pub(crate) fn member_unit(
    member_id: uuid::Uuid,
    display_name: &str,
    kind: continuum_positron::SenderKind,
    role: Option<&str>,
    viewer: uuid::Uuid,
) -> RagRenderUnit {
    let mut line = if member_id == viewer {
        format!("{display_name} ({kind:?}, YOU)")
    } else {
        format!("{display_name} ({kind:?})")
    };
    if let Some(role) = role.filter(|r| !r.is_empty()) {
        line.push_str(&format!(" — {role}"));
    }
    let mut unit = RagRenderUnit::new(member_id.to_string(), line);
    unit.pointer = Some(display_name.to_string());
    unit.metadata.insert("peer_id".into(), serde_json::json!(member_id));
    unit.metadata.insert("display_name".into(), serde_json::json!(display_name));
    unit
}
