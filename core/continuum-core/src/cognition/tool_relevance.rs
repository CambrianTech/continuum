//! Which tool CATEGORIES the adaptive tool menu should EXPAND for this turn.
//!
//! The persona's command surface is ~170 verbs across ~37 categories. Dumping
//! every verb every turn is the "1000-button menu of yesteryear"; hiding them
//! behind a bare category index makes her guess where a tool lives. The middle
//! path (Joel 2026-06-29, [[adaptive-tool-surface-meets-you-in-the-middle]]) is an
//! EXPANDABLE BOOKMARKED MENU: every category HEADER is always shown (the stable
//! spine — the menu never changes shape), but only the categories RELEVANT to what
//! she is doing right now are opened to show their verbs; the rest stay collapsed
//! bookmarks she can open with `commands/list --filter <category>`.
//!
//! This module owns the "which to open" decision. It is a VISIBILITY concern, not
//! an authorization one — everything stays callable by name; expansion only changes
//! what is SHOWN by default. And it is data-driven relevance, NOT a hardcoded
//! everyday-vs-obscure category list and NOT a reader of the persona's output to
//! puppet it ([[no-hardcoded-heuristics-to-steer-cognition]]).
//!
//! ## Outlier-validated interface (CLAUDE.md outlier doctrine)
//! [`ToolRelevance`] is scored at two extremes to prove the interface holds:
//! - Outlier A — [`LexicalToolRelevance`] (here): keyword overlap, no model, no I/O.
//! - Outlier B — a neural embedding scorer over [`crate::cognition::tool_embedding`]'s
//!   `cosine_similarity` (same signature, richer signal); slice 2+.
//! Same trait, same selection, same render — only the scorer swaps.

use std::collections::BTreeSet;

/// Scores how relevant one tool category is to the current turn context, so the
/// adaptive menu can EXPAND matching categories and leave the rest as collapsed
/// bookmarks. Implementations range from cheap lexical overlap to neural embedding
/// similarity — the menu does not care which, it just ranks and opens the top.
pub trait ToolRelevance: Send + Sync {
    /// Relevance in `[0.0, 1.0]` of `category` (with its member `verbs`, the names
    /// after the leading category segment) to the turn `context`. Higher = more
    /// worth opening this turn.
    fn score(&self, category: &str, verbs: &[&str], context: &str) -> f32;
}

/// Outlier A: keyword-overlap relevance. No model, no I/O — measures what fraction
/// of the turn's meaningful tokens this category's own vocabulary (its name + its
/// verb names) covers. Size-independent by construction: a bigger category that
/// genuinely addresses more of the task scores higher, which is correct, rather than
/// being penalized for having more verbs. Proves the [`ToolRelevance`] interface at
/// the cheap extreme; the embedding scorer proves the same signature at the neural
/// extreme.
pub struct LexicalToolRelevance;

impl ToolRelevance for LexicalToolRelevance {
    fn score(&self, category: &str, verbs: &[&str], context: &str) -> f32 {
        let ctx = tokenize(context);
        if ctx.is_empty() {
            return 0.0;
        }
        let mut vocab = tokenize(category);
        for v in verbs {
            vocab.extend(tokenize(v));
        }
        if vocab.is_empty() {
            return 0.0;
        }
        // Fraction of the turn's tokens this category's vocabulary covers — "how
        // much of what she's talking about is this category about".
        let covered = ctx.iter().filter(|t| vocab.contains(*t)).count();
        covered as f32 / ctx.len() as f32
    }
}

/// Lowercase, split on any non-alphanumeric (so `code/run` → `code`, `run`), drop
/// tokens too short to be discriminating. A `BTreeSet` so membership is the only
/// thing that matters (no double-counting, deterministic).
fn tokenize(text: &str) -> BTreeSet<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() >= 3)
        .map(|t| t.to_ascii_lowercase())
        .collect()
}

