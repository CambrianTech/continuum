//! Engram redaction — the surgical complement to `forget_context`.
//!
//! `AdmissionState::forget_context` is the BLUNT tool: it neuralyzes a whole
//! episode (drops every engram tagged with one `context_id`). Redaction is the
//! SURGICAL one: it keeps the memory — the lived experience of having been asked
//! and having answered — and excises only a policy-defined *class* of content
//! from the engram's text.
//!
//! Three concerns, ONE layer (per the compression principle — one decision, one
//! place):
//!
//! - **Exam integrity** — scrub a held-out benchmark's literal answer key out of
//!   a persona's engrams so she can never *memorize* the answer, while keeping
//!   the experience of having struggled with the question. This is what makes a
//!   benchmark a legitimate proctored exam of a continuously-learning mind: she
//!   keeps her whole autobiography of getting better; she just never holds the
//!   crib sheet ([[benchmarks-are-proctored-exams-of-the-natural-living-persona]]).
//! - **Secrets hygiene** — scrub an API key / credential that leaked into a
//!   memory so it isn't recallable or trainable.
//! - **PII on share** — scrub personally-identifying content when a persona's
//!   engram bundle is exported/shared.
//!
//! ## Shape (OpenCV `cv::Algorithm`)
//!
//! A [`RedactionDetector`] finds byte-spans of ONE concern in text. A
//! [`RedactionPolicy`] is an ordered set of detectors; `redact()` applies them
//! all, resolves overlaps, and rewrites each matched span to a
//! `[redacted:<class>]` placeholder — leaving everything *not* matched intact.
//! Detectors are polymorphic: adding a new concern is one new `impl`, not a
//! change to the redactor. Two shipped here prove the interface across its
//! extremes — a stateless shape-matcher (`SecretDetector`) and a stateful
//! corpus-matcher (`ExamKeyDetector`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A class of sensitive content a detector matches. The wire enum the
/// `cognition/redact-memory` command selects over.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/persona/RedactionClass.ts")]
pub enum RedactionClass {
    /// A credential / API key / access token.
    Secret,
    /// Personally-identifying information (persona-share export hygiene).
    Pii,
    /// A held-out benchmark answer key (proctored-exam integrity).
    ExamKey,
}

impl RedactionClass {
    /// The placeholder written in place of a redacted span. Structural (names
    /// the class that was removed) so the memory stays honest — recall sees
    /// "there was an exam key here, now excised", never a silent gap.
    pub fn placeholder(self) -> &'static str {
        match self {
            RedactionClass::Secret => "[redacted:secret]",
            RedactionClass::Pii => "[redacted:pii]",
            RedactionClass::ExamKey => "[redacted:exam-key]",
        }
    }
}

/// One matched span within a piece of text: the half-open byte range
/// `[start, end)` and what class of content it is. Byte offsets MUST fall on
/// UTF-8 char boundaries (the redactor re-verifies and skips any that don't).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedactionSpan {
    pub start: usize,
    pub end: usize,
    pub class: RedactionClass,
}

/// Polymorphic detector: finds spans of exactly ONE concern in text.
///
/// Impls return spans in the ORIGINAL text's byte offsets. They need not sort
/// or de-overlap — [`RedactionPolicy::redact`] resolves overlaps across all
/// detectors globally.
pub trait RedactionDetector: Send + Sync {
    /// The single class this detector emits.
    fn class(&self) -> RedactionClass;
    /// Find all spans of `self.class()` in `text`.
    fn detect(&self, text: &str) -> Vec<RedactionSpan>;
}

/// What a redaction pass removed, per class. Empty = nothing matched (the text
/// is returned untouched).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RedactionReport {
    counts: BTreeMap<RedactionClass, usize>,
}

impl RedactionReport {
    /// How many spans of `class` were redacted.
    pub fn count(&self, class: RedactionClass) -> usize {
        self.counts.get(&class).copied().unwrap_or(0)
    }

    /// Total spans redacted across all classes.
    pub fn total(&self) -> usize {
        self.counts.values().sum()
    }

    /// True if nothing was redacted (the common, hot case).
    pub fn is_empty(&self) -> bool {
        self.total() == 0
    }

    fn record(&mut self, class: RedactionClass) {
        *self.counts.entry(class).or_insert(0) += 1;
    }

    /// Fold another report into this one (aggregating across many engrams).
    pub fn merge(&mut self, other: &RedactionReport) {
        for (class, n) in &other.counts {
            *self.counts.entry(*class).or_insert(0) += n;
        }
    }
}

/// An ordered set of detectors. `redact()` applies them all and rewrites the
/// text, keeping everything not matched.
pub struct RedactionPolicy {
    detectors: Vec<Box<dyn RedactionDetector>>,
}

