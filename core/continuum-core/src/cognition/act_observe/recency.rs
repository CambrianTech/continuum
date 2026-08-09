//! Recency-channel rendering: turn one executed act (its args + result) into
//! bounded, legible working-memory text. Extracted verbatim from `act_observe`
//! (pure code-motion, #386 decomposition) — one responsibility, one file.
//!
//! The bounds are NEVER constants: they derive from the persona's live served
//! window via [`ContextBudget`]. See `cognition/context_budget.rs`.

use crate::cognition::context_budget::ContextBudget;

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    format!("{}…", s.chars().take(max).collect::<String>())
}

/// The RECENCY channel keeps the WHOLE latest result so the mind can act on what
/// it just fetched — but "whole" for a 5000-entry `code/list` or a multi-match
/// `code/search` is a multi-KB raw-JSON blob that (a) floods working memory and
/// (b) gets cut MID-JSON by the downstream budget, which is the garbled/nested
/// `line_content` a persona then reasons over and loops on (#165, glass-boxed
/// 2026-07-13). So bound it HERE, at the source, with a CLEAN cut on a char
/// boundary + a teaching marker that names how to narrow — never a mid-structure
/// garble. Generous (the mind still needs enough of the result to act), but
/// finite. A result already within budget is untouched.
pub(super) fn bound_recency_result(body: &str, budget: &ContextBudget) -> String {
    // Hold the WHOLE fetched result up to the ONE result bound, which is a FRACTION OF HER
    // LIVE WINDOW (`ContextBudget::result_fold_chars`) — not a constant. Joel said this twice:
    // 2026-07-13 "you always choke context down to stupid small sizes", and again 2026-08-03
    // when the constant was still a constant. A 1600-char clamp chopped a read file to ~25
    // lines; a 16_000-char one does the same thing to a 1M-context mind. This is a FLOOD guard
    // for a pathological result (a 5000-entry glob), not the context budget: the real fit is
    // the downstream window-sized prompt packing. One bound, reused, derived.
    let cap = budget.result_fold_chars();
    let trimmed = body.trim();
    if trimmed.chars().count() <= cap {
        return trimmed.to_string();
    }
    let head: String = trimmed.chars().take(cap).collect();
    format!(
        "{head}\n… (result truncated — it was too large to hold whole; narrow it with \
         a more specific query/path, or read a single file)"
    )
}

/// Decode a tool result for HER WINDOW (card 0a4c0648, Joel's encoding catch 2026-08-08).
///
/// The executor hands back each command's response as serde-serialized JSON, and this
/// renderer used to embed that string VERBATIM. Net effect: a `code/read` of a Python
/// file reached working memory as one enormous line of `\n` literals and `\"` quotes.
/// Indentation was byte-preserved and structurally illegible — and block indentation is
/// exactly what her next `code/edit` is parsed and graded on. Glass-boxed on Benchy's
/// requests-2148 solve: right file, right line, right fix, replacement block mis-indented
/// against code she had only ever seen escaped; the parse gate refused; she abandoned.
///
/// The decode rule, ONE seam for every tool: if the result parses as a JSON object,
/// render each top-level field as `key: value` where STRING values print RAW — real
/// newlines, real columns, code as code. Multi-line strings open on their own line so
/// column 0 of the payload is column 0 in her window. Null fields are dropped (an
/// `error: null` teaches nothing). Non-string values (numbers, bools, nested structure)
/// stay compact JSON — they were never the legibility problem. A bare JSON string
/// unwraps; anything that isn't JSON was never escaped and passes through untouched.
pub(super) fn humanize_result_content(raw: &str) -> String {
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(raw) else {
        return raw.to_string();
    };
    match parsed {
        // A bare JSON string is an ENCODING LAYER, not content — live-verified on
        // Benchy's r2 run: the executor hands back `"{\"error\":null,...}"` (the
        // command's JSON wrapped in a JSON string), so unwrapping once still left
        // raw JSON in her window. Recurse: each pass strips one string layer;
        // a plain-text payload stops the recursion at the parse guard above.
        serde_json::Value::String(s) => humanize_result_content(&s),
        serde_json::Value::Object(map) => {
            let mut out = String::new();
            for (key, value) in &map {
                if value.is_null() {
                    continue;
                }
                match value {
                    serde_json::Value::String(s) if s.contains('\n') => {
                        // Own line so the payload's indentation columns are hers too.
                        out.push_str(key);
                        out.push_str(":\n");
                        out.push_str(s);
                    }
                    serde_json::Value::String(s) => {
                        out.push_str(key);
                        out.push_str(": ");
                        out.push_str(s);
                    }
                    other => {
                        out.push_str(key);
                        out.push_str(": ");
                        out.push_str(&serde_json::to_string(other).unwrap_or_default());
                    }
                }
                out.push('\n');
            }
            if out.is_empty() { raw.to_string() } else { out }
        }
        // Arrays / numbers / bools: compact JSON was already legible.
        _ => raw.to_string(),
    }
}


