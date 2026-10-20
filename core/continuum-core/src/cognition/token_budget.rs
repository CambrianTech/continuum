//! `token_budget` — the ONE estimator for "how much of the prompt does this
//! piece of content cost?"
//!
//! Every RAG/prompt layer a persona assembles (recall engrams, roster, doctrine,
//! room purpose, workspace map, the airc thread) competes for a finite token
//! budget against the base model's context window. To "take no prompt layer for
//! granted" you must be able to put a number on each layer — and that number has
//! to be the SAME unit everywhere, or the ledger lies.
//!
//! Today the same `((content.chars().count() / 4) as u32).saturating_add(1)` is
//! copy-pasted private into ~6 `persona/*_source.rs` files (one of them on bytes,
//! not chars — already drifting). This is the canonical home they converge onto
//! (task: collapse the 6 private copies onto this). The `cognition/replay` budget
//! ledger uses THIS so its per-layer numbers match what the sources budgeted
//! against — a ledger in different units than the allocator would be a lie.
//!
//! It is a deterministic ESTIMATE (chars/4), not a model tokenizer: it is a
//! measurement instrument for relative layer cost and total-vs-window, run on the
//! cold path, and it must never depend on a loaded model (which may not be
//! resident when you replay). When an exact count is needed against a specific
//! model, that is the tokenizer's job at the inference seam, not this.

/// Estimate the prompt-token cost of a piece of content. Char-based (handles
/// multibyte text correctly), `/4` for the standard ~4-chars-per-token heuristic,
/// `+1` so non-empty content is never costed at zero.
pub fn estimate_prompt_tokens(content: &str) -> u32 {
    if content.is_empty() {
        return 0;
    }
    ((content.chars().count() / 4) as u32).saturating_add(1)
}

/// Traverse structured textual payloads using the caller's existing pricing policy.
/// Deliberation's conservative guard and RAG's relative estimate have different
/// units; preserving that policy avoids silently recalibrating existing text.
/// Media bytes are excluded; the finalized provider quote owns media/template cost.
pub fn estimate_message_content_tokens(
    content: &crate::ai::types::MessageContent,
    estimate: impl Fn(&str) -> usize,
) -> usize {
    use crate::ai::types::{ContentPart, MessageContent};
    match content {
        MessageContent::Text(text) => estimate(text),
        MessageContent::Parts(parts) => parts.iter().fold(0usize, |total, part| {
            let tokens = match part {
                ContentPart::Text { text } => estimate(text),
                ContentPart::ToolUse { id, name, input } => estimate(id)
                    .saturating_add(estimate(name))
                    .saturating_add(estimate(&input.to_string())),
                ContentPart::ToolResult {
                    tool_use_id,
                    content,
                    ..
                } => estimate(tool_use_id).saturating_add(estimate(content)),
                ContentPart::Image { .. }
                | ContentPart::Audio { .. }
                | ContentPart::Video { .. } => 0,
            };
            total.saturating_add(tokens)
        }),
    }
}

/// Keep the END of `content` up to ~`budget_tokens` (chars/4, as [`estimate_prompt_tokens`]),
/// cutting on a char boundary and preferring the first newline inside the kept slice so the
/// cut lands between lines. The END is what is kept because it is what happened last: an
/// outcome, an answer, a question put to her sit there, and a prompt that loses them acts on
/// a world where time never moved.
pub fn tail_to_tokens(content: &str, budget_tokens: u32) -> String {
    let budget_chars = (budget_tokens as usize).saturating_mul(4);
    let total = content.chars().count();
    if total <= budget_chars {
        return content.to_string();
    }
    let start = content
        .char_indices()
        .nth(total - budget_chars)
        .map(|(i, _)| i)
        .unwrap_or(0); // total > budget_chars, so nth always exists; 0 keeps everything if not
    let slice = &content[start..];
    match slice.find('\n') {
        // only prefer the line cut when it keeps a useful fraction of the slice
        Some(nl) if slice.len() - nl > budget_chars / 2 => slice[nl + 1..].to_string(),
        _ => slice.to_string(),
    }
}

/// The ONE vocabulary for telling a mind her copy of something was shortened.
///
/// An unmarked fragment does not read as a fragment: it reads as a whole message that begins
/// mid-sentence, and a citizen reconstructs an intent that was never delivered (card
/// 3833b472). The marker LEADS, because the OPENING is what went missing (the end is always
/// kept, see [`tail_to_tokens`]) and a citizen reading left-to-right must learn that first.
///
/// `what` names the kind of thing ("message", "post"); `expand` is the HANDLE, the exact verb
/// (and the id it takes) that yields the whole of it, when one exists. A trim with no handle
/// is a clip (whole-or-handle, PERSONA-COGNITION-PIPELINE.md §3).
pub fn mark_tail_kept(tail: &str, full_tokens: u32, what: &str, expand: Option<&str>) -> String {
    match expand {
        Some(cmd) => format!("(…{full_tokens}-token {what}, opening trimmed — {cmd}) {tail}"),
        None => format!("(…{full_tokens}-token {what}, opening trimmed) {tail}"),
    }
}

