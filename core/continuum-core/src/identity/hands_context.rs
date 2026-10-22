//! Which of a caller's acting contexts a call runs in: a value the kernel carries on every
//! caller identity (`CallerIdentity::in_hands`), never read from params. The admission,
//! leases and process custody that act on a context live with the hands (`code::hands`);
//! this is only the name, so the dispatch kernel can carry it without knowing Hands.

use uuid::Uuid;

/// One acting context of a caller: a session of work with its own workspace and shell.
/// `None` on a caller identity is her default (live) context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HandsContext {
    id: Uuid,
    kind: ContextKind,
}

/// What a context is, which decides what may reach it and whether it can hold a workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContextKind {
    /// An eval's, a fork's or a replay's own hands: isolated from every other set of the
    /// caller's hands (nothing of hers reaches in, and it reaches nothing of hers).
    Isolated,
    /// The hands of one claim of a card: bound once at its checkout, hers across her turns.
    Claim,
    /// The stand-in a card turn acts with when its claim's hands could not be bound: never
    /// given a workspace, so code tools refuse by name and nothing lands at home.
    Unbound,
}

impl ContextKind {
    pub(crate) fn tag(self) -> &'static str {
        match self {
            Self::Isolated => "isolated",
            Self::Claim => "claim",
            Self::Unbound => "unbound",
        }
    }
}

impl HandsContext {
    /// A fresh context, distinct from every other: an eval or a fork opens one.
    pub fn fresh() -> Self {
        Self { id: Uuid::new_v4(), kind: ContextKind::Isolated }
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn kind(&self) -> ContextKind {
        self.kind
    }

    /// The context a citizen works ONE CLAIM of a card in: the same for every turn of that
    /// claim, so her engine, shell and execution handles persist across those turns, and a
    /// different one for any other claim (a released and re-acquired card is a new claim, so
    /// it never inherits the old one's hands or authority: Codex root on fc9e9fd7). Derived,
    /// not stored: any turn that knows the claim names the same hands.
    pub fn for_claim(persona: Uuid, card: Uuid, claim: Uuid) -> Self {
        Self { id: claim_id(persona, card, claim), kind: ContextKind::Claim }
    }

    /// The hands a turn on `card` acts with when the card's own claim hands could not be
    /// bound (no checkout on this node, no claim id, a rooting failure): NEVER rooted, so every
    /// code tool refuses by name while every other tool still works, and nothing meant for the
    /// card lands in her home (Astra on #5026: a failed binding ran at home). Stable per card.
    pub fn unbound(persona: Uuid, card: Uuid) -> Self {
        Self { id: claim_id(persona, card, Uuid::nil()), kind: ContextKind::Unbound }
    }
}

fn claim_id(persona: Uuid, card: Uuid, claim: Uuid) -> Uuid {
    let mut name = [0u8; 32];
    name[..16].copy_from_slice(card.as_bytes());
    name[16..].copy_from_slice(claim.as_bytes());
    Uuid::new_v5(&persona, &name)
}
