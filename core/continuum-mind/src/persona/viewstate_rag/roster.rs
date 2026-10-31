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
            .map(|slot| {
                // Kind and role are what make a name actionable ("who can I ask?"),
                // so they ride the SAME unit as the name rather than a second block
                // a tight budget would sever from it.
                // WHICH ONE IS HER (2026-09-06, card bb387821): a citizen appeared in
                // her own roster formatted identically to her peers, with no marker.
                // Measured on IntelMac, BOTH directions, both with correct prompts —
                // Paige's prompt said "You are Paige" five times plus an explicit
                // NOT-list and she answered "I am Saoirse"; Saoirse's said "You are
                // Saoirse" and she answered "My name is Paige". The prose asserted an
                // identity five times while the STRUCTURED list beside it presented two
                // interchangeable agents, and the structure won. Marking the reader
                // costs four characters and removes the ambiguity the prose was losing
                // to. She stays IN the list — a citizen is present in her own room, and
                // deleting her row would make "who is here" undercount the room.
                let mut line = if slot.member_id == viewer {
                    format!("{} ({:?}, YOU)", slot.display_name, slot.kind)
                } else {
                    format!("{} ({:?})", slot.display_name, slot.kind)
                };
                if let Some(role) = slot.role_label.as_deref().filter(|r| !r.is_empty()) {
                    line.push_str(&format!(" — {role}"));
                }
                // NO liveness flag in the mind's copy (2026-09-04): `[ready]`/`[busy]`
                // flips with every heartbeat, and this block sits inside the STABLE
                // system prefix — consecutive prompts of one citizen diverged at char
                // ~9.7k of a 10.1k system on exactly this flag, forfeiting the whole
                // prefix (hit_rate 0.0 on 57/57 turns). Who is here, their kind and
                // role, are the stable facts; whether a peer is busy this second is
                // presence — the live view carries it, and it belongs in the volatile
                // tail if a turn ever needs it ([[a-mutating-system-prompt-destroys-kv-reuse]]).
                // Key = peer id (stable across turns). Pointer = the bare name: a member
                // line is ~10 tokens so the view rarely collapses one, but when it does she
                // still knows WHO is here, and `room/members` yields the rest.
                let mut unit = RagRenderUnit::new(slot.member_id.to_string(), line);
                unit.pointer = Some(slot.display_name.clone());
                unit.metadata
                    .insert("peer_id".into(), serde_json::json!(slot.member_id));
                unit.metadata
                    .insert("display_name".into(), serde_json::json!(slot.display_name));
                unit
            })
            .collect()
    }

    /// The roster describes ONE room, so it rides the shared room gate.
    fn room(&self) -> Option<uuid::Uuid> {
        Some(self.room_id)
    }
}
