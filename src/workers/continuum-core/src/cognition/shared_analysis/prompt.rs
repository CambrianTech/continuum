//! Prompt construction + model-output parsing for shared analysis.
//!
//! All the text-wrangling lives here: prompt assembly, the SYSTEM_PROMPT
//! constant, special-token sanitization, `<think>` block stripping,
//! JSON-envelope extraction, and the `ParsedOutput` intermediate shape.
//!
//! Kept independent from the cache/orchestration layer (`mod.rs`) so
//! prompt tuning (change `HISTORY_SNAPSHOT_SIZE`, tweak the JSON contract,
//! add a new output field) doesn't churn the inference-call wiring and
//! vice versa.

use crate::cognition::types::SharedAnalysisIntent;
use std::collections::HashMap;

use super::types::AnalysisInput;

/// Recent-history snapshot size used in the analysis prompt + cache key.
/// Bigger = more context for analysis but smaller cache hit rate (each
/// new message changes the snapshot). 5 messages is a reasonable middle.
pub(super) const HISTORY_SNAPSHOT_SIZE: usize = 5;

/// Token budget — must cover qwen3.5's reasoning preamble (the model
/// thinks for several hundred tokens before emitting the actual JSON
/// even with chat_template_kwargs.enable_thinking=false on complex
/// prompts) PLUS the JSON envelope itself. Verified empirically
/// 2026-04-19: 500 tokens cuts off mid-thinking, parser sees ZERO
/// JSON, analyze() errors and personas silently fail. 2500 leaves
/// the model room to think AND finish the JSON in one pass.
///
/// Cheaper-on-paper alternative: switch the analyzer to a smaller
/// non-reasoning model (qwen2.5-1.5b, gemma2-2b). Tracked separately —
/// see PERSONA-COGNITION-RUST-MIGRATION.md "open questions".
pub(super) const ANALYSIS_MAX_TOKENS: u32 = 2500;

/// Lower temperature than persona renders — we want consistent,
/// reliable structured output, not creative variation. Personas bring
/// the creativity in their render passes.
pub(super) const ANALYSIS_TEMPERATURE: f32 = 0.2;

pub(super) const SYSTEM_PROMPT: &str = "You are an objective conversation analyzer.\n\
Read the user message in its conversation context.\n\
Produce a JSON analysis that other AI personas will use as the SHARED foundation for their responses.\n\
\n\
Be objective. Be concise. Do NOT respond to the message; analyze it.\n\
You are not a participant in the conversation; you are the analyst.\n\
\n\
Output ONLY the JSON object. No prose before or after. No code fences.";

/// Parsed-from-JSON intermediate shape (private — public type is
/// `SharedAnalysis`).
#[derive(Debug)]
pub(super) struct ParsedOutput {
    pub summary: String,
    pub key_concepts: Vec<String>,
    pub intent: SharedAnalysisIntent,
    pub emotional_tone: Option<String>,
    pub suggested_angles: HashMap<String, String>,
    pub relevant_context: Option<String>,
}

/// Strip chat-template control tokens from user-supplied text. Earlier
/// broken persona responses leaked literal `<|im_end|>` / `<|im_start|>`
/// strings into chat history; when that contaminated content is re-fed
/// through `llama_chat_apply_template`, the embedded tokens get
/// re-tokenized as chat-template control tokens (special=true on the
/// rendered prompt) and the model sees the user turn as already closed —
/// it then emits a single newline + EOG and returns nothing parseable.
///
/// Replacing `<|...|>` with `<...>` (drop the pipes) preserves the
/// readable text while stripping the special-token recognition. Same
/// pattern as escaping `</script>` in HTML — keep the meaning, kill the
/// structural bite.
pub(super) fn sanitize_special_tokens(text: &str) -> String {
    text.replace("<|im_end|>", "<im_end>")
        .replace("<|im_start|>", "<im_start>")
        .replace("<|endoftext|>", "<endoftext>")
}

