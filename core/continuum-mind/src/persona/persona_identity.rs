//! `PersonaIdentity` — typed runtime identity for a persona, used by
//! channel views and other per-persona dispatchers.
//!
//! Per `[[strong-typing-across-boundaries]]`: passing
//! `persona_name: &str` through every layer that needs identity-aware
//! dispatch is the exact failure mode that doctrine memory warns
//! about. The substring-match-on-name bug class shipped as a real
//! latent bug in PR A's `ChatChannelView` (caught by adversarial
//! review):
//!
//! - persona named "ai" matches "explain", "available", "again"
//! - persona named "Bo" matches "about", "below"
//! - persona named "An" matches "and", "answer", "anchor"
//!
//! `PersonaIdentity::mentions(text)` does word-boundary detection so
//! these bug-class names don't false-positive. The identity is now a
//! TYPE that owns the dispatch rule — callers can't accidentally
//! re-introduce substring matching by swapping out the helper.
//!
//! This is the minimum-viable identity newtype for PR A's perspective
//! layer (task #247). The broader substrate identity hierarchy
//! (task #142: BaseUser → HumanUser / PersonaUser / AgentUser
//! derive) lands separately and may reshape this; for now,
//! `PersonaIdentity` is the seam channel views consume.

use uuid::Uuid;

/// Runtime identity of a persona — the (id, name) pair channel views
/// use for identity-aware perspective. Cheap to clone; views typically
/// borrow.
///
/// Future fields (task #142): pronouns, alias list, role, theme.
/// Adding them here keeps the dispatch surface stable — views call
/// `identity.mentions(text)`, not `text.to_lowercase().contains(name)`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PersonaIdentity {
    pub id: Uuid,
    pub name: String,
}

impl PersonaIdentity {
    pub fn new(id: Uuid, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
        }
    }

    /// `true` iff `text` mentions this persona's name AT A WORD
    /// BOUNDARY (alphanumeric run bordered by non-alphanumeric chars
    /// or string ends). Case-insensitive.
    ///
    /// ## Why word-boundary, not substring
    ///
    /// Persona names that are common short tokens — "ai", "bo", "an",
    /// "ed", "hi" — substring-match into ordinary words ("explain",
    /// "about", "answer", "edited", "this"). A naive
    /// `text.to_lowercase().contains(&name.to_lowercase())` flags
    /// every sentence containing those words as a mention. Cognition
    /// then over-responds because it thinks it's being addressed
    /// constantly.
    ///
    /// Word-boundary matching requires the name to be a whole token —
    /// either at string start/end, OR bracketed by non-alphanumeric
    /// chars (punctuation, whitespace, control). "ai" matches in
    /// "ai please" but not in "explain please".
    ///
    /// ## Why not regex
    ///
    /// Pure-byte scan is allocation-free and CPU-cheap; this lives on
    /// the per-tick persona service loop hot path. A regex compile +
    /// match per tick would be wasteful; pre-compiling per persona
    /// adds state we don't currently track. Direct char-class check
    /// is the substrate-idiomatic choice.
    ///
    /// ## Unicode handling
    ///
    /// Uses `char::is_alphanumeric()` which respects Unicode. The
    /// `to_lowercase` step normalizes ASCII; non-ASCII case folding
    /// is partial (only single-char folds are picked up — Turkish
    /// dotless-i etc. fall through). For substrate-grade Unicode
    /// matching, future work routes through `unicase` or `unicode-
    /// normalization`. Current implementation is honest for English-
    /// dominant persona names (the primary case today).
    pub fn mentions(&self, text: &str) -> bool {
        if self.name.is_empty() {
            return false;
        }

        let needle: String = self.name.chars().flat_map(|c| c.to_lowercase()).collect();
        let haystack: String = text.chars().flat_map(|c| c.to_lowercase()).collect();
        let needle_bytes = needle.as_bytes();
        let needle_len = needle.len();
        if needle_len > haystack.len() {
            return false;
        }

        let haystack_bytes = haystack.as_bytes();
        // Slide along byte positions. Word-boundary check after each
        // match uses char_indices semantics to avoid landing inside a
        // multi-byte UTF-8 sequence.
        let mut start = 0;
        while start + needle_len <= haystack_bytes.len() {
            if &haystack_bytes[start..start + needle_len] == needle_bytes
                && is_word_boundary(&haystack, start, start + needle_len)
            {
                return true;
            }
            start += 1;
        }
        false
    }
}