/// `text` whole when it fits `budget`, else the marker plus its END, the whole costing at most
/// `budget`. `None` when the budget cannot carry the marker plus any of the end. The ONE
/// tail-and-handle fit: sources never clip with their own char count, and never keep the head.
pub fn fit_tail_with_handle(text: &str, budget: u32, what: &str, expand: Option<&str>) -> Option<String> {
    let full = estimate_prompt_tokens(text);
    if full <= budget {
        return Some(text.to_string());
    }
    let marker = estimate_prompt_tokens(&mark_tail_kept("", full, what, expand));
    let mut tail_budget = budget.checked_sub(marker)?;
    while tail_budget > 0 {
        let tail = tail_to_tokens(text, tail_budget);
        if tail.is_empty() {
            return None;
        }
        let shown = mark_tail_kept(&tail, full, what, expand);
        if estimate_prompt_tokens(&shown) <= budget {
            return Some(shown);
        }
        tail_budget -= 1; // the estimator rounds each piece up, so the sum can overshoot by one
    }
    None
}

/// The units that fit `budget`, in order, the NEWEST (last) end whole. `pinned` leading units
/// are always kept (a headline that says what the rest is); of the rest the OLDEST are
/// omitted, and a marker unit right after the pinned ones says how many and names `expand`.
/// Order is the units' own order (time, or document order): the end is what is kept.
/// When even the last unit does not fit alone it is delivered as its tail plus the
/// opening-trimmed marker. `truncated` says anything was left out or cut.
pub struct PackedUnits {
    pub items: Vec<(String, u32)>,
    pub truncated: bool,
}

pub fn pack_suffix_units(
    units: &[String],
    pinned: usize,
    budget: u32,
    what: &str,
    expand: Option<&str>,
) -> Option<PackedUnits> {
    let pinned = pinned.min(units.len());
    let (head, rest) = units.split_at(pinned);
    let head_cost: u32 = head.iter().map(|u| estimate_prompt_tokens(u)).sum();
    let notice = |omitted: usize| match expand {
        Some(cmd) => format!("…{omitted} earlier {what}(s) not shown — {cmd}"),
        None => format!("…{omitted} earlier {what}(s) not shown"),
    };
    // the most trailing units whose whole (with the notice when some are left out) fits
    let mut keep = 0usize;
    for k in (1..=rest.len()).rev() {
        let omitted = rest.len() - k;
        let tail_cost: u32 = rest[omitted..].iter().map(|u| estimate_prompt_tokens(u)).sum();
        let notice_cost = if omitted > 0 { estimate_prompt_tokens(&notice(omitted)) } else { 0 };
        if head_cost.saturating_add(tail_cost).saturating_add(notice_cost) <= budget {
            keep = k;
            break;
        }
    }
    let mut items: Vec<(String, u32)> = head.iter().map(|u| (u.clone(), estimate_prompt_tokens(u))).collect();
    if rest.is_empty() {
        return (head_cost <= budget).then_some(PackedUnits { items, truncated: false });
    }
    if keep == 0 {
        // not even the newest unit fits beside the pinned ones: its tail, marked
        let room = budget.checked_sub(head_cost)?;
        let last = rest.last()?;
        let shown = fit_tail_with_handle(last, room, what, expand)?;
        let cost = estimate_prompt_tokens(&shown);
        items.push((shown, cost));
        return Some(PackedUnits { items, truncated: true });
    }
    let omitted = rest.len() - keep;
    if omitted > 0 {
        let n = notice(omitted);
        let cost = estimate_prompt_tokens(&n);
        items.push((n, cost));
    }
    for u in &rest[omitted..] {
        items.push((u.clone(), estimate_prompt_tokens(u)));
    }
    Some(PackedUnits { items, truncated: omitted > 0 })
}