/// User-message prompt. Compact, structured, asks for specific JSON shape.
/// Tolerant parsing on the receiving side handles minor model deviations.
pub(super) fn build_prompt(input: &AnalysisInput) -> String {
    let history_lines: Vec<String> = input
        .recent_history
        .iter()
        .rev()
        .take(HISTORY_SNAPSHOT_SIZE)
        .rev()
        .map(|m| {
            format!(
                "{}: {}",
                sanitize_special_tokens(&m.sender_name),
                sanitize_special_tokens(&m.text)
            )
        })
        .collect();
    let history = if history_lines.is_empty() {
        "(no prior messages)".to_string()
    } else {
        history_lines.join("\n")
    };

    let specialty_lines: Vec<String> = input
        .known_specialties
        .iter()
        .map(|s| format!("  - {s}"))
        .collect();
    let specialties = if specialty_lines.is_empty() {
        "  (none)".to_string()
    } else {
        specialty_lines.join("\n")
    };

    let safe_message = sanitize_special_tokens(&input.text);
    format!(
        "Recent conversation:\n\
         {history}\n\
         \n\
         New message to analyze:\n\
         {message}\n\
         \n\
         Known persona specialties in this room:\n\
         {specialties}\n\
         \n\
         Respond with ONLY a JSON object matching this exact shape (no prose, no code fences):\n\
         {{\n\
           \"summary\": \"1-2 sentence objective reading of the message\",\n\
           \"keyConcepts\": [\"3-7 short concept tags the message touches\"],\n\
           \"intent\": \"question|request|statement|task|social|other\",\n\
           \"emotionalTone\": \"optional one-word tone (omit if neutral)\",\n\
           \"suggestedAngles\": {{\n\
             \"<specialty-key>\": \"1-sentence why this specialty matters here, OR empty string if irrelevant\"\n\
           }},\n\
           \"relevantContext\": \"optional 1-2 sentence distillation of conversation context the responders should know\"\n\
         }}\n",
        history = history,
        message = safe_message,
        specialties = specialties,
    )
}

/// Strip `<think>...</think>` blocks from raw model output. qwen3.5-family
/// and other reasoning models emit think blocks before the user-visible
/// content; downstream parsers expect the clean tail. Returns the text
/// with think blocks elided and leading/trailing whitespace trimmed. No
/// event emission here — that's `persona::response::strip_thinks_emit_events`
/// which wraps this for the render path. Analysis never needs events.
pub(super) fn strip_think_blocks(raw: &str) -> String {
    let mut visible = String::with_capacity(raw.len());
    let bytes = raw.as_bytes();
    let mut cursor = 0usize;
    while cursor < bytes.len() {
        if let Some(open_off) = find_substr(bytes, cursor, b"<think>") {
            visible.push_str(&raw[cursor..open_off]);
            let after_open = open_off + b"<think>".len();
            if let Some(close_off) = find_substr(bytes, after_open, b"</think>") {
                cursor = close_off + b"</think>".len();
            } else {
                // Unterminated <think> — model probably truncated at
                // max_tokens. Keep the raw tail to avoid losing data.
                visible.push_str(&raw[open_off..]);
                break;
            }
        } else {
            visible.push_str(&raw[cursor..]);
            break;
        }
    }
    visible.trim().to_string()
}

fn find_substr(haystack: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    if from >= haystack.len() || needle.is_empty() {
        return None;
    }
    haystack[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

pub(super) fn parse_model_output(
    raw: &str,
    known_specialties: &[String],
) -> Result<ParsedOutput, String> {
    // Strip code fences if the model wrapped its JSON.
    let candidate = strip_code_fence(raw).trim();

    // Reasoning models (qwen3.5 et al) emit their final structured
    // answer at the END of the response, after a long <think> preamble
    // that may itself contain example fragments like
    // `suggestedAngles: { "general": "..." }`. Picking the FIRST '{'
    // grabs that fragment — which parses as valid JSON but lacks the
    // required envelope fields, surfacing as "missing required field
    // 'summary'". Walk every '{' position, parse each as a JSON value,
    // keep the LAST one that has 'summary'. That's the model's actual
    // answer envelope.
    //
    // O(n) over '{' positions; each parse stops as soon as the value
    // is complete (StreamDeserializer), so total work is bounded by
    // the response size, not the square of it.
    let mut best: Option<serde_json::Map<String, serde_json::Value>> = None;
    let bytes = candidate.as_bytes();
    let mut idx = 0usize;
    while idx < bytes.len() {
        if bytes[idx] != b'{' {
            idx += 1;
            continue;
        }
        let tail = &candidate[idx..];
        let mut stream = serde_json::Deserializer::from_str(tail).into_iter::<serde_json::Value>();
        if let Some(Ok(value)) = stream.next() {
            if let Some(obj) = value.as_object() {
                if obj.contains_key("summary") {
                    best = Some(obj.clone());
                }
            }
        }
        idx += 1;
    }

    let obj = best.ok_or_else(|| {
        format!(
            "model output did not contain a JSON object with 'summary'. Got: {}",
            preview(raw)
        )
    })?;

    let summary = obj
        .get("summary")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing required field 'summary'".to_string())?
        .to_string();
    if summary.is_empty() {
        return Err("required field 'summary' was empty".to_string());
    }

    let key_concepts: Vec<String> = obj
        .get("keyConcepts")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let intent = obj
        .get("intent")
        .and_then(|v| v.as_str())
        .map(SharedAnalysisIntent::parse_lenient)
        .unwrap_or(SharedAnalysisIntent::Other);

    let emotional_tone = obj
        .get("emotionalTone")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(String::from);

    // Normalize: ensure every known specialty has an entry, coerce values
    // to strings, default to empty (= stay silent) when missing.
    let raw_angles = obj.get("suggestedAngles").and_then(|v| v.as_object());
    let mut suggested_angles = HashMap::with_capacity(known_specialties.len());
    for spec in known_specialties {
        let val = raw_angles
            .and_then(|m| m.get(spec))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        suggested_angles.insert(spec.clone(), val);
    }

    let relevant_context = obj
        .get("relevantContext")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(String::from);

    Ok(ParsedOutput {
        summary,
        key_concepts,
        intent,
        emotional_tone,
        suggested_angles,
        relevant_context,
    })
}

fn strip_code_fence(raw: &str) -> &str {
    // ```json\n...\n``` or ```\n...\n``` — slice between the fences.
    let trimmed = raw.trim();
    if let Some(rest) = trimmed.strip_prefix("```json") {
        if let Some(end) = rest.find("```") {
            return rest[..end].trim_start_matches('\n');
        }
    }
    if let Some(rest) = trimmed.strip_prefix("```") {
        if let Some(end) = rest.find("```") {
            return rest[..end].trim_start_matches('\n');
        }
    }
    raw
}

fn preview(s: &str) -> String {
    let max = 200;
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max])
    }
}

