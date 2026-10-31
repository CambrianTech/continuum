//! What each element she perceives follows from (MIND-CONTEXT-AND-ENGINE-CONFIG.md, "causality is
//! in the type"). Every element is one occurrence in a causal chain: an act and its result, a
//! message and its reply, a move and the position after it. The view keeps chains whole at their
//! heads, folds a chain without breaking it, and renders an element only with its cause
//! represented, so no source can present a repeat as progress or an effect without its cause.

use sha2::{Digest, Sha256};

/// One occurrence, named by the producer that persists it (an act, a message, an engram):
/// stable across epochs and restarts, never an in-view position. Two reads of one file are two
/// occurrences with two ids; the file they read is the item's `key`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct ElementId(u128);

impl ElementId {
    pub const fn from_u128(raw: u128) -> Self {
        Self(raw)
    }

    pub const fn as_u128(self) -> u128 {
        self.0
    }

    /// The id named by a producer's stable identity, given as its parts (whose, which producer,
    /// the producer's own id for it): the first 128 bits of their SHA-256. One derivation, so two
    /// producers never disagree about how an identity becomes an id. Each part is length-prefixed,
    /// so the boundary between parts is part of the identity: `["ab", "c"]` and `["a", "bc"]` are
    /// different occurrences (Cormac on #5611).
    pub fn of(identity: &[&[u8]]) -> Self {
        let mut hasher = Sha256::new();
        for part in identity {
            hasher.update((part.len() as u64).to_le_bytes());
            hasher.update(part);
        }
        let digest: [u8; 32] = hasher.finalize().into();
        let (head, _) = digest.split_at(std::mem::size_of::<u128>());
        let mut bytes = [0u8; std::mem::size_of::<u128>()];
        bytes.copy_from_slice(head);
        Self(u128::from_be_bytes(bytes))
    }
}

/// What an element follows from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Cause {
    /// The occurrence it follows from: her act for its result, the message a reply answers, the
    /// act before this one in her chain.
    Of(ElementId),
    /// A genuine origin, the only cause that is not another element. The source-hygiene ratchet
    /// counts each source's `Root` constructions: an origin claimed where a cause exists is how a
    /// chain breaks without anyone seeing it.
    Root(Origin),
    /// A record restored from before causes were kept: its cause was never written. Counted
    /// by the validator, never toward causal closure and never a repeat's ancestor; not
    /// `CauseNotHeld`, which promises a durable handle these never had (Cormac on #5607).
    Unrecorded,
}

/// The genuine origins.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Origin {
    /// From outside her: a person's message, a peer's event, a notice from the world.
    External,
    /// The start of an activity she entered: the first element of its chain.
    ActivityStart,
}

/// What is known of an element's whole: its hash, or nothing at all for a record restored from
/// before hashes were kept. Never a minted hash.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Content {
    Hashed(ContentHash),
    Absent,
}

/// The hash of an element's whole, computed once where the whole is held or stored, so a deferred
/// whole is compared without being read again.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ContentHash([u8; 32]);

impl ContentHash {
    /// The hash of a whole's text.
    pub fn of(whole: &str) -> Self {
        Self(Sha256::digest(whole.as_bytes()).into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: two occurrences with the same output compared as one effect. Content
    // equality is evidence only for an unchanged observation; the occurrences stay distinct
    // (Codex on #5607: two appends can both return "OK").
    #[test]
    fn equal_content_hashes_equal_and_distinct_occurrences_stay_distinct() {
        assert_eq!(ContentHash::of("fn a() {}"), ContentHash::of("fn a() {}"));
        assert_ne!(ContentHash::of("fn a() {}"), ContentHash::of("fn b() {}"));
        assert_ne!(ElementId::from_u128(1), ElementId::from_u128(2));
    }

    // what this catches: two different occurrences sharing an id because their identity parts
    // concatenate to the same bytes; every chain is keyed by the id (Cormac on #5611).
    #[test]
    fn the_boundary_between_identity_parts_is_part_of_the_id() {
        assert_ne!(ElementId::of(&[b"ab", b"c"]), ElementId::of(&[b"a", b"bc"]));
        assert_ne!(ElementId::of(&[b"ab"]), ElementId::of(&[b"a", b"b"]));
        assert_eq!(ElementId::of(&[b"a", b"bc"]), ElementId::of(&[b"a", b"bc"]));
    }
}
