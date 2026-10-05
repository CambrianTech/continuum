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

/// The SHORT form of an id: the leading 4 to [`SHORT_ID_LEN`] hex digits of a UUID, the
/// form every board shows and a citizen quotes back. A value, not a string: the digits as
/// a `u32` plus how many were given, so a prefix test is a shift and a compare.
///
/// THE one place an id is shortened or a short id is read (Joel, 2026-10-05: "Make the
/// short uuid a type … use the great facilities of rust for validation and efficient
/// conversion … in ONE PLACE"). Build one from a [`Uuid`] with `From`, parse one with
/// `FromStr`, show one with `Display`. Before this, 99 sites sliced `simple()[..8]` by
/// hand, and `work/review` demanded a full UUID that no board showed, so Kimi supplied an
/// invented one (`9a488f84-9683-…`, 2026-10-05 14:52Z) that matched nothing anywhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ShortId {
    /// The given digits, right-aligned (`0a5b` is 0x0a5b, not 0x0a5b0000).
    digits: u32,
    /// How many hex digits were given: MIN_PREFIX_HEX..=SHORT_ID_LEN.
    len: u8,
}

impl ShortId {
    /// Does `id` begin with these digits?
    pub fn prefixes(&self, id: &Uuid) -> bool {
        let head = u32::from_be_bytes(head_bytes(id));
        head >> (4 * (SHORT_ID_LEN as u32 - u32::from(self.len))) == self.digits
    }
}

/// The first four bytes of a UUID: the 8 hex digits its short form shows.
fn head_bytes(id: &Uuid) -> [u8; 4] {
    let b = id.as_bytes();
    [b[0], b[1], b[2], b[3]]
}

impl From<&Uuid> for ShortId {
    fn from(id: &Uuid) -> Self {
        Self { digits: u32::from_be_bytes(head_bytes(id)), len: SHORT_ID_LEN as u8 }
    }
}

impl std::str::FromStr for ShortId {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let t = s.trim();
        if !(MIN_PREFIX_HEX..=SHORT_ID_LEN).contains(&t.len()) {
            return Err(format!(
                "'{s}' is not a short id: {MIN_PREFIX_HEX} to {SHORT_ID_LEN} hex digits, the form a board shows"
            ));
        }
        let digits = u32::from_str_radix(t, 16)
            .map_err(|_| format!("'{s}' is not a short id: only hex digits (0-9, a-f)"))?;
        // the range check above bounds the length to 4..=8, which fits a u8
        Ok(Self { digits, len: t.len() as u8 })
    }
}

impl std::fmt::Display for ShortId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:0width$x}", self.digits, width = usize::from(self.len))
    }
}

/// A reference to something that already exists, as a citizen can write it: the short
/// form a board shows, or the full UUID. The parameter type of every verb that names an
/// existing card, claim or submission; validated when params are decoded, resolved against
/// the scoped candidates by [`IdRef::resolve`]. Ids a caller MINTS stay plain [`Uuid`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdRef {
    Full(Uuid),
    Short(ShortId),
}

impl IdRef {
    /// The one candidate this names. A full id that is not among non-empty candidates is
    /// tried by its short form (a model that pads a shown short id with an invented tail,
    /// or corrupts the middle, still meant its head); a miss lists what IS there.
    pub fn resolve(&self, candidates: &[Uuid], label: &str) -> Result<Uuid, String> {
        match self {
            Self::Full(id) if candidates.is_empty() || candidates.contains(id) => Ok(*id),
            Self::Full(id) => resolve_short(&ShortId::from(id), candidates, label).map_err(|miss| {
                format!("{label} {id} is not here, and neither is its short form: {miss}")
            }),
            Self::Short(short) => resolve_short(short, candidates, label),
        }
    }
}

impl From<Uuid> for IdRef {
    fn from(id: Uuid) -> Self {
        Self::Full(id)
    }
}

impl std::str::FromStr for IdRef {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let t = s.trim();
        if let Ok(id) = Uuid::parse_str(t) {
            return Ok(Self::Full(id));
        }
        // Strict: only hex digits and dashes are an id. `normalize` strips every other
        // character, which turned the WORD "placeholder" into the short id "acede".
        if t.is_empty() || !t.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
            return Err(format!(
                "'{s}' is not an id: give the short form a board shows (like 0a5b96b8) or the full UUID"
            ));
        }
        let hex: String = t.chars().filter(char::is_ascii_hexdigit).collect();
        if hex.len() == 32 {
            if let Ok(id) = Uuid::parse_str(&hex) {
                return Ok(Self::Full(id));
            }
        }
        // a short form as shown, or a mistyped UUID whose intact head is its short form
        let head: String = hex.chars().take(SHORT_ID_LEN).collect();
        head.parse().map(Self::Short)
    }
}

impl std::fmt::Display for IdRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Full(id) => write!(f, "{id}"),
            Self::Short(short) => write!(f, "{short}"),
        }
    }
}

impl serde::Serialize for IdRef {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> serde::Deserialize<'de> for IdRef {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        raw.parse().map_err(serde::de::Error::custom)
    }
}

impl schemars::JsonSchema for IdRef {
    fn schema_name() -> String {
        "IdRef".into()
    }
    fn json_schema(_gen: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        schemars::schema::SchemaObject {
            instance_type: Some(schemars::schema::InstanceType::String.into()),
            metadata: Some(Box::new(schemars::schema::Metadata {
                description: Some("An id as a board shows it (its short form, like 0a5b96b8) or the full UUID.".into()),
                ..Default::default()
            })),
            ..Default::default()
        }
        .into()
    }
}