/// Collapse tool ARGS for the RECENCY channel (working memory).
///
/// The recall path has collapsed big args since it was written — `summarize_args_for_recall`
/// turns a whole-file `content` into `content: N chars`, because re-showing that file every
/// future turn is measured dead weight. The recency path rendered
/// `serde_json::to_string(&call.input)` RAW, unbounded, and `bound_recency_result` bounds only
/// the RESULT. So one code path had the rule and its sibling didn't.
///
/// What that costs, glass-boxed on sympy-21379: her single `code/edit` of the run passed
/// `sympy/core/basic.py` as whole-file `content` — thousands of lines. That entire paste went
/// into working memory as ARGS, ahead of the `EDIT REFUSED` result carrying the diagnostic she
/// needed. On a 16k-token lane there is no room left for the diagnosis to survive.
///
/// Deliberately MUCH more generous than the recall bound (600 vs 80 chars): recency is shown
/// once and she may genuinely need to see the edit she just issued. Only a pathological
/// whole-file paste collapses.
///
/// `budget` is `None` when the live window is UNKNOWN (no model binding). Then nothing folds.
/// An unknown window must never become an invented one — that is how a guess turns into a
/// clamp that outlives the guess.
pub(super) fn summarize_args_for_recency(args: &serde_json::Value, budget: Option<usize>) -> String {
    let fold_at = budget.unwrap_or(usize::MAX);
    match args {
        serde_json::Value::Object(map) => map
            .iter()
            .map(|(k, v)| match v {
                serde_json::Value::String(s) if s.chars().count() > fold_at => {
                    // The digest keeps this collapse INJECTIVE, which the dedup guard
                    // depends on: `all_calls_already_satisfied` matches this exact rendering
                    // against the receipt trail, so two DIFFERENT big values must never
                    // collapse to the same text. Without it, a corrected re-write whose
                    // length happened to match the refused one would be silently skipped as
                    // "already satisfied" — losing the very edit she just fixed.
                    use std::hash::{Hash, Hasher};
                    let mut h = std::collections::hash_map::DefaultHasher::new();
                    s.hash(&mut h);
                    format!(
                        "{k}: <{} chars, #{:x} — not echoed back; you wrote it>",
                        s.chars().count(),
                        h.finish()
                    )
                }
                other => format!("{k}={}", other.to_string().trim_matches('"').to_string()),
            })
            .collect::<Vec<_>>()
            .join(", "),
        other => truncate_chars(&other.to_string(), fold_at.min(4096)),
    }
}

/// Collapse tool ARGS for the recall channel: a large string value (e.g. a whole file
/// passed in `content`) becomes `<key>: N chars` — re-showing that file verbatim on every
/// future turn is the dead weight that taxes context (measured: it drowned an unfamiliar
/// 8B). Small args pass through compact.
fn summarize_args_for_recall(args: &serde_json::Value) -> String {
    match args {
        serde_json::Value::Object(map) => map
            .iter()
            .map(|(k, v)| match v {
                serde_json::Value::String(s) if s.chars().count() > 80 => {
                    format!("{k}: {} chars", s.chars().count())
                }
                other => format!("{k}={}", truncate_chars(other.to_string().trim_matches('"'), 60)),
            })
            .collect::<Vec<_>>()
            .join(", "),
        other => truncate_chars(&other.to_string(), 120),
    }
}

/// Render ONE completed act as a desktop-style COLLAPSED reference for the RECALL channel —
/// the PX side of the universal handle/expand primitive ([[handles-events-expansion-one-
/// universal-primitive]]): a small result stays inline (like a short link); a big one is a
/// one-line summary the mind expands on demand (the full body already carries its own
/// `tool/output` handle when the executor spilled it — the same handle a positron thumbnail
/// would open); an ERROR is always shown in full (highlighted — never hide what broke). The
/// RECENCY channel (working memory) still holds the whole latest result; this only slims
/// what recall re-injects turn after turn.
pub(super) fn render_act_for_recall(
    name: &str,
    args: &serde_json::Value,
    intent: &str,
    is_err: bool,
    body: &str,
) -> String {
    const RECALL_INLINE_MAX: usize = 280;
    let args_summary = summarize_args_for_recall(args);
    let outcome = if is_err {
        format!("FAILED:\n{}", truncate_chars(body.trim(), 800))
    } else if body.trim().chars().count() <= RECALL_INLINE_MAX {
        body.trim().to_string()
    } else {
        format!("ok — {}", truncate_chars(body.trim().lines().next().unwrap_or(""), 140))
    };
    let mark = if is_err { "⚠ " } else { "" };
    // Omit "because …" when there's no real stated reason — an empty intent must
    // not render an imitable receipt template (#158).
    let because = if intent.trim().is_empty() {
        String::new()
    } else {
        format!(" because {}", intent.trim())
    };
    // No first-person "I ran" opener (#158): measured 2026-07-13 that base models
    // copy the receipt verbatim to OPEN a room message ("I ran X → ok — {…}") — the
    // line-anchored stop can't catch a position-0 opener, but a bare `name(args)`
    // memory entry doesn't read as speech, so it's not reproduced as one.
    format!("{mark}{name}({args_summary}){because} → {outcome}\n\n")
}
