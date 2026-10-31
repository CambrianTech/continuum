#![cfg(test)]
//! Guard: an identity field typed `String` must be DECLARED.
//!
//! Joel, 2026-08-13, after the `c0de0001-…` fake-UUID incident and the
//! `MessageId(String)` find: *"eliminate all smell or you will copy it."* That is
//! literally true here — a model reading this tree learns its conventions from it,
//! and `persona_id: String` was 55 sites teaching that ids are text. Prose does not
//! stop that. A failing test does.
//!
//! The rule: any `<something>_id: String` field in the crate must appear in
//! `LOOSE_IDS` with a category and a reason. A new one fails the build. A declared
//! one that gets FIXED also fails, so the list cannot rot into a graveyard the way
//! the comment on `StubAircCitizen::subscribe_all_rooms` did.
//!
//! Categories:
//! - **external** — another system owns the string on the wire (LiveKit participants).
//!   Legitimately not ours to type.
//! - **pending** — ours, but nothing on that path RESOLVES yet. Typing it as an
//!   identity today would assert something the code does not do, which is worse than
//!   leaving it text. Converts in the slice that wires resolution.
//! - **defect** — should already be typed, with what blocked it.
//!
//! Deliberately NOT a lint on the type alone: `String` is fine for a name, a label,
//! a model repo (`unsloth/Devstral-…`). What this catches is the *identity* names.

struct LooseId {
    file: &'static str,
    field: &'static str,
    why: &'static str,
}

/// Identity-shaped field names. A `String` here is what gets audited; anything
/// else in the crate is out of scope on purpose.
const ID_NAMES: &[&str] = &[
    "persona_id",
    "room_id",
    "user_id",
    "card_id",
    "peer_id",
    "context_id",
    "session_id",
    "message_id",
    "actor_id",
    "owner_id",
    "author_id",
    "sender_id",
    "citizen_id",
    "agent_id",
];