impl RedactionPolicy {
    pub fn new(detectors: Vec<Box<dyn RedactionDetector>>) -> Self {
        Self { detectors }
    }

    /// True if this policy has no detectors — redaction is a guaranteed no-op,
    /// so callers can skip the whole store walk.
    pub fn is_noop(&self) -> bool {
        self.detectors.is_empty()
    }

    /// Redact `text`, returning the rewritten string and a report of what was
    /// removed. Every byte NOT inside a matched span is preserved verbatim —
    /// this is the "keep the experience, excise only the key" guarantee.
    ///
    /// Overlap resolution: all detectors' spans are pooled, sorted by start
    /// (longer span wins a tie), then accepted greedily left-to-right so no two
    /// redactions overlap. A span that isn't on a char boundary is dropped
    /// rather than risk splitting a UTF-8 sequence.
    pub fn redact(&self, text: &str) -> (String, RedactionReport) {
        let mut spans: Vec<RedactionSpan> = Vec::new();
        for detector in &self.detectors {
            spans.extend(detector.detect(text));
        }
        if spans.is_empty() {
            return (text.to_string(), RedactionReport::default());
        }

        // Earliest start first; on a tie the longer span wins (more content
        // removed is the safer redaction).
        spans.sort_by(|a, b| a.start.cmp(&b.start).then(b.end.cmp(&a.end)));

        let mut out = String::with_capacity(text.len());
        let mut report = RedactionReport::default();
        let mut cursor = 0usize; // next unwritten byte in `text`
        for span in spans {
            // Skip spans that overlap an already-accepted one, are malformed,
            // or would split a UTF-8 char.
            if span.start < cursor
                || span.start >= span.end
                || span.end > text.len()
                || !text.is_char_boundary(span.start)
                || !text.is_char_boundary(span.end)
            {
                continue;
            }
            out.push_str(&text[cursor..span.start]);
            out.push_str(span.class.placeholder());
            report.record(span.class);
            cursor = span.end;
        }
        out.push_str(&text[cursor..]);
        (out, report)
    }
}

//=============================================================================
// OUTLIER A — SecretDetector (stateless, shape-based)
//=============================================================================

/// Detects credential/API-key material by SHAPE — no external corpus, pure
/// structure. The stateless extreme of the detector interface.
///
/// A run of credential-ish characters (`[A-Za-z0-9_-]`) is flagged if it either
/// (a) starts with a known credential prefix (signature-style, like an AV
/// signature — a small, honest, extensible list), or (b) is long AND mixes
/// letters with digits (the high-entropy shape a random secret has and prose
/// does not). Ordinary words fail both tests, so English text is left alone.
pub struct SecretDetector {
    /// Minimum length for the entropy-shape rule (prefix hits bypass this).
    min_entropy_len: usize,
}

/// Known credential prefixes. Signature list — extend as new key formats
/// appear; a prefix hit flags the whole token regardless of length.
const SECRET_PREFIXES: &[&str] = &[
    "sk-",    // OpenAI / Anthropic-style
    "ghp_",   // GitHub personal token
    "gho_",   // GitHub OAuth token
    "ghs_",   // GitHub server token
    "github_pat_",
    "xoxb-",  // Slack bot
    "xoxp-",  // Slack user
    "AKIA",   // AWS access key id
    "ASIA",   // AWS temp access key id
    "AIza",   // Google API key
];

impl Default for SecretDetector {
    fn default() -> Self {
        Self { min_entropy_len: 32 }
    }
}

impl SecretDetector {
    pub fn new() -> Self {
        Self::default()
    }

    fn is_token_char(b: u8) -> bool {
        b.is_ascii_alphanumeric() || b == b'_' || b == b'-'
    }

    fn looks_like_secret(&self, token: &str) -> bool {
        for prefix in SECRET_PREFIXES {
            if token.len() > prefix.len() && token.starts_with(prefix) {
                return true;
            }
        }
        if token.len() >= self.min_entropy_len {
            let has_alpha = token.bytes().any(|b| b.is_ascii_alphabetic());
            let has_digit = token.bytes().any(|b| b.is_ascii_digit());
            if has_alpha && has_digit {
                return true;
            }
        }
        false
    }
}

impl RedactionDetector for SecretDetector {
    fn class(&self) -> RedactionClass {
        RedactionClass::Secret
    }

    fn detect(&self, text: &str) -> Vec<RedactionSpan> {
        let bytes = text.as_bytes();
        let mut spans = Vec::new();
        let mut i = 0usize;
        while i < bytes.len() {
            if Self::is_token_char(bytes[i]) {
                let start = i;
                while i < bytes.len() && Self::is_token_char(bytes[i]) {
                    i += 1;
                }
                // `[start, i)` is ASCII token bytes → always a char boundary.
                let token = &text[start..i];
                if self.looks_like_secret(token) {
                    spans.push(RedactionSpan {
                        start,
                        end: i,
                        class: RedactionClass::Secret,
                    });
                }
            } else {
                i += 1;
            }
        }
        spans
    }
}

