//! `self_sized`: a consumer deciding how much of a resource it gets, instead of taking it
//! from its grant. The shapes, each found at many sites on canary 365f70fe4:
//!
//! - a sized constant (`EMBEDDING_CACHE_MAX = 20_000`, `KEEP = 8`, `ECHOED_ARG_CHARS = 600`);
//! - a product of constants (`PREFILL_TARGET_SECONDS × CONSERVATIVE_PREFILL_TOKENS_PER_S`
//!   clipped every machine's conversation to a flat 15,000 tokens);
//! - a literal ceiling (`.take(8)`, `.truncate(5)`, `.min(1_500)`, `channel(256)`);
//! - a head cut of text she will read (`.chars().take(`): it drops what happened last, so
//!   she acts as if time never moved and repeats herself (Benchy, 125 identical acts, 10-20);
//! - a size read off `ContextBudget`'s fractions of the window, a second budget the turn's
//!   one allocation never saw (the 739-probe swing on 10-28).
//!
//! Durations are a different class (`time_bounds`); a constant
//! whose name says it is a time is not counted here.

use super::{is_duration_name, is_vendored, owned_by};
use crate::lexer::{constant_product, is_bare_literal, numeric_const};
use crate::{sites, SourceFile, SourceRule, Violation};

/// The allocator: the one place sizes are computed.
const ALLOCATOR: &[&str] = &["core/continuum-grounding/src/rag_budget/flex_allocate.rs"];

/// The turn's one allocation: the only caller of the allocator for a mind's prompt.
const TURN_ALLOCATION: &[&str] = &["core/continuum-cognition/src/cognition/llm_deliberation_faculty/prompt/"];

/// Where text is rendered for a mind to read; a head cut there loses her newest end.
const MIND_TEXT: &[&str] = &[
    "core/continuum-cognition/",
    "core/continuum-mind/",
    "core/continuum-prompt/",
    "core/continuum-grounding/",
    "core/continuum-context/",
    "core/continuum-academy/",
    "core/continuum-perception/",
    "core/continuum-capture/",
    "core/continuum-core/src/commands/tool/",
];

/// Module directories whose text reaches a mind wherever their crate is (`head_cuts`' coverage,
/// kept whole: modules/voice_reply_speaker.rs cut spoken text by its head, Codex on #5609).
const MIND_TEXT_MODULES: &[&str] =
    &["cognition/", "persona/", "genome/", "rag/", "modules/", "airc/", "positron/", "experience/", "recipe/"];

/// Name fragments that say a constant sizes something.
const SIZE_WORDS: &[&str] = &[
    "CAP", "MAX", "MIN_", "FLOOR", "CEILING", "LIMIT", "BOUND", "BUDGET", "QUOTA", "RESERVE", "ALLOWANCE",
    "HEADROOM", "KEEP", "RETAIN", "ENTRIES", "WINDOW", "CONTEXT", "CTX", "TOKEN", "PROMPT", "CHARS", "BYTES",
    "KIB", "MIB", "GIB", "FRACTION", "SHARE", "DIVISOR", "RATIO", "PCT", "PERCENT", "LANES", "SLOTS", "LINES",
    "ROWS", "ITEMS", "SIZE", "LEN", "DEPTH", "GENERATIONS", "STEPS",
];

/// Calls that size against `ContextBudget`'s fractions of the window, or allocate outside
/// the turn's one allocation.
const BUDGET_FRACTIONS: &[&str] = &[
    "ContextBudget::live(",
    "ContextBudget::live_or_floor(",
    "ContextBudget::from_window(",
    "ContextBudget::from_window_for(",
    "ContextBudget::from_window_unbounded(",
    ".recall_tokens(",
    ".grounding_tokens(",
    ".tool_surface_tokens(",
    ".latest_action_chars(",
    ".dispatch_result_chars(",
    ".render_slice_chars(",
    ".catalog_summary_chars(",
    ".echoed_arg_chars(",
    ".trail_head_chars(",
    ".recent_results_chars(",
    ".receipt_archive_chars(",
    ".default_response_tokens(",
    "_SHARE_DENOM",
];

/// Calls into the allocator; allowed only in the turn's allocation.
const ALLOCATOR_CALLS: &[&str] = &["flex_allocate(", "budget_claim(", "FlexboxRagBudgetAdapter::new(", "adapter.allocate("];