/// The unique candidate a short id names, or a refusal that lists what is there.
fn resolve_short(short: &ShortId, candidates: &[Uuid], label: &str) -> Result<Uuid, String> {
    resolve_matching(&short.to_string(), candidates, label, |id| short.prefixes(id))
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
    resolve_prefix(&needle, candidates, label)
}

/// Resolve an execution handle without repairing malformed input. Unlike the
/// legacy tolerant ID path, retain every prefix digit so callers can disambiguate
/// collisions by supplying more characters. Candidates must come from the owner
/// and caller scope; this function does not grant access to the resolved object.
pub fn resolve_handle(s: &str, candidates: &[Uuid], label: &str) -> Result<Uuid, String> {
    let s = s.trim();
    if let Ok(id) = Uuid::parse_str(s) {
        return Ok(id);
    }
    if !(MIN_PREFIX_HEX..32).contains(&s.len()) || !s.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("'{s}' is not a usable {label} handle — give a full UUID or at least {MIN_PREFIX_HEX} leading hex characters"));
    }
    resolve_prefix(&s.to_ascii_lowercase(), candidates, label)
}

fn resolve_prefix(needle: &str, candidates: &[Uuid], label: &str) -> Result<Uuid, String> {
    resolve_matching(needle, candidates, label, |id| id.simple().to_string().starts_with(needle))
}

/// One candidate set, one match test, one set of refusals: the zero / one / many decision
/// every resolver shares, whatever decides a match.
fn resolve_matching(
    needle: &str,
    candidates: &[Uuid],
    label: &str,
    is_match: impl Fn(&Uuid) -> bool,
) -> Result<Uuid, String> {
    // Candidates are a SET: the same id folded from two boards (a card visible
    // from two subscribed rooms, 2026-09-05, #3722 review) is one card, never an
    // ambiguity between two.
    let mut matches: Vec<&Uuid> = candidates
        .iter()
        .filter(|id| is_match(id))
        .collect();
    matches.sort();
    matches.dedup();
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
                candidates.iter().map(|id| ShortId::from(id).to_string()).collect::<Vec<_>>().join(", ")
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

    // what this catches (Kimi, 2026-10-05 14:52Z): an id she could only have invented,
    // because the board showed 8 digits and the verb demanded 36. A short id round-trips
    // and tests by value; an invented tail on a real head resolves to the real card; a
    // full id on no board names what IS there; junk fails at decode, not later.
    #[test]
    fn a_short_id_is_a_value_and_an_invented_tail_still_names_the_card() {
        let card = u("0a5b96b8-545c-48d9-a0a6-11efdfd8a0be");
        let other = u("9bb24964-0000-4000-8000-000000000001");
        let shown = ShortId::from(&card);
        assert_eq!(shown.to_string(), "0a5b96b8");
        assert_eq!("0a5b96b8".parse::<ShortId>(), Ok(shown));
        assert!("0a5b".parse::<ShortId>().is_ok_and(|s| s.prefixes(&card) && !s.prefixes(&other)));
        assert_eq!("0A5B".parse::<ShortId>().map(|s| s.to_string()), Ok("0a5b".into()));
        assert!("0a5".parse::<ShortId>().is_err() && "0a5b96b8f".parse::<ShortId>().is_err());
        assert!("zzzz".parse::<ShortId>().is_err());

        let candidates = [card, other];
        assert_eq!("0a5b96b8".parse::<IdRef>().and_then(|r| r.resolve(&candidates, "card")), Ok(card));
        let padded: IdRef = "0a5b96b8-1111-4222-8333-444455556666".parse().expect("test: well-formed");
        assert_eq!(padded.resolve(&candidates, "card"), Ok(card), "an invented tail on a shown head");
        let invented: IdRef = "9a488f84-9683-4cec-923d-9530e4c6c5c5".parse().expect("test: well-formed");
        let miss = invented.resolve(&candidates, "card").expect_err("test: on no board");
        assert!(miss.contains("0a5b96b8") && miss.contains("9bb24964"), "lists what is there: {miss}");

        let decoded: Result<IdRef, _> = serde_json::from_value(serde_json::json!("placeholder"));
        assert!(decoded.is_err(), "a word is not an id, though 'acede' hides in it");
        assert_eq!("d7cfe47e0-8e39-41f5-bb2a-4e5d36e558e1".parse::<IdRef>().map(|r| r.to_string()), Ok("d7cfe47e".into()), "a mistyped UUID keeps its head");
        assert_eq!(serde_json::to_value(IdRef::Short(shown)).ok(), Some(serde_json::json!("0a5b96b8")));
    }

    // what this catches: an eight-digit collision must be resolvable by giving
    // more digits, and a cancellation handle must never repair malformed input.
    #[test]
    fn strict_handles_preserve_disambiguating_digits() {
        let a = u("12345678-1000-4000-8000-000000000001");
        let b = u("12345678-2000-4000-8000-000000000002");
        assert!(resolve_handle("12345678", &[a, b], "execution").unwrap_err().contains("ambiguous"));
        assert_eq!(resolve_handle("123456781", &[a, b], "execution"), Ok(a));
        assert_eq!(resolve_handle(&a.to_string(), &[a, b], "execution"), Ok(a));
        assert!(resolve_handle("12345678garbage", &[a], "execution").is_err());
        assert!(resolve_handle("12345678", &[], "execution").is_err());
    }

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
    /// what this catches: one id present twice in the candidates (the same card
    /// folded from two boards) must resolve, not read as an ambiguity between two.
    #[test]
    fn the_same_id_listed_twice_is_one_candidate_not_an_ambiguity() {
        let id = Uuid::new_v4();
        let prefix = id.simple().to_string()[..8].to_string();
        assert_eq!(resolve(&prefix, &[id, id], "card"), Ok(id));
    }
}
