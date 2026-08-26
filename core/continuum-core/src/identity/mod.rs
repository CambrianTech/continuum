//! `Identity` — the substrate's universal actor identity, ORM-backed.
//!
//! Per [[airc-is-the-session-not-a-feature]] (Joel 2026-06-04):
//! "airc is CORE LEVEL. this is the session etc." + "Each UNIQUE
//! identity per persona, per you per me. Not shared." + "This is
//! like Android context and must be fixed."
//!
//! ## What this IS
//!
//! Identity is the ORM-backed actor identity that EVERY citizen on
//! the substrate carries — personas, Claude Code sessions, Codex
//! sessions, Joel-at-a-terminal, jtag CLI invocations, future web
//! users. Same shape, same table, foreign-keyable from every other
//! entity (engrams, work cards, captures, the lot).
//!
//! Per the Android-Context analogue: Identity is what makes
//! `&ctx` mean "this specific actor instance" rather than "the
//! process's global airc handle." There is no global. Code that
//! acts on the substrate either takes a `&Context` (which carries
//! an `Identity`) or it's a pure function.
//!
//! ## Why this is an Entity (not a struct + JSON file)
//!
//! Per [[no-sql-everything-through-orm-entities]] EVERY storable
//! datum goes through the ORM + entity registration. The prior
//! shape — `PersonaInstanceInfo` struct persisted as
//! `~/.continuum/personas/<name>/seed.json` — predates the
//! `#[derive(Entity)]` macro that #1519 / task #166 landed. With
//! the derive macro available, hand-rolled JSON-on-disk is
//! technical debt: untyped, non-queryable, no FK, no schema
//! migration.
//!
//! Identity uses Pattern A of the derive macro (canonical) —
//! `#[entity(primary_key)] id: Uuid` pulls in BaseEntity columns
//! (`id`, `createdAt`, `updatedAt`, `version`) automatically.
//!
//! ## Why `id == airc peer_id`
//!
//! Per [[persona-identity-derives-from-source-id]]: a persona's
//! peer_id IS its substrate identity. Same applies to every actor
//! kind — the airc Ed25519 keypair is what makes you YOU on the
//! substrate. So Identity's `id` is the peer_id directly, not a
//! separate continuum-side surrogate. Other entities that need to
//! reference an actor FK on `Identity.id` and route via airc
//! automatically.
//!
//! ## Out of scope for this slice
//!
//! - `Context` struct wrapping Identity + services + captures
//!   (Slice 2 of task #142)
//! - Bootstrap paths per IdentityKind (fresh Claude Code session
//!   minting its own Identity row + airc home; jtag CLI invocation
//!   minting ephemeral; etc. — Slice 3)
//! - `&ctx` ubiquitous refactor across substrate APIs (Slice 4)
//! - Migration of `PersonaInstanceInfo` callers to read from
//!   Identity table (Slice 1B, follow-up PR)

use continuum_orm_derive::Entity;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

/// The substrate's ONE canonical actor-identity type: `airc_core::PeerId`.
///
/// Every actor on the substrate — persona, agent session, human, jtag, web —
/// is identified by this single type. Per the module doctrine above ("your
/// airc cryptographic identity IS your substrate identity ... the peer_id
/// directly, not a separate continuum-side surrogate"), the canonical type is
/// airc's OWN `PeerId`: a `#[serde(transparent)]` UUIDv4 newtype
/// (`Copy + Hash + Eq + Display`, `from_uuid`/`as_uuid`) whose wire shape is the
/// bare hyphenated string — identical to what a bare `Uuid` serializes to. We
/// re-export it here so the whole crate imports identity from ONE home.
///
/// ## Do NOT define another `*Id(Uuid)` newtype for an actor
///
/// `genome::working_set::PersonaId` and `genome::recall::PeerId` were two
/// continuum-side re-inventions of exactly this concept; both were collapsed
/// onto this canonical type (2026-06-30, [[identity-one-canonical-newtype-not-bare-uuid]]).
/// A third re-invention re-opens the same drift — a persona IS a peer, so its
/// identity IS a `PeerId`; the field NAME (`persona:`, `node:`) carries the role.
///
/// For a ts-rs wire struct, type the field `PeerId` and annotate
/// `#[ts(type = "string")]` — airc's `PeerId` carries no `TS` derive by design
/// (it serializes transparently to the same string a `Uuid` would), so the
/// generated TS shape is unchanged.
pub use airc_core::PeerId;

