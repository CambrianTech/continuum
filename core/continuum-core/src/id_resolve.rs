//! `id_resolve` — the ONE place a human-typed / model-emitted id string becomes a
//! canonical [`Uuid`], tolerant of the two ways that string arrives imperfect.
//!
//! ## Why this exists
//!
//! Every surface DISPLAYS ids as 8-char short forms (`card 08ece9e8`, `persona
//! 90e758b2`) — that's what a persona SEES. But the verbs that consume ids demanded
//! the full 32-char UUID, so a persona quoting the id it was shown bounced. Worse,
//! a model reliably CORRUPTS a full UUID by adding/dropping ONE character mid-string
//! (glass-boxed 2026-07-13: 28% of live `work/claim` calls — `d7cfe47e0-8e39-…` with
//! a 9-char first group; a 33-hex variant). Joel: "short form uuids ought to work
//! too" — what a surface displays, its verbs must accept.
//!
//! The fix is NOT per-command id parsing (it drifts — `work/claim` learned this and
//! every other id-taking verb would have to re-learn it). It is ONE normalization
//! primitive ([`normalize`]) + ONE prefix-resolution against a candidate set
//! ([`resolve`]). The candidate set is the ONLY per-id-type knowledge — cards come
//! from the work board, personas from the live registry, rooms from the room list —
//! so a caller supplies the candidates and inherits the tolerance for free. This is
//! the registry-agnostic half; the per-type candidate lookup is the caller's.
//!
//! Lifted from `work.rs::card_id_lookup` (the proven outlier) and generalized.
//! [[px-persona-experience-tools-as-good-ux]], the E=mc² compression rule: one
//! logical decision, one place.

use uuid::Uuid;

/// How a raw id string should be looked up: a clean full UUID, a leading short-id
/// prefix to resolve against a candidate set, or genuinely unusable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdMatch {
    /// A clean, fully-parseable UUID (dashed or 32-char simple) — use directly.
    Full(Uuid),
    /// A leading hex prefix (the board's short-id width) to resolve against the
    /// candidate set. Always lowercase, at most 8 chars.
    Prefix(String),
    /// Fewer than 4 hex digits — nothing to disambiguate; fail loud upstream.
    Invalid,
}

/// The board's displayed short-id width — the number of leading hex chars we key a
/// prefix resolution on. Capping the needle here (rather than using the whole hex
/// run) means a UUID corrupted in the MIDDLE still resolves on its intact head.
pub const SHORT_ID_LEN: usize = 8;

/// Minimum hex digits worth treating as a prefix — below this there's nothing to
/// disambiguate against a candidate set.
const MIN_PREFIX_HEX: usize = 4;

/// How many candidate short-ids to enumerate inline in a zero-match error. Small
/// enough to stay readable in a room turn; the live id-typed sets (work board,
/// live personas, rooms) are all well under this. Larger sets get a count instead
/// of a wall of ids.
const MAX_LISTED_CANDIDATES: usize = 16;

/// The board's displayed short form of a canonical id — the leading [`SHORT_ID_LEN`]
/// hex chars, exactly what every surface shows and what a persona should quote back.
fn short_form(id: &Uuid) -> String {
    id.simple().to_string().chars().take(SHORT_ID_LEN).collect()
}

/// Classify a raw id string — the PURE, registry-free decision (unit-testable
/// without any candidate set). Handles the three live shapes:
///  1. a clean UUID (dashed or 32-char simple) → [`IdMatch::Full`];
///  2. a mistyped-length near-UUID → strip separators; a clean 32-hex run is a full
///     id, otherwise its leading [`SHORT_ID_LEN`] hex chars are a prefix to resolve;
///  3. under [`MIN_PREFIX_HEX`] hex digits → [`IdMatch::Invalid`].
pub fn normalize(s: &str) -> IdMatch {
    let s = s.trim();
    if let Ok(id) = Uuid::parse_str(s) {
        return IdMatch::Full(id);
    }
    let hex: String = s.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if hex.len() == 32 {
        if let Ok(id) = Uuid::parse_str(&hex) {
            return IdMatch::Full(id);
        }
    }
    if hex.len() < MIN_PREFIX_HEX {
        return IdMatch::Invalid;
    }
    IdMatch::Prefix(
        hex.chars()
            .take(SHORT_ID_LEN)
            .collect::<String>()
            .to_ascii_lowercase(),
    )
}