/// Calls that keep the START of text and drop its end.
const HEAD_KEEPERS: &[&str] = &[".chars().take(", "head_to_tokens(", "truncate_chars(", ".lines().next()"];

/// Calls whose literal argument is a ceiling on a count or a length.
const LITERAL_CEILINGS: &[&str] = &[".take(", ".truncate(", "[..", "channel(", "bounded("];

pub struct SelfSized;

impl SourceRule for SelfSized {
    fn name(&self) -> &'static str {
        "self_sized"
    }

    fn check(&self, file: &SourceFile) -> Vec<Violation> {
        if is_vendored(file) || owned_by(file, ALLOCATOR) {
            return Vec::new();
        }
        let turn_allocation = owned_by(file, TURN_ALLOCATION);
        let mind_text = owned_by(file, MIND_TEXT) || MIND_TEXT_MODULES.iter().any(|module| file.rel.starts_with(module));
        sites(file, self.name(), |code| shape(code, turn_allocation, mind_text))
    }
}

/// Which self-sizing shape `code` has, if any.
fn shape(code: &str, turn_allocation: bool, mind_text: bool) -> Option<String> {
    if let Some((name, _, value)) = numeric_const(code) {
        if names_a_size(name) && is_bare_literal(value) {
            return Some(format!("sized constant `{name}`"));
        }
    }
    if let Some((left, right)) = constant_product(code) {
        if names_a_size(left) || names_a_size(right) {
            return Some(format!("product of constants `{left} * {right}`"));
        }
    }
    if let Some(call) = BUDGET_FRACTIONS.iter().find(|c| code.contains(**c)) {
        return Some(format!("a fraction of the window via `{call}`"));
    }
    if !turn_allocation {
        if let Some(call) = ALLOCATOR_CALLS.iter().find(|c| code.contains(**c)) {
            return Some(format!("an allocation outside the turn's one allocation via `{call}`"));
        }
    }
    if mind_text {
        if let Some(call) = HEAD_KEEPERS.iter().find(|c| code.contains(**c)) {
            return Some(format!("a head cut via `{call}`"));
        }
    }
    if let Some(call) = LITERAL_CEILINGS.iter().find(|c| literal_after(code, c)) {
        return Some(format!("a literal ceiling via `{call}`"));
    }
    if names_a_size_in(code) && (literal_after(code, ".min(") || literal_clamp_ceiling(code)) {
        return Some("a literal ceiling via `.min`/`.clamp`".to_owned());
    }
    None
}

/// Does this constant's name say it sizes something (and is not a duration)?
fn names_a_size(name: &str) -> bool {
    SIZE_WORDS.iter().any(|w| name.contains(w)) && !is_duration_name(name)
}

/// Does this line talk about a size (so a bare `.min(3)` on it is a ceiling)?
fn names_a_size_in(code: &str) -> bool {
    let upper = code.to_ascii_uppercase();
    names_a_size(&upper)
}

/// Is `call` (`.take(`, `channel(`, `channel::<T>(`) given a literal?
fn literal_after(code: &str, call: &str) -> bool {
    let direct = code.match_indices(call).any(|(at, _)| code[at + call.len()..].trim_start().starts_with(|c: char| c.is_ascii_digit()));
    direct || call.strip_suffix('(').is_some_and(|name| turbofish_literal(code, &format!("{name}::<")))
}

/// `channel::<T>(256)`: a literal capacity after a turbofish.
fn turbofish_literal(code: &str, call: &str) -> bool {
    code.match_indices(call).any(|(at, _)| {
        let rest = &code[at + call.len()..];
        rest.find(">(").is_some_and(|close| rest[close + 2..].trim_start().starts_with(|c: char| c.is_ascii_digit()))
    })
}

/// `.clamp(low, N)` with a literal upper bound.
fn literal_clamp_ceiling(code: &str) -> bool {
    code.match_indices(".clamp(").any(|(at, _)| {
        let args = &code[at + ".clamp(".len()..];
        let Some(close) = args.find(')') else { return false };
        let Some((_, upper)) = args[..close].rsplit_once(',') else { return false };
        upper.trim_start().starts_with(|c: char| c.is_ascii_digit())
    })
}