const LOOSE_IDS: &[LooseId] = &[
    LooseId { file: "code/file_engine.rs", field: "persona_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
    LooseId { file: "code/shell_session/max_output_lines_per_execution.rs", field: "persona_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
    LooseId { file: "code/shell_types.rs", field: "persona_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
    LooseId { file: "code/shell_types.rs", field: "session_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
    LooseId { file: "code/types.rs", field: "author_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
    LooseId { file: "cognition/eval/eval_task_result.rs", field: "persona_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
    LooseId { file: "cognition/eval/fork_eval_cycle_waiting.rs", field: "persona_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
    LooseId { file: "cognition/eval/format_exam_lesson.rs", field: "persona_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
    LooseId { file: "cognition/replay.rs", field: "room_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
    LooseId { file: "cognition/should_respond.rs", field: "persona_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
    LooseId { file: "cognition/should_respond.rs", field: "room_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
    LooseId { file: "commands/consciousness_context.rs", field: "room_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
    LooseId { file: "commands/multi_layer_recall.rs", field: "room_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
    LooseId { file: "commands/recall_hook.rs", field: "room_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
    LooseId { file: "commands/persona/wall/pin.rs", field: "room_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
    LooseId { file: "commands/cognition/inbox_message_request.rs", field: "room_id", why: "pending: airc wire id, arrives as text from the daemon. Types when the airc-side ids do — #396" },
    LooseId { file: "commands/cognition/inbox_message_request.rs", field: "sender_id", why: "pending: airc wire id, arrives as text from the daemon. Types when the airc-side ids do — #396" },
    LooseId { file: "persona/stream_rail.rs", field: "room_id", why: "pending: airc wire id, arrives as text from the daemon. Types when the airc-side ids do — #396" },
    LooseId { file: "persona/stream_rail.rs", field: "sender_id", why: "pending: airc wire id, arrives as text from the daemon. Types when the airc-side ids do — #396" },
    LooseId { file: "live/audio/mixer.rs", field: "user_id", why: "external: LiveKit participant/room identity — the media server owns this string on the wire" },
    LooseId { file: "live/audio/router.rs", field: "user_id", why: "external: LiveKit participant/room identity — the media server owns this string on the wire" },
    LooseId { file: "live/transport/bridge_client/live_kit_agent_manager.rs", field: "user_id", why: "external: LiveKit participant/room identity — the media server owns this string on the wire" },
    LooseId { file: "live/transport/call_server/text_preview_length.rs", field: "persona_id", why: "external: LiveKit participant/room identity — the media server owns this string on the wire" },
    LooseId { file: "live/transport/call_server/call_manager_2.rs", field: "user_id", why: "external: LiveKit participant/room identity — the media server owns this string on the wire" },
    LooseId { file: "live/transport/call_server/text_preview_length.rs", field: "user_id", why: "external: LiveKit participant/room identity — the media server owns this string on the wire" },
    LooseId { file: "live/transport/media.rs", field: "room_id", why: "external: LiveKit participant/room identity — the media server owns this string on the wire" },
    LooseId { file: "live/transport/media.rs", field: "user_id", why: "external: LiveKit participant/room identity — the media server owns this string on the wire" },
    LooseId { file: "live/types.rs", field: "persona_id", why: "external: LiveKit participant/room identity — the media server owns this string on the wire" },
    LooseId { file: "live/video/source.rs", field: "user_id", why: "external: LiveKit participant/room identity — the media server owns this string on the wire" },
    LooseId { file: "memory/recall.rs", field: "room_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
    LooseId { file: "memory/types.rs", field: "actor_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
    LooseId { file: "memory/types.rs", field: "context_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
    LooseId { file: "memory/types.rs", field: "persona_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
    LooseId { file: "memory/types.rs", field: "room_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
    LooseId { file: "modules/rag.rs", field: "room_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
    LooseId { file: "room.rs", field: "room_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
    LooseId { file: "work/work_card_state_changed.rs", field: "card_id", why: "pending: airc work-card id. Needs a CardRef/CardId split with airc's own resolver — #164" },
    LooseId { file: "work/work_create.rs", field: "card_id", why: "pending: airc work-card id. Needs a CardRef/CardId split with airc's own resolver — #164" },
    LooseId { file: "work/work_create_command.rs", field: "card_id", why: "pending: airc work-card id. Needs a CardRef/CardId split with airc's own resolver — #164" },
    LooseId { file: "work/work_heartbeat.rs", field: "card_id", why: "pending: airc work-card id. Needs a CardRef/CardId split with airc's own resolver — #164" },
    LooseId { file: "work/work_note.rs", field: "card_id", why: "pending: airc work-card id. Needs a CardRef/CardId split with airc's own resolver — #164" },
    LooseId { file: "work/work_similar.rs", field: "card_id", why: "pending: airc work-card id. Needs a CardRef/CardId split with airc's own resolver — #164" },
    LooseId { file: "work/submission/latest_submission.rs", field: "card_id", why: "pending: airc work-card id. Needs a CardRef/CardId split with airc's own resolver — #164" },
    LooseId { file: "work/submission/next_after_submission.rs", field: "card_id", why: "pending: airc work-card id. Needs a CardRef/CardId split with airc's own resolver — #164" },
    LooseId { file: "work/submission/latest_submission.rs", field: "card_id", why: "pending: the SAME airc work-card handle modules/work.rs carries, blocked on the same CardRef/CardId split with airc's own resolver — #164. The wire value is what the BOARD PRINTED (8 hex chars), and typing it `Uuid` is what caused #4357: serde refused the handle before any handler could name the problem or the form it wanted. Both handlers resolve it to a typed `WorkCardId` in their first lines, via `id_resolve::resolve_handle` against a room-scoped candidate set, and every output stays typed" },
    LooseId { file: "work/submission/next_after_submission.rs", field: "card_id", why: "pending: the SAME airc work-card handle modules/work.rs carries, blocked on the same CardRef/CardId split with airc's own resolver — #164. The wire value is what the BOARD PRINTED (8 hex chars), and typing it `Uuid` is what caused #4357: serde refused the handle before any handler could name the problem or the form it wanted. Both handlers resolve it to a typed `WorkCardId` in their first lines, via `id_resolve::resolve_handle` against a room-scoped candidate set, and every output stays typed" },
    LooseId { file: "persona/airc_admission.rs", field: "message_id", why: "pending: airc wire id, arrives as text from the daemon. Types when the airc-side ids do — #396" },
    LooseId { file: "persona/airc_admission.rs", field: "room_id", why: "pending: airc wire id, arrives as text from the daemon. Types when the airc-side ids do — #396" },
    LooseId { file: "persona/airc_admission.rs", field: "sender_id", why: "pending: airc wire id, arrives as text from the daemon. Types when the airc-side ids do — #396" },
    LooseId { file: "cognition/conversation/channel_items.rs", field: "context_id", why: "pending: airc wire id, arrives as text from the daemon. Types when the airc-side ids do — #396" },
    LooseId { file: "cognition/conversation/channel_items.rs", field: "persona_id", why: "pending: airc wire id, arrives as text from the daemon. Types when the airc-side ids do — #396" },
    LooseId { file: "cognition/conversation/channel_items.rs", field: "room_id", why: "pending: airc wire id, arrives as text from the daemon. Types when the airc-side ids do — #396" },
    LooseId { file: "cognition/conversation/channel_items.rs", field: "sender_id", why: "pending: airc wire id, arrives as text from the daemon. Types when the airc-side ids do — #396" },
    LooseId { file: "persona/durable_history.rs", field: "message_id", why: "pending: airc wire id, arrives as text from the daemon. Types when the airc-side ids do — #396" },
    LooseId { file: "persona/durable_history.rs", field: "sender_id", why: "pending: airc wire id, arrives as text from the daemon. Types when the airc-side ids do — #396" },
    LooseId { file: "engram.rs", field: "message_id", why: "pending: airc wire id, arrives as text from the daemon. Types when the airc-side ids do — #396" },
    LooseId { file: "engram.rs", field: "room_id", why: "pending: airc wire id, arrives as text from the daemon. Types when the airc-side ids do — #396" },
    LooseId { file: "engram.rs", field: "sender_id", why: "pending: airc wire id, arrives as text from the daemon. Types when the airc-side ids do — #396" },
    LooseId { file: "ai/types.rs", field: "persona_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
    LooseId { file: "ai/types.rs", field: "room_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
    LooseId { file: "ai/types.rs", field: "user_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
    LooseId { file: "routing/airc_realtime.rs", field: "user_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
    LooseId { file: "cognition/eval/sweep_orphan_eval_roots.rs", field: "room_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
    LooseId { file: "modules/dataset/dataset_manifest.rs", field: "room_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
    LooseId { file: "escalation.rs", field: "persona_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
    LooseId { file: "types.rs", field: "persona_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
    LooseId { file: "persona/service_loop/burst_fingerprint.rs", field: "room_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
    LooseId { file: "persona/service_loop/burst_fingerprint.rs", field: "sender_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
];

fn rs_files() -> Vec<(String, String)> {
    super::source_tree::crate_sources().into_iter().map(|f| (f.rel, f.text)).collect()
}

/// `  pub persona_id: String,` / `  room_id: Option<String>,` — field decls only.
/// Comments are stripped first so a doc line can never register as a field.
fn loose_id_fields(src: &str) -> Vec<&'static str> {
    let mut found = Vec::new();
    for raw in src.lines() {
        let line = match raw.find("//") {
            Some(idx) => &raw[..idx],
            None => raw,
        };
        let line = line.trim();
        // any visibility (`pub`, `pub(crate)`, `pub(super)`, `pub(in ..)`): a split that
        // widened a field must not hide it from this guard
        let line = match line.strip_prefix("pub") {
            Some(rest) if rest.starts_with(' ') => rest.trim_start(),
            Some(rest) if rest.starts_with('(') => rest.find(')').map_or(line, |end| rest[end + 1..].trim_start()),
            _ => line,
        };
        for name in ID_NAMES {
            let with_colon = format!("{name}:");
            if let Some(rest) = line.strip_prefix(&with_colon) {
                let rest = rest.trim();
                if rest == "String," || rest == "Option<String>," {
                    found.push(*name);
                }
            }
        }
    }
    found
}

/// Replace the contents of every `"…"` span with spaces, preserving byte
/// offsets so the caller's index arithmetic still lines up. Escaped quotes
/// (`\"`) do not close a span.
fn blank_string_literals(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut in_string = false;
    let mut escaped = false;
    for ch in line.chars() {
        if in_string {
            let closes = ch == '"' && !escaped;
            escaped = ch == '\\' && !escaped;
            out.push(if closes { '"' } else { ' ' });
            if closes {
                in_string = false;
            }
        } else {
            if ch == '"' {
                in_string = true;
                escaped = false;
            }
            out.push(ch);
        }
    }
    out
}

/// `fn f(peer_id: &str, …)` — identity names in PARAMETER position.
///
/// The field audit above never saw these: its predicate is
/// `line.strip_prefix("<name>:")` with the whole remainder equal to
/// `String,`, which by construction cannot match a signature. So every
/// `&str` id in the crate was invisible to a guard that reported clean.
///
/// Non-overlapping with [`loose_id_fields`] on purpose:
/// - a BORROWED form (`&str` / `&String`) is never a struct field here — it
///   would need a lifetime — so it is always a parameter;
/// - an OWNED form counts only when it is NOT the whole line, because a
///   whole-line `peer_id: String,` is exactly what the field audit claims.
///
/// Without that split both guards would report the same site, and fixing it
/// once would leave the other one red with nothing left to fix.
fn loose_id_params(src: &str) -> Vec<&'static str> {
    let mut found = Vec::new();
    for raw in src.lines() {
        let line = match raw.find("//") {
            Some(idx) => &raw[..idx],
            None => raw,
        };
        // Blank out double-quoted spans. Without this the audit reports
        // its OWN positive-control fixtures (`"peer_id: &str,"` a few
        // lines below) as three real findings in test_env/loose_id_guard.rs — a
        // detector that cannot tell code from a string about code.
        // `loose_id_fields` needs no equivalent: it demands the WHOLE
        // trimmed line be `name: String,`, which a quoted literal never is.
        let line = blank_string_literals(line);
        let line = line.trim();
        // any visibility (`pub`, `pub(crate)`, `pub(super)`, `pub(in ..)`): a split that
        // widened a field must not hide it from this guard
        let line = match line.strip_prefix("pub") {
            Some(rest) if rest.starts_with(' ') => rest.trim_start(),
            Some(rest) if rest.starts_with('(') => rest.find(')').map_or(line, |end| rest[end + 1..].trim_start()),
            _ => line,
        };
        for name in ID_NAMES {
            let mut from = 0;
            while let Some(rel) = line[from..].find(name) {
                let at = from + rel;
                from = at + name.len();
                // Left boundary: `signer_peer_id` must not register as
                // `peer_id`. `char::is_alphanumeric` alone does NOT do this
                // — `_` is not alphanumeric, so the underscore-joined
                // prefix would slip straight through.
                let left_ok = at == 0
                    || !matches!(line.as_bytes()[at - 1], b'_' | b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9');
                if !left_ok {
                    continue;
                }
                let Some(rest) = line[at + name.len()..].strip_prefix(':') else {
                    continue;
                };
                let rest = rest.trim_start();
                let borrowed = rest.starts_with("&str") || rest.starts_with("&String");
                let owned = rest.starts_with("String") || rest.starts_with("Option<String>");
                if borrowed || (owned && at > 0) {
                    found.push(*name);
                }
            }
        }
    }
    found
}

// what this catches: a predicate that silently matches NOTHING. A guard
// whose detector is broken does not go quiet — it reports the codebase
// clean, which is strictly worse than having no guard, because it converts
// an open defect into a green claim that the work is done.
//
// Written because exactly that happened while building this: a throwaway
// scan of this same shape returned 0 twice in a row, and 0 was not the
// answer — the detector was broken both times. A predicate is not trusted
// until it has been shown to fire on a known-true input AND stay silent on
// the near-misses that surround it.
#[test]
fn the_param_predicate_fires_and_does_not_over_fire() {
    assert_eq!(
        loose_id_params("    peer_id: &str,"),
        vec!["peer_id"],
        "must fire on the plain borrowed param — if this is empty the audit below is decorative"
    );
    assert_eq!(
        loose_id_params("fn f(persona_id: &str, x: u8)"),
        vec!["persona_id"],
        "must fire mid-signature, not only on its own line"
    );
    assert!(
        loose_id_params("    signer_peer_id: String,").is_empty(),
        "a `_`-joined SUFFIX is a different name and must not register as peer_id"
    );
    assert!(
        loose_id_params("    pub room_id: String,").is_empty(),
        "whole-line owned decl belongs to the FIELD audit — double-reporting makes one of them unfixable"
    );
    assert!(
        loose_id_params("    // peer_id: &str,").is_empty(),
        "a commented example must not register, same as the field audit"
    );
    assert!(
        loose_id_params("    assert_eq!(f(\"peer_id: &str,\"), x);").is_empty(),
        "an id inside a STRING LITERAL is text about code, not code — this audit \
         reported its own fixtures three times before the blanking step existed"
    );
}

// what this catches: a NEW `<something>_id: String` landing anywhere in the
// crate without a declaration. That is the exact shape that grew to 55
// persona_id sites, 5 String peer_ids, and a MessageId whose `new("msg-1")`
// let two callers collide — every one of them added one line at a time by
// someone (usually me) who did not know the convention.
#[test]
fn every_string_typed_identity_field_is_declared() {
    let mut undeclared: Vec<String> = Vec::new();
    for (file, src) in rs_files() {
        for field in loose_id_fields(&src) {
            let declared = LOOSE_IDS.iter().any(|d| d.file == file && d.field == field);
            if !declared {
                undeclared.push(format!("{file}: {field}"));
            }
        }
    }
    undeclared.sort();
    undeclared.dedup();
    assert!(
        undeclared.is_empty(),
        "{} identity field(s) typed `String` with no declaration:\n  {}\n\n\
         An id is not text. Use the typed form — `PeerId` for an actor, \
         `PersonaRef` for an unresolved caller reference, a `*Id(Uuid)` newtype \
         otherwise. If it genuinely must stay a String (another system owns the \
         wire format, or the resolver does not exist yet), add a LOOSE_IDS entry \
         in test_env/loose_id_guard.rs saying which and why.",
        undeclared.len(),
        undeclared.join("\n  ")
    );
}

// TEMPORARY: dumps the real param-position list so LOOSE_ID_PARAMS can be
// filled from a COMPILED run against the real tree, not from a scratch
// script. Deleted in the same commit that lands the table.
#[test]
#[ignore = "scaffolding: run with --ignored to regenerate LOOSE_ID_PARAMS"]
fn dump_param_positions() {
    let mut rows: Vec<String> = Vec::new();
    for (file, src) in rs_files() {
        let mut names = loose_id_params(&src);
        names.sort();
        names.dedup();
        for name in names {
            rows.push(format!("{file} | {name}"));
        }
    }
    rows.sort();
    panic!("{} (file,param) pairs:\n{}", rows.len(), rows.join("\n"));
}

// what this catches: the list rotting into a graveyard. A declaration whose
// field has actually been fixed must be DELETED, or the next reader believes
// a smell is still there and works around it.
#[test]
fn no_declaration_outlives_its_defect() {
    let files = rs_files();
    let mut stale: Vec<String> = Vec::new();
    for decl in LOOSE_IDS {
        let still_loose = files
            .iter()
            .any(|(file, src)| file == decl.file && loose_id_fields(src).contains(&decl.field));
        if !still_loose {
            stale.push(format!("{}: {}", decl.file, decl.field));
        }
    }
    assert!(
        stale.is_empty(),
        "{} LOOSE_IDS entr(ies) name a field that is no longer a loose String \
         — delete them, they are now telling the next reader a lie:\n  {}",
        stale.len(),
        stale.join("\n  ")
    );
}

// what this catches: a declaration used as a silent mute. Every entry states a
// category and a real reason, the same bar the module-wiring audit holds.
#[test]
fn declarations_carry_a_real_reason() {
    for decl in LOOSE_IDS {
        let categorized = ["external:", "pending:", "defect:"]
            .iter()
            .any(|c| decl.why.starts_with(c));
        assert!(
            categorized,
            "{}: {} — reason must start with external:/pending:/defect:, got {:?}",
            decl.file, decl.field, decl.why
        );
        assert!(
            decl.why.len() > 40,
            "{}: {} — reason is too thin to be a decision: {:?}",
            decl.file,
            decl.field,
            decl.why
        );
    }
}