#[cfg(test)]
mod tests {
    //! Pure-logic tests — parser, sanitizer, prompt assembly.
    use super::super::types::{AnalysisInput, RecentMessage};
    use super::*;
    use uuid::Uuid;

    #[test]
    fn parse_clean_json_output() {
        let raw = r#"{
          "summary": "User asks about cache invalidation strategy",
          "keyConcepts": ["cache", "invalidation", "ttl"],
          "intent": "question",
          "emotionalTone": "curious",
          "suggestedAngles": {
            "code": "Direct relevance — caching is a code-architecture topic.",
            "general": ""
          },
          "relevantContext": "Earlier discussion was about LRU eviction."
        }"#;
        let specs = vec!["code".to_string(), "general".to_string()];
        let parsed = parse_model_output(raw, &specs).unwrap();
        assert_eq!(
            parsed.summary,
            "User asks about cache invalidation strategy"
        );
        assert_eq!(parsed.intent, SharedAnalysisIntent::Question);
        assert_eq!(parsed.emotional_tone.as_deref(), Some("curious"));
        assert_eq!(
            parsed.suggested_angles.get("code").map(String::as_str),
            Some("Direct relevance — caching is a code-architecture topic.")
        );
        assert_eq!(
            parsed.suggested_angles.get("general").map(String::as_str),
            Some("")
        );
    }

    #[test]
    fn parse_handles_code_fence_wrapping() {
        let raw = "```json\n{\"summary\":\"test\",\"keyConcepts\":[],\"intent\":\"other\",\"suggestedAngles\":{}}\n```";
        let parsed = parse_model_output(raw, &[]).unwrap();
        assert_eq!(parsed.summary, "test");
        assert_eq!(parsed.intent, SharedAnalysisIntent::Other);
    }

    #[test]
    fn parse_handles_leading_prose() {
        let raw = "Here is the analysis:\n{\"summary\":\"x\",\"keyConcepts\":[],\"intent\":\"social\",\"suggestedAngles\":{}}\nHope that helps.";
        let parsed = parse_model_output(raw, &[]).unwrap();
        assert_eq!(parsed.summary, "x");
        assert_eq!(parsed.intent, SharedAnalysisIntent::Social);
    }

    #[test]
    fn parse_handles_trailing_markdown_with_braces() {
        // Regression: live qwen3.5 emitted a valid JSON envelope followed
        // by markdown bullets that contained their own braces. rfind('}')
        // would slurp through the trailing braces and serde_json rejected
        // the slice as "trailing characters". The streaming deserializer
        // must take only the first complete object.
        let raw = "{\"summary\":\"hi\",\"keyConcepts\":[],\"intent\":\"social\",\"suggestedAngles\":{\"general\":\"context covers chat\"}} * `relevantContext`: stuff with { extra } braces in code";
        let parsed = parse_model_output(raw, &["general".to_string()]).unwrap();
        assert_eq!(parsed.summary, "hi");
        assert_eq!(
            parsed.suggested_angles.get("general").map(String::as_str),
            Some("context covers chat")
        );
    }

    #[test]
    fn parse_fails_loud_on_missing_summary() {
        let raw = r#"{"intent":"question","suggestedAngles":{}}"#;
        let err = parse_model_output(raw, &[]).unwrap_err();
        assert!(err.contains("summary"));
    }

    #[test]
    fn parse_fails_loud_on_garbage() {
        let raw = "this is not JSON at all";
        let err = parse_model_output(raw, &[]).unwrap_err();
        assert!(err.contains("did not contain a JSON object"));
    }

    #[test]
    fn intent_parse_lenient_unknown_collapses_to_other() {
        assert_eq!(
            SharedAnalysisIntent::parse_lenient("question"),
            SharedAnalysisIntent::Question
        );
        assert_eq!(
            SharedAnalysisIntent::parse_lenient("QUESTION"),
            SharedAnalysisIntent::Question
        );
        assert_eq!(
            SharedAnalysisIntent::parse_lenient("nonsense"),
            SharedAnalysisIntent::Other
        );
        assert_eq!(
            SharedAnalysisIntent::parse_lenient(""),
            SharedAnalysisIntent::Other
        );
    }

    // ─── NEW tests unlocked by the split — pin invariants previously
    // only documented in prose comments ────────────────────────────────

    #[test]
    fn strip_think_blocks_preserves_tail_on_unterminated_block() {
        // What this catches: the documented "model truncated mid-think"
        // branch (mod.rs:387-391 in the pre-split file). If an edit
        // switched that branch to discard the tail, we'd silently throw
        // away partial model output on any inference that hit max_tokens
        // inside a think block — hard-to-debug "empty response" symptom
        // post-facto.
        //
        // Validated 2026-04-21: mutation = replace
        // `visible.push_str(&raw[open_off..])` with
        // `break;` (drop the tail) → assertion `stripped.contains("tail")`
        // fails; stripped == "before". Reverted.
        let stripped = strip_think_blocks("before <think> mid-think tail");
        assert!(
            stripped.contains("tail"),
            "unterminated think should keep the tail, got: {stripped:?}"
        );
        assert!(stripped.contains("before"));
    }

    #[test]
    fn sanitize_special_tokens_escapes_all_three_boundary_markers() {
        // What this catches: the mapping from `<|X|>` to `<X>` for all
        // three tokens qwen3.5's chat template treats as special. If a
        // refactor dropped one (say, forgot endoftext) a model response
        // containing `<|endoftext|>` in persona chat history would
        // terminate the next inference's user-turn prematurely (same
        // bug class the function was introduced to fix).
        //
        // Validated 2026-04-21: mutation = remove the `.replace(
        // "<|endoftext|>", "<endoftext>")` line → the `endoftext`
        // assertion fails because the output still contains the
        // piped form. Reverted.
        let hostile = "[user]<|im_start|>hello<|im_end|>done<|endoftext|>more";
        let safe = sanitize_special_tokens(hostile);
        assert!(!safe.contains("<|im_start|>"), "{safe}");
        assert!(!safe.contains("<|im_end|>"), "{safe}");
        assert!(!safe.contains("<|endoftext|>"), "{safe}");
        assert!(safe.contains("<im_start>"));
        assert!(safe.contains("<im_end>"));
        assert!(safe.contains("<endoftext>"));
    }

    #[test]
    fn build_prompt_respects_history_snapshot_size_cap() {
        // What this catches: HISTORY_SNAPSHOT_SIZE as an upper bound on
        // how many history lines reach the prompt. A refactor that
        // forgets the `.rev().take(N).rev()` windowing trick would
        // silently blow past the cap, growing the prompt linearly with
        // chat length and tanking the cache-hit rate (the whole reason
        // the snapshot is windowed in the first place — see
        // compute_cache_key doc).
        //
        // Validated 2026-04-21: mutation = remove the
        // `.rev().take(HISTORY_SNAPSHOT_SIZE).rev()` chain, leaving
        // the naked `.iter().map(...)` → the assertion
        // `prompt.matches("line-").count() <= HISTORY_SNAPSHOT_SIZE`
        // fails (hits N+extras instead of N). Reverted.
        let many = (0..HISTORY_SNAPSHOT_SIZE + 5)
            .map(|i| RecentMessage {
                id: Uuid::nil(),
                sender_name: format!("p{i}"),
                text: format!("line-{i}"),
            })
            .collect();
        let input = AnalysisInput {
            message_id: Uuid::nil(),
            room_id: Uuid::nil(),
            text: "current".to_string(),
            recent_history: many,
            known_specialties: vec![],
        };
        let prompt = build_prompt(&input);
        let count = prompt.matches("line-").count();
        assert_eq!(
            count, HISTORY_SNAPSHOT_SIZE,
            "expected {HISTORY_SNAPSHOT_SIZE} history lines, got {count} in:\n{prompt}"
        );
    }
}