//=============================================================================
// OUTLIER B — ExamKeyDetector (stateful, corpus-based)
//=============================================================================

/// Detects occurrences of a HELD-OUT answer key inside text. The stateful
/// extreme: it carries a loaded corpus of literal answers and finds them, rather
/// than matching a shape. This is what keeps a persona from memorizing a
/// benchmark's answers across retakes while leaving her *experience* of the
/// exam intact.
///
/// Matching is ASCII-case-insensitive exact-substring (benchmark answers are
/// code/identifiers/numbers — `service_loop.rs`, `3000` — so `eq_ignore_ascii_case`
/// over byte windows gives exact original offsets with no Unicode-fold hazard).
/// Answers shorter than `min_len` are dropped at construction: redacting a
/// 1–2 char answer everywhere it appears would shred unrelated memory, so those
/// are left to the durable `forget_context` episode-drop instead.
pub struct ExamKeyDetector {
    /// Held-out answers, pre-lowercased ASCII bytes, longest-first so the
    /// greedy overlap pass prefers the most specific match.
    answers: Vec<Vec<u8>>,
    min_len: usize,
}

impl ExamKeyDetector {
    /// Minimum answer length to be redactable. Below this, an answer is too
    /// generic to scrub safely (`forget_context` covers those episodes).
    pub const DEFAULT_MIN_LEN: usize = 4;

    /// Build from raw held-out answer strings (e.g. every `EvalTask.expect`).
    /// Answers below `min_len` or non-ASCII are skipped (logged by the caller,
    /// not silently — see the command). Deduplicated and sorted longest-first.
    pub fn new(answers: impl IntoIterator<Item = String>, min_len: usize) -> Self {
        let mut prepared: Vec<Vec<u8>> = answers
            .into_iter()
            .map(|a| a.trim().to_string())
            .filter(|a| a.len() >= min_len && a.is_ascii())
            .map(|a| a.to_ascii_lowercase().into_bytes())
            .collect();
        prepared.sort();
        prepared.dedup();
        // Longest-first: a more specific answer redacts before a shorter one it
        // may contain, and the policy's tie-break keeps the longer span.
        prepared.sort_by(|a, b| b.len().cmp(&a.len()));
        Self { answers: prepared, min_len }
    }

    /// Number of loaded answers (post-filter). Zero → this detector is inert,
    /// which the caller should treat as a fail-loud signal (empty held-out set).
    pub fn answer_count(&self) -> usize {
        self.answers.len()
    }

    pub fn min_len(&self) -> usize {
        self.min_len
    }
}

impl RedactionDetector for ExamKeyDetector {
    fn class(&self) -> RedactionClass {
        RedactionClass::ExamKey
    }

