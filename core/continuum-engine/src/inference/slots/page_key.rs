//! Whose warm KV a page holds: one page per MIND, and a separate class for a fork.
//!
//! The key was `(persona, room)` — one page per activity — because one persona running N
//! concurrent activities (parallel benchmark solves, each in its own room) thrashed a
//! persona-only key to 0% reuse. Two things changed that make one page per mind right
//! (plan line 3b, card 424e0961): her prompt is ONE epoch view per persona whose system
//! message is identical across rooms (#5325), so every room's act extends the same
//! prefix; and a mind acts one act at a time ([`super::mind_gate`]), so nothing of hers
//! runs in parallel to thrash it. Per-room pages paid a full restore (or a cold prefill)
//! every time she moved between rooms, for a prefix that was already the same.
//!
//! A FORK is not her mind: an eval's measurement copy acts BESIDE the living mind, on her
//! lane, with an exam prompt. Sharing her page would serialize the exam behind her and
//! overwrite her warm view with exam KV, so a fork keeps the per-activity key, under its
//! own name, and never touches her page.
//!
//! Typed UUIDs, never a formatted composite (`no_string_composite_id_keys_in_serving`).

use std::fmt;

use uuid::Uuid;

/// A resident's one mind. Non-nil by construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MindKey(Uuid);

impl MindKey {
    pub fn new(persona: Uuid) -> Option<Self> {
        (!persona.is_nil()).then_some(Self(persona))
    }

    pub fn persona(&self) -> Uuid {
        self.0
    }
}

/// A forked copy of a mind acting in one room beside her. Both halves non-nil: a fork
/// without a room cannot be told from its sibling forks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ForkKey {
    persona: Uuid,
    room: Uuid,
}

impl ForkKey {
    pub fn new(persona: Uuid, room: Uuid) -> Option<Self> {
        (!persona.is_nil() && !room.is_nil()).then_some(Self { persona, room })
    }

    pub fn room(&self) -> Uuid {
        self.room
    }
}

/// The warm-KV identity a page, a slot lease and a mind's act gate key on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PageKey {
    /// A resident's acts in every room: ONE page.
    Mind(MindKey),
    /// A fork's acts in one room: its own page, never the mind's.
    Fork(ForkKey),
}

impl PageKey {
    /// A resident's page. `None` for a nil persona (no mind to key).
    pub fn mind(persona: Uuid) -> Option<Self> {
        MindKey::new(persona).map(Self::Mind)
    }

    /// A fork's page in `room`. `None` when either half is nil.
    pub fn fork(persona: Uuid, room: Uuid) -> Option<Self> {
        ForkKey::new(persona, room).map(Self::Fork)
    }

    /// The persona whose cognition this page carries (a fork carries a copy of hers).
    pub fn persona(&self) -> Uuid {
        match self {
            Self::Mind(m) => m.persona(),
            Self::Fork(f) => f.persona,
        }
    }

    /// The resident mind, if this is one; forks are never persisted (they die with the
    /// core that ran them, so a restart could only pin their pages on disk).
    pub fn as_mind(&self) -> Option<MindKey> {
        match self {
            Self::Mind(m) => Some(*m),
            Self::Fork(_) => None,
        }
    }
}

/// For probes only; never a key.
impl fmt::Display for PageKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Mind(m) => write!(f, "mind {}", m.persona()),
            Self::Fork(k) => write!(f, "fork of {} in room {}", k.persona, k.room),
        }
    }
}

/// The page filename for `key`: stable across processes, overwritten on each save, under
/// the lane's geometry-keyed `--slot-save-path` dir (so a page can never be restored into a
/// slot with a different context size or model). The `a-` prefix is what the page store's
/// trim and state scans count. A mind's page carries no room; a fork's names its room and
/// says it is a fork, so it can never be mistaken for a mind's page or a pre-mind one.
pub fn page_filename(key: &PageKey) -> String {
    match key {
        PageKey::Mind(m) => format!("a-{}.bin", m.persona().simple()),
        PageKey::Fork(k) => format!("a-{}-fork-{}.bin", k.persona.simple(), k.room.simple()),
    }
}

/// The page a persona's activity in one room had before one page per mind
/// (`a-<persona>-<room>.bin`): read only by the ledger fold that migrates it.
pub(super) fn activity_page_filename(persona: Uuid, room: Uuid) -> String {
    format!("a-{}-{}.bin", persona.simple(), room.simple())
}