/// Resolve a raw id string to a canonical [`Uuid`] against a candidate set — the
/// full contract every id-taking verb wants. A clean UUID passes straight through
/// (candidates unused). A prefix expands to the UNIQUE candidate whose simple-hex
/// form starts with it; zero matches or ambiguity fail loud with what WAS found so
/// the caller (often a model) can correct. `label` names the id kind in the error
/// (`"card"`, `"persona"`, `"room"`) — teaching, not a silent miss.
pub fn resolve(s: &str, candidates: &[Uuid], label: &str) -> Result<Uuid, String> {
    let needle = match normalize(s) {
        IdMatch::Full(id) => return Ok(id),
        IdMatch::Prefix(p) => p,
        IdMatch::Invalid => {
            return Err(format!(
                "'{s}' is not a usable {label} id — give the full id or its leading \
                 short form (at least {MIN_PREFIX_HEX} hex characters)"
            ))
        }
    };
    let matches: Vec<&Uuid> = candidates
        .iter()
        .filter(|id| id.simple().to_string().starts_with(&needle))
        .collect();
    match matches.as_slice() {
        [one] => Ok(**one),
        // Zero match — the live failure mode (2026-07-13: a persona claimed a
        // FABRICATED id and a peer had to hand it the right one). "Check the id you
        // were shown" isn't actionable when the persona never held a real id; ENUMERATE
        // the valid short forms inline so the miss self-corrects on the next turn
        // instead of stalling on peer coaching. [[px-persona-experience-tools-as-good-ux]]
        [] => Err(match candidates.len() {
            0 => format!("no {label}s exist to match id prefix '{needle}' — there are none to choose from right now"),
            n if n <= MAX_LISTED_CANDIDATES => format!(
                "no {label} matches id prefix '{needle}' — available {label} ids: {}",
                candidates.iter().map(short_form).collect::<Vec<_>>().join(", ")
            ),
            n => format!(
                "no {label} matches id prefix '{needle}' among {n} {label}s — check the id you were shown"
            ),
        }),
        many => Err(format!(
            "{label} id prefix '{needle}' is ambiguous ({} match) — give more characters",
            many.len()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(s: &str) -> Uuid {
        Uuid::parse_str(s).unwrap()
    }

    // what this catches: the normalization decision (#161/#164) — a clean UUID is
    // Full, a mistyped-length near-UUID rescues to its intact leading-8 short id,
    // and sub-prefix junk is Invalid. This is the registry-free half lifted from
    // work.rs::card_id_lookup and shared across every id-taking verb.
    #[test]
    fn normalize_classifies_clean_mistyped_and_junk() {
        assert!(matches!(
            normalize("d7cfe47e-8e39-41f5-bb2a-4e5d36e558e1"),
            IdMatch::Full(_)
        ));
        assert!(matches!(
            normalize("d7cfe47e8e3941f5bb2a4e5d36e558e1"),
            IdMatch::Full(_)
        ));
        // the exact live corruptions → intact leading-8 prefix
        assert_eq!(
            normalize("d7cfe47e0-8e39-41f5-bb2a-4e5d36e558e1"),
            IdMatch::Prefix("d7cfe47e".into())
        );
        assert_eq!(
            normalize("d7cfe47e08e3941f5bb2a4e5d36e558e1"),
            IdMatch::Prefix("d7cfe47e".into())
        );
        // the board's short form, verbatim
        assert_eq!(normalize("08ece9e8"), IdMatch::Prefix("08ece9e8".into()));
        assert_eq!(normalize("xyz"), IdMatch::Invalid);
        assert_eq!(normalize(""), IdMatch::Invalid);
    }

    // what this catches: prefix resolution against a candidate set — unique match
    // wins, a full UUID passes through untouched, zero/ambiguous fail LOUD naming
    // the id kind (teaching, never a silent miss). This is the one contract every
    // id-taking verb reuses; the candidate set is the only per-type knowledge.
    #[test]
    fn resolve_expands_prefix_uniquely_and_fails_loud() {
        let a = u("90e758b2-3cf3-45c1-b100-de7c4ab5a549");
        let b = u("fe4dac17-f62d-4cda-bb66-73da30ac7e15");
        let cands = [a, b];
        // full uuid passes through (candidates irrelevant)
        assert_eq!(resolve(&a.to_string(), &[], "persona").unwrap(), a);
        // unique short prefix resolves
        assert_eq!(resolve("90e758b2", &cands, "persona").unwrap(), a);
        // mistyped-length near-uuid rescues via leading-8
        assert_eq!(resolve("fe4dac170-f62d", &cands, "persona").unwrap(), b);
        // zero match → loud, names the kind AND enumerates the valid short forms so a
        // persona that fabricated an id (the 2026-07-13 live failure) self-corrects on
        // the next turn instead of waiting for a peer to hand it the right id.
        let e = resolve("deadbeef", &cands, "persona").unwrap_err();
        assert!(e.contains("persona") && e.contains("no "), "teaches: {e}");
        assert!(
            e.contains("90e758b2") && e.contains("fe4dac17"),
            "lists the valid ids: {e}"
        );
        // ambiguous → loud
        let c = u("90e70000-0000-0000-0000-000000000000");
        let e = resolve("90e7", &[a, c], "persona").unwrap_err();
        assert!(e.contains("ambiguous"), "teaches: {e}");
        // junk → loud
        assert!(resolve("!!", &cands, "persona").is_err());
    }

    // what this catches: the zero-match error scales sanely with the candidate set —
    // an empty set says so plainly, a small set lists every valid short form (the
    // self-correction path), and a set past the cap gives a count instead of a wall of
    // ids. This is what turns "check the id you were shown" (dead end) into an
    // actionable next move for a model that mis-emitted an id.
    #[test]
    fn zero_match_error_scales_with_candidate_count() {
        // empty set → plainly says there are none
        let e = resolve("deadbeef", &[], "card").unwrap_err();
        assert!(
            e.contains("no card") && e.contains("none to choose"),
            "empty: {e}"
        );

        // small set (<= cap) → enumerates the short forms
        let cands: Vec<Uuid> = (0..3)
            .map(|i| u(&format!("0000000{i}-0000-0000-0000-000000000000")))
            .collect();
        let e = resolve("deadbeef", &cands, "card").unwrap_err();
        assert!(e.contains("available card ids"), "lists: {e}");
        assert!(
            e.contains("00000000") && e.contains("00000002"),
            "each short form present: {e}"
        );

        // past the cap → a count, not a wall of ids
        let many: Vec<Uuid> = (0..(MAX_LISTED_CANDIDATES + 5))
            .map(|_| Uuid::new_v4())
            .collect();
        let e = resolve("ffffffff", &many, "card").unwrap_err();
        assert!(
            e.contains(&format!("among {} card", many.len())) && !e.contains("available card ids"),
            "counts instead of listing: {e}"
        );
    }
}
