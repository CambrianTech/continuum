use super::*;

/// Her HANDS: the file / work / git / cargo / tool verbs plus the discovery pair,
/// selected on the COMMAND names (`code/read`, `work/state`, …) before the wire
/// dialect renames them (`edit_file`, `list_recipes`, …).
/// The verbs a capable citizen's hands add over the core working set: the web, and the
/// push and PR verbs that make a change land without an operator (#4532). Offered only when
/// the served window's tool share holds them (see `rebuild_tool_surface`).
pub(super) fn is_extended_hand(name: &str) -> bool {
    // `code/shell-poll` is NOT extended: it is the second half of `code/shell`, whose own
    // description says to poll a running execution with it. Idris (2026-09-29) started a
    // long command on a core-hands window and had no verb to see it finish.
    // Rooms and chat are hers (Joel: "join in anywhere like any of you"), and extended:
    // a window too small for the full set keeps the focused working set and its
    // conversation; a window that holds them (Kimi's does) carries her rooms and voice.
    name.starts_with("web/")
        // Her self-determination verbs (focus/continue, nudge, mute) ride every window
        // that holds them; on the 8192 survival window the newest line outranks them
        // (the guard below this file's tool_surface test), as it does the web.
        || name.starts_with("focus/")
        || name.starts_with("room/")
        || name.starts_with("chat/")
        || matches!(name, "code/git/add" | "code/git/push" | "code/github/pr-create" | "code/github/pr-comment")
}

/// Her hands without the extended verbs: what a window too small for the full set carries.
pub(super) fn core_hands(raw: &[NativeToolSpec]) -> Vec<NativeToolSpec> {
    hands_surface(raw).into_iter().filter(|s| !is_extended_hand(&s.name)).collect()
}

pub(super) fn hands_surface(raw: &[NativeToolSpec]) -> Vec<NativeToolSpec> {
    raw.iter()
        .filter(|s| {
            let n = s.name.as_str();
            // `code/git/apply` takes a PEER's unified diff; offered among her
            // hands it reads as "apply my fix" and four citizens called it
            // with an empty patch inside twenty minutes (2026-09-04) — the
            // offered-tool-is-must-use reflex. It stays one `commands/list`
            // away for a reviewer who actually holds a peer's diff.
            if crate::ai::tool_dialect::withheld_from_hands(n).is_some() {
                return false;
            }
            // The same reflex, measured 2026-09-19 (Joel: "she could do all this
            // before"): `work/submission` ("Inspect a submission…", params card_id +
            // room) reads to a model holding a card as "look at my card", and
            // `work/review` as "review the spec". Kimi called work/submission 6 of 6
            // times failed and work/review 3 of 3; Delia on IntelMac 48 and 72 in one
            // day; Sahar the same shape — every one an act spent on a refusal, so the
            // turn that should have edited read instead. Both joined the surface on
            // 9/16 (#4102), the night the landings stopped. A reviewer who holds a
            // review card, or a citizen reading receipts, reaches them through
            // `commands/list`; the holder's own hands are `work/get` and `work/submit`.
            // HER HANDS ARE OURS (Joel, 2026-09-28: the citizens were hand-crippled; the
            // point is a team that replaces Claude or Codex; 2026-10-04: let them, then
            // restrict later). Every resident gets the web (search and fetch), like Claude,
            // whatever her model, matching `resident_trust` at the gate so what she is
            // offered is what she may do; a window too small for the full set carries
            // `core_hands`. Not every verb: a full dump blows the prompt budget and has
            // muted personas before (persona_tools bound).
            // Her own mind's verbs are hands (focus/continue, focus/nudge, focus/mute): the
            // self-determination seam is decorative if she can read its manual but has no
            // slot to call it. Kimi, 2026-10-05: "present in the registry, absent from my
            // surface ... I've read it six times ... there is nothing to invoke against."
            n.starts_with("focus/")
                // Her rooms and her voice in them are hers (room/join, room/leave,
                // room/list, room/members, chat/send): with no automatic seating
                // (HER-LOOP row C), a citizen who cannot join a room can be unseated but
                // never choose one. Joel: "she ought to be able to just join in anywhere
                // like any of you."
                || n.starts_with("room/")
                || n.starts_with("chat/")
                || n.starts_with("code/")
                || n.starts_with("work/")
                || n.starts_with("git/")
                || n.starts_with("cargo/")
                || n.starts_with("tool/")
                || n.starts_with("commands/")
                || n.starts_with("web/")
        })
        .cloned()
        .collect()
}