pub mod activity;
pub use activity::{ActivityRoom, RoomlessTurn};

/// The durable on-disk home of one citizen: `<root>/citizens/peers/<peer_id>`.
///
/// THE ONE SPELLING of that layout. It was being rebuilt by hand at each use, in
/// two different shapes — `home.join("citizens/peers")` (`modules/work.rs`) and
/// `home.join("citizens").join("peers")` (`commands/benchmark.rs`) — plus prose
/// copies in `persona_workspace.rs` and `persona_roster.rs`. Four expressions of
/// one decision is exactly the drift the compression principle forbids, and the
/// next writer (the lived-turn experience stream) would have made a fifth.
///
/// Keyed by [`PeerId`], never a `String`: the citizen's identity IS the directory
/// name, so a caller that has not resolved a name to an identity cannot address
/// her storage by accident ([[uuids-are-not-strings-and-never-hand-drawn]]).
///
/// Pure path arithmetic — creates nothing, checks nothing. Callers that need the
/// directory to exist say so themselves, so a read-only caller never has the side
/// effect of minting an empty citizen dir.
pub fn citizen_peer_dir(root: &std::path::Path, peer: PeerId) -> std::path::PathBuf {
    root.join("citizens")
        .join("peers")
        .join(peer.as_uuid().to_string())
}

/// What a CALLER writes when it means "that persona" — a full UUID, an 8-char
/// short-id, or a name (`"Asha"`). Deliberately NOT an identity.
///
/// ## Why this is a separate type from [`PeerId`]
///
/// Both were `String`, so nothing stopped an unresolved reference being stored,
/// compared, or handed to a subsystem as though it were an identity — and that is
/// not hypothetical. `PersonaWorkspaceRegistry::resolve_persona` exists precisely
/// to close "the loose-`String` id boundary … the defect class that fed a dead id
/// to a doomed eval" (its own words), and when this type was introduced it had
/// **one** production caller against 55 `persona_id: String` fields. The check was
/// right and almost nothing called it — the nastiest shape a check can have.
///
/// A name is not an identity: it is ambiguous (two personas can share one), it is
/// mutable, and it is only meaningful against a live roster. So the two roles get
/// two types, and the ONLY bridge between them is resolution:
///
/// ```ignore
/// let id: PeerId = registry.resolve_persona(&params.persona)?;  // the one door
/// ```
///
/// Params carry a `PersonaRef`. Everything downstream carries a [`PeerId`]. Passing
/// an unresolved reference where an identity belongs is now a type error rather
/// than a runtime surprise three subsystems away.
///
/// Wire shape is unchanged — `#[serde(transparent)]` over the string a caller
/// already sends, so no client, recipe, or stored payload has to change.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    TS,
    schemars::JsonSchema,
)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/identity/PersonaRef.ts"
)]
#[serde(transparent)]
#[schemars(description = "A persona reference: full UUID, 8-char short-id, or name")]
pub struct PersonaRef(pub String);

impl PersonaRef {
    pub fn new(reference: impl Into<String>) -> Self {
        Self(reference.into())
    }

    /// The raw text, for the resolver and for error messages. Deliberately the ONLY
    /// accessor — there is no `as_peer_id()`, because a reference is not an identity
    /// until a roster says which one it is.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for PersonaRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for PersonaRef {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl From<String> for PersonaRef {
    fn from(s: String) -> Self {
        Self(s)
    }
}

/// A resolved identity is always a legal reference to itself — the direction that
/// is safe. The reverse has no `From` on purpose: it requires a roster.
impl From<PeerId> for PersonaRef {
    fn from(id: PeerId) -> Self {
        Self(id.to_string())
    }
}