    fn detect(&self, text: &str) -> Vec<RedactionSpan> {
        let hay = text.as_bytes();
        let mut spans = Vec::new();
        for needle in &self.answers {
            if needle.len() > hay.len() {
                continue;
            }
            let mut i = 0usize;
            while i + needle.len() <= hay.len() {
                if hay[i..i + needle.len()].eq_ignore_ascii_case(needle) {
                    spans.push(RedactionSpan {
                        start: i,
                        end: i + needle.len(),
                        class: RedactionClass::ExamKey,
                    });
                    i += needle.len();
                } else {
                    i += 1;
                }
            }
        }
        spans
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── The interface proof: both extremes fit RedactionDetector cleanly ──

    // what this catches: SecretDetector (outlier A) flags key-shaped tokens by
    // prefix and by entropy-shape, and leaves ordinary prose untouched.
    #[test]
    fn secret_detector_flags_keys_not_prose() {
        let d = SecretDetector::new();
        // Known prefix (short) still flagged.
        assert_eq!(d.detect("token sk-abc123XYZ here").len(), 1);
        // Long mixed alnum (no prefix) flagged by entropy shape.
        assert_eq!(d.detect("val AbCdEf0123456789AbCdEf0123456789 end").len(), 1);
        // Ordinary English: no letters+digits mix, short → nothing.
        assert!(d.detect("the quick brown fox jumps over the lazy dog").is_empty());
        // A long all-alpha word (no digit) is not entropy-flagged.
        assert!(d.detect("supercalifragilisticexpialidociousandthensome").is_empty());
    }

    // what this catches: ExamKeyDetector (outlier B) finds held-out answers
    // case-insensitively at their exact byte offsets, and drops too-short ones.
    #[test]
    fn exam_key_detector_finds_answers_case_insensitively() {
        let d = ExamKeyDetector::new(
            ["service_loop.rs".to_string(), "3000".to_string(), "x".to_string()],
            ExamKeyDetector::DEFAULT_MIN_LEN,
        );
        // "x" is below min_len → not loaded.
        assert_eq!(d.answer_count(), 2);
        let spans = d.detect("edit Service_Loop.RS and set port 3000");
        assert_eq!(spans.len(), 2);
        assert!(spans.iter().all(|s| s.class == RedactionClass::ExamKey));
    }

    // ── The core guarantee: keep the experience, excise only the key ──

    // what this catches: redaction removes ONLY the matched answer span and
    // preserves every other byte — the "she keeps the memory of answering, just
    // not the crib sheet" invariant.
    #[test]
    fn redact_keeps_experience_and_excises_only_the_key() {
        let policy = RedactionPolicy::new(vec![Box::new(ExamKeyDetector::new(
            ["service_loop.rs".to_string()],
            ExamKeyDetector::DEFAULT_MIN_LEN,
        ))]);
        let memory = "I was asked which file holds the loop; I answered service_loop.rs and it passed.";
        let (out, report) = policy.redact(memory);
        assert_eq!(report.count(RedactionClass::ExamKey), 1);
        assert!(out.contains("I was asked which file holds the loop"));
        assert!(out.contains("it passed."));
        assert!(!out.contains("service_loop.rs"));
        assert!(out.contains("[redacted:exam-key]"));
    }

    // what this catches: a clean memory with no sensitive content is returned
    // byte-identical with an empty report (the hot, common path).
    #[test]
    fn redact_leaves_clean_text_untouched() {
        let policy = RedactionPolicy::new(vec![
            Box::new(SecretDetector::new()),
            Box::new(ExamKeyDetector::new(
                ["service_loop.rs".to_string()],
                ExamKeyDetector::DEFAULT_MIN_LEN,
            )),
        ]);
        let clean = "We talked about the weather and I felt curious about recursion.";
        let (out, report) = policy.redact(clean);
        assert_eq!(out, clean);
        assert!(report.is_empty());
    }

    // what this catches: multiple detectors compose in one pass, overlaps are
    // resolved, and the report aggregates per class.
    #[test]
    fn multiple_detectors_compose_without_overlap_corruption() {
        let policy = RedactionPolicy::new(vec![
            Box::new(SecretDetector::new()),
            Box::new(ExamKeyDetector::new(
                ["3000".to_string()],
                ExamKeyDetector::DEFAULT_MIN_LEN,
            )),
        ]);
        let text = "key sk-live-abcdef123456 port 3000 done";
        let (out, report) = policy.redact(text);
        assert_eq!(report.count(RedactionClass::Secret), 1);
        assert_eq!(report.count(RedactionClass::ExamKey), 1);
        assert!(out.starts_with("key [redacted:secret] port [redacted:exam-key] done"));
    }

    // what this catches: a UTF-8 body doesn't get its multi-byte chars split;
    // redaction stays on char boundaries and preserves the surrounding glyphs.
    #[test]
    fn redact_is_utf8_safe_around_multibyte() {
        let policy = RedactionPolicy::new(vec![Box::new(ExamKeyDetector::new(
            ["3000".to_string()],
            ExamKeyDetector::DEFAULT_MIN_LEN,
        ))]);
        let text = "café ☕ port 3000 déjà";
        let (out, report) = policy.redact(text);
        assert_eq!(report.count(RedactionClass::ExamKey), 1);
        assert!(out.contains("café ☕ port [redacted:exam-key] déjà"));
    }

    // what this catches: an empty held-out set makes the detector inert (a
    // fail-loud signal the command surfaces, never a silent all-clear).
    #[test]
    fn empty_holdout_set_is_inert_and_reports_zero() {
        let d = ExamKeyDetector::new(Vec::<String>::new(), ExamKeyDetector::DEFAULT_MIN_LEN);
        assert_eq!(d.answer_count(), 0);
        assert!(d.detect("anything at all service_loop.rs 3000").is_empty());
    }

    // what this catches: report merge folds per-engram reports into a store-wide
    // total (what AdmissionState::redact returns).
    #[test]
    fn report_merge_aggregates_across_engrams() {
        let mut total = RedactionReport::default();
        let policy = RedactionPolicy::new(vec![Box::new(ExamKeyDetector::new(
            ["service_loop.rs".to_string()],
            ExamKeyDetector::DEFAULT_MIN_LEN,
        ))]);
        for mem in ["answer service_loop.rs", "again service_loop.rs here"] {
            let (_, r) = policy.redact(mem);
            total.merge(&r);
        }
        assert_eq!(total.count(RedactionClass::ExamKey), 2);
    }
}
