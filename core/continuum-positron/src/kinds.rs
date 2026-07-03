//! Typed widget-kind enum.
//!
//! Positron's `StateEnvelope.kind` is a `String` on the wire — the
//! protocol stays open to consumer-defined widget vocabularies. On
//! continuum's substrate side we want a TYPED enum so:
//!
//! - the `match` on a payload variant is forced exhaustive by rustc,
//! - revisions key off `KnownKind` which is `Hash + Eq` for free,
//! - a typo at a builder call site is a compile error, not a runtime
//!   mismatch the host silently ignores.
//!
//! When a kind ships, `KnownKind::wire_name` returns the canonical
//! string the wire envelope carries. Hosts on the renderer side match
//! the string; this side never re-stringifies a typo.
//!
//! Per `[[strong-typing-across-boundaries]]`: encode the kind once at
//! the substrate seam; don't pass `&str` through every layer that
//! needs to dispatch on it.

/// The widget kinds continuum's substrate produces state for.
///
/// Adding a kind:
/// 1. Add a variant here.
/// 2. Add a `wire_name()` arm — that string is what the renderer side
///    matches on, and is the contract with the widget package.
/// 3. Add a typed payload struct (e.g. `chat::ChatViewState`) with
///    `#[derive(TS)]` so the widget side gets the generated type.
/// 4. Add a `StateBuilder::build_<kind>` helper or, if the payload is
///    a one-liner, call `StateBuilder::build(...)` directly with the
///    `KnownKind` variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KnownKind {
    /// `"chat"` — room chat: messages, roster, typing-in-progress.
    Chat,
    /// `"wall"` — the room's pinned shared documents (plan, rules,
    /// agenda, recipe): a supersede-projected board keyed by open
    /// consumer-defined category. See [`crate::wall::WallViewState`].
    Wall,
    /// `"kanban"` — the room's work board: cards (open→merged) grouped
    /// into lanes, projected from airc's event-sourced work board. See
    /// [`crate::kanban::KanbanViewState`].
    Kanban,
}

impl KnownKind {
    /// The on-wire kind string carried in `StateEnvelope.kind`. Must
    /// stay stable once shipped — consumers on the host side route by
    /// this name. Per `[[no-fallbacks-ever]]` there's no "default"
    /// arm: every variant maps to a concrete name.
    pub fn wire_name(self) -> &'static str {
        match self {
            KnownKind::Chat => "chat",
            KnownKind::Wall => "wall",
            KnownKind::Kanban => "kanban",
        }
    }
}

/// Revision key alias — revisions are per-`KnownKind` (one counter per
/// state instance), NOT per `(kind, layer)`.
///
/// The load-bearing semantic clarification from Fable on positron's
/// session protocol design:
///
/// > `ViewState::revision()` in the trait layer is ONE counter per
/// > state instance. Layer classifies an UPDATE's cadence, not state
/// > identity — there's ONE chat state; a typing flicker is an
/// > ephemeral-layer delivery OF it, a message arrival is a
/// > session-layer delivery OF it.
///
/// So `last_seen: [{kind: "chat", revision: 42}]` is the full key —
/// a subscriber asks "I last saw chat at revision 42, replay anything
/// past that". Layer affects DELIVERY CADENCE, not WHAT TO REPLAY.
///
/// Kept as a type alias rather than a newtype so a future addition
/// (e.g. multi-instance kinds where the same kind has parallel state
/// objects keyed by room id) can land without churning every call
/// site.
pub type RevisionKey = KnownKind;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_name_is_lowercase_widget_id() {
        // what this catches: regression where a refactor accidentally
        // changes the on-wire kind string. The renderer side routes
        // by this exact string; a silent change breaks the widget.
        assert_eq!(KnownKind::Chat.wire_name(), "chat");
        assert_eq!(KnownKind::Wall.wire_name(), "wall");
        assert_eq!(KnownKind::Kanban.wire_name(), "kanban");
    }
}