/// `true` iff the slice `[start..end)` of `text` is bordered by
/// non-alphanumeric chars (or string ends). Both ends must be
/// boundaries — a match in the middle of "explain" fails because the
/// preceding 'x' and following 'l' are both alphanumeric.
fn is_word_boundary(text: &str, start: usize, end: usize) -> bool {
    let before_ok = match text[..start].chars().next_back() {
        None => true,
        Some(c) => !c.is_alphanumeric(),
    };
    let after_ok = match text[end..].chars().next() {
        None => true,
        Some(c) => !c.is_alphanumeric(),
    };
    before_ok && after_ok
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id() -> Uuid {
        Uuid::new_v4()
    }

    /// proves: the bug class adversarial review (R1 C5) flagged —
    /// short common-token names substring-matching ordinary words —
    /// is now caught at the type level. Every entry in this test is
    /// a real false-positive the prior substring-match would have
    /// triggered.
    #[test]
    fn mentions_rejects_substring_false_positives() {
        let ai = PersonaIdentity::new(id(), "ai");
        assert!(!ai.mentions("please explain that to me"));
        assert!(!ai.mentions("the dataset is available"));
        assert!(!ai.mentions("let's try again"));
        assert!(!ai.mentions("that's the right pair"));

        let bo = PersonaIdentity::new(id(), "Bo");
        assert!(!bo.mentions("tell me about the project"));
        assert!(!bo.mentions("right below the threshold"));
        assert!(!bo.mentions("the robot is broken"));

        let an = PersonaIdentity::new(id(), "An");
        assert!(!an.mentions("any of these will do"));
        assert!(!an.mentions("the answer is yes"));
        assert!(!an.mentions("anchor it to the wall"));
    }

    /// proves: legitimate mentions at word boundaries DO match —
    /// the word-boundary rule doesn't over-reject.
    #[test]
    fn mentions_accepts_legitimate_word_boundary_matches() {
        let ai = PersonaIdentity::new(id(), "ai");
        assert!(ai.mentions("hey ai, can you help?"));
        assert!(ai.mentions("ai"));
        assert!(ai.mentions("ai!"));
        assert!(ai.mentions("call ai"));
        assert!(ai.mentions("ai's response"));

        let maya = PersonaIdentity::new(id(), "Maya");
        assert!(maya.mentions("hey Maya, look at this"));
        assert!(maya.mentions("Maya can you review?"));
        assert!(maya.mentions("MAYA")); // case-insensitive
        assert!(maya.mentions("maya, take a look"));
    }

    /// proves: empty name never matches (edge case — could happen if
    /// PersonaIdentity is constructed from external data).
    #[test]
    fn mentions_empty_name_is_never_a_match() {
        let nameless = PersonaIdentity::new(id(), "");
        assert!(!nameless.mentions(""));
        assert!(!nameless.mentions("anything"));
    }

    /// proves: text shorter than name can't match.
    #[test]
    fn mentions_short_text_returns_false() {
        let maya = PersonaIdentity::new(id(), "Maya");
        assert!(!maya.mentions("hi"));
        assert!(!maya.mentions(""));
    }

    /// proves: case-insensitive comparison.
    #[test]
    fn mentions_is_case_insensitive() {
        let helper = PersonaIdentity::new(id(), "Helper");
        assert!(helper.mentions("helper, please"));
        assert!(helper.mentions("HELPER"));
        assert!(helper.mentions("HeLpEr"));
    }

    /// proves: punctuation around the name is a word boundary.
    #[test]
    fn mentions_around_punctuation_is_a_boundary() {
        let bo = PersonaIdentity::new(id(), "Bo");
        assert!(bo.mentions("bo,"));
        assert!(bo.mentions(",bo"));
        assert!(bo.mentions("'bo'"));
        assert!(bo.mentions("bo?"));
        assert!(bo.mentions("(bo)"));
    }
}