/// The fair share each of `costs` may take of `budget`, when they do not all fit whole:
/// the max-min ("water-filling") level L such that `sum(min(cost, L)) <= budget`. A
/// small item keeps all of its cost; only an item bigger than the level collapses to
/// it, and the level is derived from the measured costs, never a constant. `None` =
/// everything fits whole, nothing collapses.
pub fn fair_share_cap(costs: &[u32], budget: u32) -> Option<u32> {
    let total: u64 = costs.iter().map(|&c| u64::from(c)).sum();
    if total <= u64::from(budget) {
        return None;
    }
    let mut sorted: Vec<u32> = costs.to_vec();
    sorted.sort_unstable();
    let mut remaining = u64::from(budget);
    for (i, &c) in sorted.iter().enumerate() {
        let left = (sorted.len() - i) as u64;
        if u64::from(c) * left > remaining {
            // `left` >= 1 here, so the division is defined
            return Some((remaining / left) as u32);
        }
        remaining -= u64::from(c);
    }
    None // unreachable when total > budget; every cost fit its share
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (card 3833b472): an unmarked fragment, and a cut that keeps the HEAD.
    // The marker must LEAD (the opening is what is missing), name the handle, and the END of
    // the text must survive verbatim: what happened last is what she must not lose.
    #[test]
    fn a_trim_keeps_the_end_and_leads_with_a_marker_naming_the_handle() {
        let tail = mark_tail_kept("the ending survives", 4321, "message", Some("`chat/history`"));
        assert_eq!(tail, "(…4321-token message, opening trimmed — `chat/history`) the ending survives");
        let text = format!("{}THE OUTCOME IS HERE", "background detail. ".repeat(200));
        let shown = fit_tail_with_handle(&text, 60, "post", Some("persona/wall/list")).expect("fits");
        assert!(shown.ends_with("THE OUTCOME IS HERE"), "the end is kept whole: {shown}");
        assert!(shown.starts_with("(…") && shown.contains("persona/wall/list"), "{shown}");
        assert!(estimate_prompt_tokens(&shown) <= 60, "{}", estimate_prompt_tokens(&shown));
        assert_eq!(fit_tail_with_handle("short", 50, "post", None).as_deref(), Some("short"));
        assert!(fit_tail_with_handle(&text, 3, "post", Some("persona/wall/list")).is_none());
    }

    // what this catches: the share being a constant again. The level must come from the
    // measured costs: everything fits -> no collapse; one essay among short turns -> only
    // the essay collapses and the short turns keep every token.
    #[test]
    fn fair_share_collapses_only_what_exceeds_its_share() {
        assert_eq!(fair_share_cap(&[10, 20, 30], 60), None);
        assert_eq!(fair_share_cap(&[10, 20, 500], 100), Some(70));
        assert_eq!(fair_share_cap(&[400, 400], 100), Some(50));
        assert_eq!(fair_share_cap(&[], 0), None);
    }

    // what this catches: a list clipped from the NEWEST end. Units that fit come back whole and
    // in order; when they do not, the OLDEST go, the omitted count and the verb are named in a
    // marker unit, the pinned headline stays first, and the newest unit survives a tight budget.
    #[test]
    fn suffix_packing_omits_the_oldest_and_names_them() {
        let units: Vec<String> = (0..10).map(|i| format!("line number {i}")).collect();
        let all = pack_suffix_units(&units, 0, 10_000, "line", None).expect("fits");
        assert!(!all.truncated && all.items.len() == 10);

        let some = pack_suffix_units(&units, 1, 30, "line", Some("code/tree")).expect("fits some");
        assert!(some.truncated);
        assert_eq!(some.items[0].0, "line number 0", "the pinned headline stays first");
        assert!(some.items[1].0.contains("earlier line(s) not shown") && some.items[1].0.contains("code/tree"));
        assert_eq!(some.items.last().map(|i| i.0.as_str()), Some("line number 9"), "the newest survives");
        assert!(some.items.iter().map(|i| i.1).sum::<u32>() <= 30);

        let big = vec!["old. ".repeat(300), format!("{}FINAL ANSWER", "x ".repeat(300))];
        let cut = pack_suffix_units(&big, 0, 60, "para", Some("cmd")).expect("tail");
        assert!(cut.items.last().expect("one").0.ends_with("FINAL ANSWER"));
    }

    // what this catches: the estimator drifting from the chars/4+1 unit the RAG
    // sources budget against (which would make the replay ledger's numbers lie),
    // and the empty-string edge costing a phantom token.
    #[test]
    fn estimate_matches_the_rag_source_unit_and_zeroes_empty() {
        assert_eq!(estimate_prompt_tokens(""), 0, "empty content costs nothing");
        // 8 chars / 4 + 1 = 3 — the exact formula the *_source.rs copies use.
        assert_eq!(estimate_prompt_tokens("abcdefgh"), 3);
        // multibyte counted by chars, not bytes: 4 chars / 4 + 1 = 2.
        assert_eq!(estimate_prompt_tokens("日本語訳"), 2);
        use crate::ai::types::{ContentPart, ImageInput, MessageContent};
        let parts = MessageContent::Parts(vec![
            ContentPart::ToolUse {
                id: "call".into(),
                name: "code/read".into(),
                input: serde_json::json!({"path": "日本語訳.rs"}),
            },
            ContentPart::ToolResult {
                tool_use_id: "call".into(),
                content: "payload".repeat(100),
                is_error: Some(true),
            },
        ]);
        assert!(
            estimate_message_content_tokens(&parts, |text| estimate_prompt_tokens(text) as usize)
                > estimate_prompt_tokens(&"payload".repeat(100)) as usize
        );
        let pixels = MessageContent::Parts(vec![ContentPart::Image {
            image: ImageInput {
                url: None,
                base64: Some("a".repeat(10_000)),
                mime_type: Some("image/png".into()),
            },
        }]);
        assert_eq!(
            estimate_message_content_tokens(&pixels, |text| estimate_prompt_tokens(text) as usize),
            0,
            "base64 is not text; finalized provider quote prices pixels"
        );
        // monotonic: more content never costs fewer tokens.
        assert!(estimate_prompt_tokens("a".repeat(400).as_str()) > estimate_prompt_tokens("a"));
    }
}