/// Guard: an identity field typed `String` must be DECLARED.
///
/// Joel, 2026-08-13, after the `c0de0001-…` fake-UUID incident and the
/// `MessageId(String)` find: *"eliminate all smell or you will copy it."* That is
/// literally true here — a model reading this tree learns its conventions from it,
/// and `persona_id: String` was 55 sites teaching that ids are text. Prose does not
/// stop that. A failing test does.
///
/// The rule: any `<something>_id: String` field in the crate must appear in
/// [`LOOSE_IDS`] with a category and a reason. A new one fails the build. A declared
/// one that gets FIXED also fails, so the list cannot rot into a graveyard the way
/// the comment on `StubAircCitizen::subscribe_all_rooms` did.
///
/// Categories:
/// - **external** — another system owns the string on the wire (LiveKit participants).
///   Legitimately not ours to type.
/// - **pending** — ours, but nothing on that path RESOLVES yet. Typing it as an
///   identity today would assert something the code does not do, which is worse than
///   leaving it text. Converts in the slice that wires resolution.
/// - **defect** — should already be typed, with what blocked it.
///
/// Deliberately NOT a lint on the type alone: `String` is fine for a name, a label,
/// a model repo (`unsloth/Devstral-…`). What this catches is the *identity* names.
#[cfg(test)]
mod loose_id_guard {
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
        LooseId { file: "code/shell_session.rs", field: "persona_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
        LooseId { file: "code/shell_types.rs", field: "persona_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
        LooseId { file: "code/shell_types.rs", field: "session_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
        LooseId { file: "code/types.rs", field: "author_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
        LooseId { file: "cognition/eval.rs", field: "persona_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
        LooseId { file: "cognition/generate_response.rs", field: "persona_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
        LooseId { file: "cognition/generate_response.rs", field: "room_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
        LooseId { file: "cognition/prompt_capture.rs", field: "persona_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
        LooseId { file: "cognition/prompt_capture.rs", field: "room_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
        LooseId { file: "cognition/replay.rs", field: "room_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
        LooseId { file: "cognition/should_respond.rs", field: "persona_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
        LooseId { file: "cognition/should_respond.rs", field: "room_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
        LooseId { file: "cognition/workspace_capture.rs", field: "persona_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
        LooseId { file: "cognition/workspace_capture.rs", field: "room_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
        LooseId { file: "cognition/workspace_dashboard.rs", field: "persona_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
        LooseId { file: "cognition/workspace_dashboard.rs", field: "room_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
        LooseId { file: "commands/memory/consciousness_context.rs", field: "room_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
        LooseId { file: "commands/memory/multi_layer_recall.rs", field: "room_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
        LooseId { file: "commands/memory/recall_hook.rs", field: "room_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
        LooseId { file: "commands/persona/wall/pin.rs", field: "room_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
        LooseId { file: "ipc/protocol.rs", field: "room_id", why: "pending: airc wire id, arrives as text from the daemon. Types when the airc-side ids do — #396" },
        LooseId { file: "ipc/protocol.rs", field: "sender_id", why: "pending: airc wire id, arrives as text from the daemon. Types when the airc-side ids do — #396" },
        LooseId { file: "ipc/stream_rail.rs", field: "room_id", why: "pending: airc wire id, arrives as text from the daemon. Types when the airc-side ids do — #396" },
        LooseId { file: "ipc/stream_rail.rs", field: "sender_id", why: "pending: airc wire id, arrives as text from the daemon. Types when the airc-side ids do — #396" },
        LooseId { file: "live/audio/mixer.rs", field: "user_id", why: "external: LiveKit participant/room identity — the media server owns this string on the wire" },
        LooseId { file: "live/audio/router.rs", field: "user_id", why: "external: LiveKit participant/room identity — the media server owns this string on the wire" },
        LooseId { file: "live/transport/bridge_client.rs", field: "user_id", why: "external: LiveKit participant/room identity — the media server owns this string on the wire" },
        LooseId { file: "live/transport/call_server.rs", field: "persona_id", why: "external: LiveKit participant/room identity — the media server owns this string on the wire" },
        LooseId { file: "live/transport/call_server.rs", field: "user_id", why: "external: LiveKit participant/room identity — the media server owns this string on the wire" },
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
        LooseId { file: "modules/room.rs", field: "room_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
        LooseId { file: "modules/sentinel/escalation.rs", field: "persona_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
        LooseId { file: "modules/work.rs", field: "card_id", why: "pending: airc work-card id. Needs a CardRef/CardId split with airc's own resolver — #164" },
        LooseId { file: "persona/airc_admission.rs", field: "message_id", why: "pending: airc wire id, arrives as text from the daemon. Types when the airc-side ids do — #396" },
        LooseId { file: "persona/airc_admission.rs", field: "room_id", why: "pending: airc wire id, arrives as text from the daemon. Types when the airc-side ids do — #396" },
        LooseId { file: "persona/airc_admission.rs", field: "sender_id", why: "pending: airc wire id, arrives as text from the daemon. Types when the airc-side ids do — #396" },
        LooseId { file: "persona/channel_items.rs", field: "context_id", why: "pending: airc wire id, arrives as text from the daemon. Types when the airc-side ids do — #396" },
        LooseId { file: "persona/channel_items.rs", field: "persona_id", why: "pending: airc wire id, arrives as text from the daemon. Types when the airc-side ids do — #396" },
        LooseId { file: "persona/channel_items.rs", field: "room_id", why: "pending: airc wire id, arrives as text from the daemon. Types when the airc-side ids do — #396" },
        LooseId { file: "persona/channel_items.rs", field: "sender_id", why: "pending: airc wire id, arrives as text from the daemon. Types when the airc-side ids do — #396" },
        LooseId { file: "persona/durable_history.rs", field: "message_id", why: "pending: airc wire id, arrives as text from the daemon. Types when the airc-side ids do — #396" },
        LooseId { file: "persona/durable_history.rs", field: "sender_id", why: "pending: airc wire id, arrives as text from the daemon. Types when the airc-side ids do — #396" },
        LooseId { file: "persona/engram.rs", field: "message_id", why: "pending: airc wire id, arrives as text from the daemon. Types when the airc-side ids do — #396" },
        LooseId { file: "persona/engram.rs", field: "room_id", why: "pending: airc wire id, arrives as text from the daemon. Types when the airc-side ids do — #396" },
        LooseId { file: "persona/engram.rs", field: "sender_id", why: "pending: airc wire id, arrives as text from the daemon. Types when the airc-side ids do — #396" },
        LooseId { file: "persona/projection.rs", field: "persona_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
        LooseId { file: "ai/types.rs", field: "persona_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
        LooseId { file: "ai/types.rs", field: "room_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
        LooseId { file: "ai/types.rs", field: "user_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
        LooseId { file: "airc/realtime.rs", field: "user_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
        LooseId { file: "cognition/eval.rs", field: "room_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
        LooseId { file: "cognition/resolution_compute.rs", field: "persona_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
        LooseId { file: "logging/client.rs", field: "session_id", why: "external: log-envelope correlation fields, written as text to a JSONL sink another tool reads" },
        LooseId { file: "logging/client.rs", field: "user_id", why: "external: log-envelope correlation fields, written as text to a JSONL sink another tool reads" },
        LooseId { file: "modules/dataset.rs", field: "room_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
        LooseId { file: "modules/sentinel/types.rs", field: "persona_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
        LooseId { file: "persona/service_loop.rs", field: "room_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
        LooseId { file: "persona/service_loop.rs", field: "sender_id", why: "pending: ours, but nothing on this path RESOLVES yet. Typing it as an identity today would assert something the code does not do — #164/#396" },
    ];

    fn rs_files() -> Vec<(String, String)> {
        fn walk(dir: &std::path::Path, root: &std::path::Path, out: &mut Vec<(String, String)>) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, root, out);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    if let Ok(text) = std::fs::read_to_string(&path) {
                        let rel = path.strip_prefix(root).unwrap_or(&path);
                        out.push((rel.to_string_lossy().replace('\\', "/"), text));
                    }
                }
            }
        }
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut out = Vec::new();
        walk(&root, &root, &mut out);
        out
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
            let line = line.strip_prefix("pub ").unwrap_or(line);
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
            // lines below) as three real findings in identity/mod.rs — a
            // detector that cannot tell code from a string about code.
            // `loose_id_fields` needs no equivalent: it demands the WHOLE
            // trimmed line be `name: String,`, which a quoted literal never is.
            let line = blank_string_literals(line);
            let line = line.trim();
            let line = line.strip_prefix("pub ").unwrap_or(line);
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
             in identity/mod.rs saying which and why.",
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
}

/// What kind of actor this identity belongs to. The substrate
/// treats every kind symmetrically — same Identity entity, same
/// ORM table, same airc-peer routing — but the kind tag lets
/// downstream code branch when the actor type matters (e.g.,
/// cognition pipeline runs for `Persona` but not for `Jtag`).
///
/// Per [[airc-is-the-session-not-a-feature]] every value here is a
/// first-class substrate citizen; none is "second-class" or "for
/// internal use."
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/identity/IdentityKind.ts"
)]
pub enum IdentityKind {
    /// An autonomous persona — has a name, cognition pipeline,
    /// engrams, optional LoRA genome. Bootstraps via
    /// `PersonaIdentityProvider` at substrate start.
    Persona,
    /// An external AI agent session — Claude Code instance, Codex
    /// session, Gemini, Hermes, OpenClaw, future provider. The
    /// SPECIFIC provider is carried in `Identity::agent_provider`
    /// so adding a new agent provider (GPT-5, Claude-5, whatever)
    /// doesn't churn this enum. Per the Slice-1 reviewer's
    /// extensibility concern + [[organization-purity-as-we-migrate]]
    /// + Joel 2026-06-04 ("Codex and Gemini etc. Use always
    /// lowercase").
    Agent,
    /// A human at a terminal / IDE — one row per active session
    /// (one Joel-at-laptop, another Joel-at-iMac). Bootstrapped
    /// via human-presence detection (login, IDE attach) or
    /// explicit `airc init`.
    Human,
    /// A jtag CLI invocation. Can be ephemeral (mint on each
    /// `jtag X` call, retire on exit) or long-lived (one identity
    /// per user, persisted across invocations) — TBD by Slice 5.
    Jtag,
    /// A browser tab / web user. One row per tab session.
    Web,
}

/// How this identity came into being — resumed from prior state
/// or minted fresh. Telemetry-honest per
/// [[substrate-is-a-good-citizen-on-the-host]] so operators can
/// see at a glance whether the substrate is rehydrating prior
/// citizens or spawning new ones.
///
/// Renamed from `PersonaIdentitySource` to `IdentitySource` per
/// the universal-kind shape — same enum now applies to every
/// `IdentityKind`, not just `Persona`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/identity/IdentitySource.ts"
)]
pub enum IdentitySource {
    /// Rehydrated from a prior session — keypair loaded from
    /// `home_path/identity.key`, ORM row already existed.
    ResumedFromDisk,
    /// Freshly minted — new keypair generated, new home_path
    /// carved out, new ORM row inserted.
    FreshlyMinted,
}

/// The substrate's universal actor identity, ORM-backed.
///
/// One row per active actor instance. Foreign-keyable from every
/// other entity that needs to record "which citizen did this" —
/// engram authorship, work-card claims, capture-sink scopes,
/// audit trails.
///
/// Pattern A of `#[derive(Entity)]` per #1519: `id` is both the
/// primary key AND the airc peer_id. The macro pulls in BaseEntity
/// columns (`createdAt`, `updatedAt`, `version`) automatically;
/// the struct only declares the kind-specific fields.
#[derive(Debug, Clone, Serialize, Deserialize, TS, Entity)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/identity/Identity.ts"
)]
#[entity(collection = "identities")]
pub struct Identity {
    /// Primary key AND airc peer_id. The substrate makes no
    /// distinction — your airc cryptographic identity IS your
    /// substrate identity. Other entities FK on this.
    #[ts(type = "string")]
    #[entity(primary_key)]
    pub id: Uuid,

    /// What kind of actor this identity belongs to. Indexed so
    /// queries like "show me all live Persona identities" or
    /// "count active Claude sessions" stay O(log N).
    #[entity(indexed)]
    pub kind: IdentityKind,

    /// Human-readable label. For personas this is the persona
    /// name (Maya, Niko, ...). For Claude/Codex sessions it's a
    /// session identifier (e.g., "Claude-Opus-4.7-2026-06-04T..."
    /// — runtime decides shape). For humans it's the operator's
    /// chosen handle. Indexed for query-by-name workflows
    /// (`airc whois <name>` and similar).
    #[entity(indexed)]
    pub agent_name: String,

    /// Absolute path to this identity's airc home dir on disk.
    /// Carries the keypair (`identity.key`) and the airc state DB.
    /// String (not PathBuf) because PathBuf doesn't have native
    /// SQLite affinity; convert at the use site.
    pub home_path: String,

    /// The room this identity defaults to subscribing to at
    /// bootstrap. For personas it's the spawned-into room
    /// (continuum's default_room). For Claude/Codex it's the
    /// room their work coordinates in. Indexed for "show me
    /// every identity in room X" queries.
    #[ts(type = "string")]
    #[entity(indexed)]
    pub default_room: Uuid,

    /// Whether this identity was rehydrated from disk or minted
    /// fresh during this bootstrap. Indexed for telemetry —
    /// "what fraction of citizens are fresh this hour?" is a
    /// meaningful operator question.
    #[entity(indexed)]
    pub source: IdentitySource,

    /// For `IdentityKind::Agent` rows, names the SPECIFIC external
    /// AI provider — "claude", "codex", "gemini", "hermes",
    /// "openclaw", future. Lowercase by convention (Joel 2026-06-04
    /// "Use always lowercase"). `None` for non-Agent kinds.
    ///
    /// Why a `String` and not a sub-enum: extensibility. Adding a
    /// new provider should NOT churn the IdentityKind enum or
    /// require enum-as-JSON migration. The substrate routes
    /// agent-provider-specific logic through this string at the
    /// few sites that need it (tool-use harness, model-tier
    /// metadata).
    ///
    /// The directory layout for Agent kinds carries the provider:
    /// `citizens/agents/<provider>/<label>/airc/`. Non-Agent kinds
    /// use `citizens/<kind>/<label>/airc/` (no provider segment).
    #[entity(indexed)]
    #[ts(optional)]
    pub agent_provider: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orm::{OrmEntity, OrmStore};
    use std::sync::Arc;

    /// Smoke test that the derive macro generated a valid OrmEntity
    /// impl: schema parses, collection name matches `#[entity(collection)]`,
    /// every declared field (serde camelCase-renamed) is present in
    /// the schema, BaseEntity columns are auto-injected.
    #[test]
    fn identity_schema_is_derived() {
        let schema = Identity::collection_schema();
        assert_eq!(schema.collection, "identities");
        let field_names: Vec<&str> = schema.fields.iter().map(|f| f.name.as_str()).collect();
        // BaseEntity columns auto-injected by the derive when
        // `#[entity(primary_key)]` is on `id: Uuid`.
        assert!(field_names.contains(&"id"), "id missing");
        assert!(field_names.contains(&"createdAt"), "createdAt missing");
        assert!(field_names.contains(&"updatedAt"), "updatedAt missing");
        // Declared fields, camelCased via #[serde(rename_all = "camelCase")].
        assert!(field_names.contains(&"kind"), "kind missing");
        assert!(field_names.contains(&"agentName"), "agentName missing");
        assert!(field_names.contains(&"homePath"), "homePath missing");
        assert!(field_names.contains(&"defaultRoom"), "defaultRoom missing");
        assert!(field_names.contains(&"source"), "source missing");
        assert!(
            field_names.contains(&"agentProvider"),
            "agentProvider missing"
        );
    }

    /// Identity round-trips through OrmStore: save, find-by-id,
    /// find-all. Proves the entity layer works for substrate's
    /// universal actor identity, the foundation of task #142's
    /// BaseUser/Context hierarchy.
    ///
    /// Query-by-kind / query-by-room (the predicate-pushdown API)
    /// is exercised when the query-builder layer lands; for now
    /// `find_all` + manual filter is the canonical read path.
    #[tokio::test]
    async fn identity_round_trips_through_orm() {
        let (adapter, _tmp) = crate::orm::store::fresh_adapter().await;
        let store: OrmStore<Identity> = OrmStore::new(Arc::clone(&adapter))
            .await
            .expect("new store");

        let shared_room = Uuid::new_v4();
        let alice = Identity {
            id: Uuid::new_v4(),
            kind: IdentityKind::Persona,
            agent_name: "Maya".to_string(),
            home_path: "/tmp/test/maya/airc".to_string(),
            default_room: shared_room,
            source: IdentitySource::FreshlyMinted,
            agent_provider: None,
        };
        let bob = Identity {
            id: Uuid::new_v4(),
            kind: IdentityKind::Agent,
            agent_name: "claude-session-X".to_string(),
            home_path: "/tmp/test/claude-x/airc".to_string(),
            default_room: shared_room,
            source: IdentitySource::FreshlyMinted,
            agent_provider: Some("claude".to_string()),
        };

        store.save(alice.id, &alice).await.expect("save alice");
        store.save(bob.id, &bob).await.expect("save bob");

        let loaded_alice = store
            .find_by_id(alice.id)
            .await
            .expect("find alice")
            .expect("alice exists");
        assert_eq!(loaded_alice.agent_name, "Maya");
        assert_eq!(loaded_alice.kind, IdentityKind::Persona);
        assert_eq!(loaded_alice.id, alice.id);
        assert_eq!(loaded_alice.default_room, shared_room);
        assert_eq!(loaded_alice.agent_provider, None);

        let all = store.find_all().await.expect("find_all");
        assert_eq!(all.len(), 2, "both identities present");

        // Manual filter by kind — exercises the round-trip end to
        // end without depending on a query-builder API. When the
        // predicate-pushdown layer lands, this becomes a single
        // filter_eq call; until then this proves the data is there
        // and decodable.
        let personas: Vec<_> = all
            .iter()
            .filter(|(_, i)| i.kind == IdentityKind::Persona)
            .collect();
        assert_eq!(personas.len(), 1);
        assert_eq!(personas[0].1.agent_name, "Maya");

        let agents: Vec<_> = all
            .iter()
            .filter(|(_, i)| i.kind == IdentityKind::Agent)
            .collect();
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].1.agent_name, "claude-session-X");
        assert_eq!(agents[0].1.agent_provider.as_deref(), Some("claude"));
    }

    // what this catches: a FIFTH hand-rolled spelling of the citizen storage layout.
    // It was already written four ways — `join("citizens/peers")` in modules/work.rs,
    // `join("citizens").join("peers")` in commands/benchmark.rs, and prose copies in
    // persona_workspace.rs + persona_roster.rs. The two path spellings produce the SAME
    // path today, which is exactly why the drift is invisible until one of them changes.
    // Pins the shape AND that it is pure arithmetic: a read-only caller must never have
    // the side effect of minting an empty citizen dir.
    #[test]
    fn the_citizen_peer_dir_has_exactly_one_spelling_and_creates_nothing() {
        let peer = PeerId::from_u128(0xfe4dac17_0000_4000_8000_000000000001);
        let root = std::path::Path::new("/x/.continuum");
        let dir = citizen_peer_dir(root, peer);

        assert_eq!(
            dir,
            root.join("citizens").join("peers").join(peer.to_string()),
            "the layout is <root>/citizens/peers/<peer_id>"
        );
        // Identity, not a formatted string: the directory name IS the peer id.
        assert!(dir.ends_with(peer.as_uuid().to_string()));
        // Pure: nothing was created under a temp root either.
        let tmp = tempfile::tempdir().expect("tempdir");
        let under_tmp = citizen_peer_dir(tmp.path(), peer);
        assert!(
            !under_tmp.exists(),
            "resolving a path must not create it — a read-only caller mints nothing"
        );
    }
}