/// Pick which categories to OPEN this turn. The result is the sticky "where you
/// were" mode cursor (always opened when present — this is the hysteresis that
/// keeps the menu coherent instead of flickering) plus the highest relevance-scored
/// categories that clear `floor`, capped at `max_expanded` total. Ties break by
/// category name so equally-scored categories never swap places between turns.
///
/// `categories` is `(category, verbs)` for the full authorized surface; `context`
/// is the turn text; `sticky` is the category she was last working in (the cursor,
/// state owned by the caller — `None` on a cold turn).
pub fn select_expanded_categories<R: ToolRelevance + ?Sized>(
    scorer: &R,
    categories: &[(&str, Vec<&str>)],
    context: &str,
    sticky: Option<&str>,
    max_expanded: usize,
    floor: f32,
) -> BTreeSet<String> {
    let mut expanded: BTreeSet<String> = BTreeSet::new();

    // The mode cursor is opened first and unconditionally — "keeps track of where
    // you were" — as long as it is a real category in the surface.
    if let Some(s) = sticky {
        if categories.iter().any(|(c, _)| *c == s) {
            expanded.insert(s.to_string());
        }
    }

    // Score the rest; sort by score desc, then name asc for stable ordering.
    let mut scored: Vec<(&str, f32)> = categories
        .iter()
        .map(|(c, verbs)| (*c, scorer.score(c, verbs, context)))
        .filter(|(_, s)| *s >= floor)
        .collect();
    scored.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(b.0))
    });

    for (cat, _) in scored {
        if expanded.len() >= max_expanded {
            break;
        }
        expanded.insert(cat.to_string());
    }
    expanded
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: lexical scorer ranks the on-task category above off-task
    // ones from the turn's vocabulary alone (the core relevance signal).
    #[test]
    fn lexical_ranks_on_task_category_highest() {
        let s = LexicalToolRelevance;
        let ctx = "read the file and run the tests";
        let code = s.score("code", &["run", "read", "edit", "write"], ctx);
        let data = s.score("data", &["list", "get", "create", "delete"], ctx);
        let gpu = s.score("gpu", &["stats", "pressure"], ctx);
        assert!(code > data, "code {code} should beat data {data}");
        assert!(code > gpu, "code {code} should beat gpu {gpu}");
    }

    // what this catches: empty / non-overlapping context scores 0, never NaN — the
    // [0,1] contract holds at the degenerate edges (no panic, no fallback).
    #[test]
    fn lexical_degenerate_inputs_are_zero_not_nan() {
        let s = LexicalToolRelevance;
        assert_eq!(s.score("code", &["run"], ""), 0.0);
        let off = s.score("gpu", &["stats"], "hello there friend");
        assert!(off.is_finite() && off == 0.0, "off-task = {off}");
    }

    // what this catches: the sticky mode cursor is ALWAYS opened (the "where you
    // were" hysteresis) even when the turn text doesn't mention it — the property
    // that keeps the menu from flickering.
    #[test]
    fn sticky_mode_cursor_stays_open() {
        let cats = vec![
            ("code", vec!["run", "read"]),
            ("data", vec!["list", "get"]),
            ("gpu", vec!["stats"]),
        ];
        let exp = select_expanded_categories(
            &LexicalToolRelevance,
            &cats,
            "list the rows", // talks about data, not code
            Some("code"),    // but she was just in code
            2,
            0.01,
        );
        assert!(
            exp.contains("code"),
            "sticky cursor must stay open: {exp:?}"
        );
        assert!(exp.contains("data"), "on-task category opens too: {exp:?}");
    }

    // what this catches: max_expanded is a hard cap (the spine stays mostly
    // collapsed — it's a menu, not a dump) and an unknown sticky category is ignored
    // rather than fabricated.
    #[test]
    fn cap_is_respected_and_unknown_sticky_ignored() {
        let cats = vec![
            ("code", vec!["run", "read", "edit"]),
            ("data", vec!["list", "get"]),
            ("memory", vec!["recall", "remember"]),
        ];
        let exp = select_expanded_categories(
            &LexicalToolRelevance,
            &cats,
            "run read edit list get recall remember",
            Some("nonexistent"), // not in the surface → must not appear
            2,
            0.0,
        );
        assert!(exp.len() <= 2, "cap of 2 violated: {exp:?}");
        assert!(!exp.contains("nonexistent"), "fabricated sticky: {exp:?}");
    }
}
