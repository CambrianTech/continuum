//! Shared Analysis — the verb that produces `SharedAnalysis`.
//!
//! ONE inference per chat message instead of N per persona. Base model,
//! no LoRA, no specialty bias — produces the objective ground floor
//! every responding persona shares. See `SHARED-COGNITION.md`.
//!
//! Why Rust: lock-free DashMap cache, true SHA-256 hashing, async
//! single-flight (concurrent personas analyzing the same message
//! collapse into one inference), zero-copy output via cache_key
//! reference. None of this expressible in TS without hand-waving.

use crate::ai::{ChatMessage, MessageContent, TextGenerationRequest};
use crate::cognition::types::{SharedAnalysis, SharedAnalysisIntent};
use crate::modules::ai_provider::{generate_text, global_registry};
use dashmap::DashMap;
use once_cell::sync::Lazy;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::Mutex as TokioMutex;
use uuid::Uuid;

/// Per-process cache of analyses, keyed by `cache_key` (content-addressable).
/// DashMap = lock-free concurrent reads; multiple personas hitting the
/// same message read in parallel without serializing.
static ANALYSIS_CACHE: Lazy<Arc<DashMap<String, SharedAnalysis>>> =
    Lazy::new(|| Arc::new(DashMap::new()));

/// In-flight single-flight tracker. When persona A starts analyzing
/// message M and persona B requests the same analysis a few ms later,
/// B awaits A's result instead of firing a second inference. Same
/// shape as PagedResourcePool's load_or_share.
static IN_FLIGHT: Lazy<Arc<TokioMutex<HashMap<String, Arc<TokioMutex<Option<Result<SharedAnalysis, String>>>>>>>> =
    Lazy::new(|| Arc::new(TokioMutex::new(HashMap::new())));

/// Cache size cap. Old entries evicted FIFO when over.
const CACHE_MAX_ENTRIES: usize = 200;

/// Stale after 5 minutes — chat moves; old analysis stops representing
/// the conversation state. Same TTL pattern as the embedding cache used.
const CACHE_TTL_MS: u64 = 5 * 60 * 1000;

/// Default model for shared analysis. The base local model — no LoRA,
/// no specialty bias. Today there's no runtime LoRA composition in
/// the inference path (genome paging is page-only), so "base model" =
/// the default DMR model the personas already use. When runtime LoRA
/// composition lands, this call explicitly opts out via no
/// `active_adapters` field on the request.
const DEFAULT_ANALYSIS_MODEL: &str = "continuum-ai/qwen3.5-4b-code-forged-GGUF";
const DEFAULT_ANALYSIS_PROVIDER: &str = "local";

/// Recent-history snapshot size used in the analysis prompt + cache key.
/// Bigger = more context for analysis but smaller cache hit rate (each
/// new message changes the snapshot). 5 messages is a reasonable middle.
const HISTORY_SNAPSHOT_SIZE: usize = 5;

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
const ANALYSIS_MAX_TOKENS: u32 = 2500;

/// Lower temperature than persona renders — we want consistent,
/// reliable structured output, not creative variation. Personas bring
/// the creativity in their render passes.
const ANALYSIS_TEMPERATURE: f32 = 0.2;

/// What the analyzer needs to know about a recent message. Minimal
/// shape so the service doesn't have to know about ChatMessageEntity.
#[derive(Debug, Clone)]
pub struct RecentMessage {
    pub id: Uuid,
    pub sender_name: String,
    pub text: String,
}

/// Input to `analyze`. Caller (chat path / orchestrator) collects these
/// from the room state.
#[derive(Debug, Clone)]
pub struct AnalysisInput {
    pub message_id: Uuid,
    pub room_id: Uuid,
    /// The new message that triggered this analysis.
    pub text: String,
    /// Recent messages for context. Most-recent last.
    pub recent_history: Vec<RecentMessage>,
    /// Stable specialty identifiers in the room (e.g. ['code',
    /// 'education', 'general']). Caller pulls from the room's
    /// persona registry. The analyzer is told to produce a
    /// `suggested_angles` entry for each.
    pub known_specialties: Vec<String>,
}

/// Run or retrieve the cached SharedAnalysis for a chat message.
///
/// Concurrent calls for the same `cache_key` collapse into a single
/// inference via `IN_FLIGHT` — persona A starts analyzing, persona B
/// awaits the same future, both get the same result.
///
/// Returns `Err` if the model output can't be parsed into the contract
/// shape — failing loud is right; silent fallback to a degraded
/// analysis would mask a real model regression.
pub async fn analyze(input: AnalysisInput) -> Result<SharedAnalysis, String> {
    let cache_key = compute_cache_key(&input);

    // L1 hit: return immediately, mark from_cache for telemetry.
    if let Some(cached) = ANALYSIS_CACHE.get(&cache_key) {
        if !is_stale(&cached) {
            let mut hit = cached.clone();
            hit.from_cache = true;
            return Ok(hit);
        }
        // Stale: drop and fall through to re-analysis.
        drop(cached);
        ANALYSIS_CACHE.remove(&cache_key);
    }

    // Single-flight: if another caller is already analyzing this same
    // input, await their result. Otherwise become the analyzer.
    let slot = {
        let mut inflight = IN_FLIGHT.lock().await;
        if let Some(existing) = inflight.get(&cache_key) {
            existing.clone()
        } else {
            let new_slot: Arc<TokioMutex<Option<Result<SharedAnalysis, String>>>> =
                Arc::new(TokioMutex::new(None));
            inflight.insert(cache_key.clone(), new_slot.clone());
            // Mark THIS task as the analyzer.
            drop(inflight);
            // Run inference + parse, store result in slot, then remove
            // from in-flight map so future cache misses re-analyze.
            let result = run_analysis(&input, &cache_key).await;
            *new_slot.lock().await = Some(result.clone());
            IN_FLIGHT.lock().await.remove(&cache_key);
            // Cache successful results only — failed parses don't poison.
            if let Ok(ref analysis) = result {
                cache_put(cache_key.clone(), analysis.clone());
            }
            return result;
        }
    };

    // Awaiter path: another task is the analyzer; wait for its slot.
    // Loop because the slot might be taken but result not yet stored.
    loop {
        if let Some(result) = slot.lock().await.clone() {
            return result;
        }
        // Tiny yield — the analyzer is in flight. In practice the lock
        // hand-off above means one wake-up is enough.
        tokio::task::yield_now().await;
    }
}

/// Stable hash of (room + current message + sorted specialty list).
///
/// Deliberately EXCLUDES recent_history. The whole point of single-flight
/// here is N personas analyzing the SAME inbound message coalesce into ONE
/// inference. Including history defeats that — each persona's RAG produces
/// slightly different conversationHistory (per-persona excludeMessageIds,
/// per-persona memory injection, per-persona budget trimming) → different
/// hash → 4 separate inferences instead of 1 + 3 awaiters → DMR's single
/// slot can't keep up → 3 personas fail with empty responses (caught
/// 2026-04-19, Round 11 chat showed Helper + CodeReview erroring while
/// Local Assistant succeeded — symptom of the cache key being too granular).
///
/// Specialties stay in the key because they DO change which angles the
/// analysis must populate. Personas in the same room should always have the
/// same sorted specialty set, so this still coalesces correctly.
fn compute_cache_key(input: &AnalysisInput) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.room_id.as_bytes());
    hasher.update(b"|");
    hasher.update(input.text.as_bytes());
    hasher.update(b"|");
    let mut sorted_specs = input.known_specialties.clone();
    sorted_specs.sort();
    for s in &sorted_specs {
        hasher.update(s.as_bytes());
        hasher.update(b",");
    }
    format!("{:x}", hasher.finalize())
}

fn is_stale(analysis: &SharedAnalysis) -> bool {
    now_ms().saturating_sub(analysis.generated_at_ms) > CACHE_TTL_MS
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

async fn run_analysis(input: &AnalysisInput, cache_key: &str) -> Result<SharedAnalysis, String> {
    let start = SystemTime::now();
    let prompt = build_prompt(input);

    let request = TextGenerationRequest {
        messages: vec![
            ChatMessage {
                role: "system".to_string(),
                content: MessageContent::Text(SYSTEM_PROMPT.to_string()),
                name: None,
            },
            ChatMessage {
                role: "user".to_string(),
                content: MessageContent::Text(prompt),
                name: None,
            },
        ],
        system_prompt: None,
        model: Some(DEFAULT_ANALYSIS_MODEL.to_string()),
        provider: Some(DEFAULT_ANALYSIS_PROVIDER.to_string()),
        temperature: Some(ANALYSIS_TEMPERATURE),
        max_tokens: Some(ANALYSIS_MAX_TOKENS),
        top_p: None,
        top_k: None,
        repeat_penalty: None,
        stop_sequences: None,
        tools: None,
        tool_choice: None,
        // FORCE JSON OUTPUT. llama.cpp / DMR constrain the sampler so the
        // model can only emit valid JSON. Eliminates qwen3.5's thinking-mode
        // prose that broke the parser. The right way to enforce structured
        // output: at the model level, not via parser fallbacks.
        response_format: Some(crate::ai::types::ResponseFormat::JsonObject),
        active_adapters: None, // Explicit no-LoRA. Stays opted-out when runtime composition lands.
        request_id: None,
        user_id: None,
        room_id: Some(input.room_id.to_string()),
        purpose: Some("shared-cognition-analysis".to_string()),
    };

    // Acquire the registry read lock for the duration of the call.
    let registry = global_registry();
    let registry_guard = registry.read().await;
    let response = generate_text(&registry_guard, request).await?;

    // qwen3.5-family models emit <think>...</think> reasoning before the
    // user-visible output. parse_model_output wants the JSON envelope; if
    // we feed it the raw response, the leading <think> trips the JSON
    // detector and we fail the whole analysis. Strip thinks first so the
    // parser sees the actual structured output.
    let stripped = strip_think_blocks(&response.text);
    let parsed = parse_model_output(&stripped, &input.known_specialties)?;
    let duration_ms = start
        .elapsed()
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    Ok(SharedAnalysis {
        message_id: input.message_id,
        room_id: input.room_id,
        cache_key: cache_key.to_string(),
        generated_at_ms: now_ms(),
        summary: parsed.summary,
        key_concepts: parsed.key_concepts,
        intent: parsed.intent,
        emotional_tone: parsed.emotional_tone,
        suggested_angles: parsed.suggested_angles,
        relevant_context: parsed.relevant_context,
        duration_ms,
        model_used: response.model,
        from_cache: false,
    })
}

/// User-message prompt. Compact, structured, asks for specific JSON shape.
/// Tolerant parsing on the receiving side handles minor model deviations.
fn build_prompt(input: &AnalysisInput) -> String {
    let history_lines: Vec<String> = input
        .recent_history
        .iter()
        .rev()
        .take(HISTORY_SNAPSHOT_SIZE)
        .rev()
        .map(|m| format!("{}: {}", m.sender_name, m.text))
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
        message = input.text,
        specialties = specialties,
    )
}

/// Parsed-from-JSON intermediate shape (private — public type is
/// `SharedAnalysis`).
#[derive(Debug)]
struct ParsedOutput {
    summary: String,
    key_concepts: Vec<String>,
    intent: SharedAnalysisIntent,
    emotional_tone: Option<String>,
    suggested_angles: HashMap<String, String>,
    relevant_context: Option<String>,
}

/// Strip `<think>...</think>` blocks from raw model output. qwen3.5-family
/// and other reasoning models emit think blocks before the user-visible
/// content; downstream parsers expect the clean tail. Returns the text
/// with think blocks elided and leading/trailing whitespace trimmed. No
/// event emission here — that's `persona::response::strip_thinks_emit_events`
/// which wraps this for the render path. Analysis never needs events.
fn strip_think_blocks(raw: &str) -> String {
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

fn parse_model_output(raw: &str, known_specialties: &[String]) -> Result<ParsedOutput, String> {
    // Strip code fences if the model wrapped its JSON.
    let candidate = strip_code_fence(raw).trim();

    // Find the first '{' — tolerates leading prose.
    let obj_start = candidate.find('{').ok_or_else(|| {
        format!(
            "model output did not contain a JSON object. Got: {}",
            preview(raw)
        )
    })?;

    // Stream-parse the first complete JSON value starting at obj_start.
    // Why: rfind('}') would slurp trailing markdown that contains its own
    // braces (e.g. `{"a":"b"} * code with { x } block`) and then
    // serde_json rejects it as "trailing characters". The streaming
    // deserializer stops at the first complete value and ignores the rest,
    // which is the correct behavior for a model that occasionally tacks
    // on prose after the JSON envelope.
    let tail = &candidate[obj_start..];
    let mut stream = serde_json::Deserializer::from_str(tail).into_iter::<serde_json::Value>();
    let parsed: serde_json::Value = stream
        .next()
        .ok_or_else(|| {
            format!(
                "model output did not contain a JSON object. Got: {}",
                preview(raw)
            )
        })?
        .map_err(|e| {
            format!(
                "model output was not valid JSON: {e}. Got: {}",
                preview(tail)
            )
        })?;

    let obj = parsed.as_object().ok_or_else(|| {
        format!("model output was not a JSON object. Got: {}", preview(tail))
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
    let raw_angles = obj
        .get("suggestedAngles")
        .and_then(|v| v.as_object());
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

fn cache_put(key: String, analysis: SharedAnalysis) {
    ANALYSIS_CACHE.insert(key, analysis);
    // Approximate FIFO eviction when over cap. DashMap doesn't preserve
    // insertion order so this isn't true LRU; for the chat cadence
    // (a few entries per minute) it's good enough — full LRU can swap
    // in via PagedResourcePool when pressure becomes meaningful.
    while ANALYSIS_CACHE.len() > CACHE_MAX_ENTRIES {
        if let Some(entry) = ANALYSIS_CACHE.iter().next() {
            let oldest_key = entry.key().clone();
            drop(entry);
            ANALYSIS_CACHE.remove(&oldest_key);
        } else {
            break;
        }
    }
}

/// Test-only accessor for cache state.
#[cfg(test)]
pub fn _test_clear_cache() {
    ANALYSIS_CACHE.clear();
}

/// Test-only accessor for cache size.
#[cfg(test)]
pub fn _test_cache_size() -> usize {
    ANALYSIS_CACHE.len()
}

const SYSTEM_PROMPT: &str = "You are an objective conversation analyzer.\n\
Read the user message in its conversation context.\n\
Produce a JSON analysis that other AI personas will use as the SHARED foundation for their responses.\n\
\n\
Be objective. Be concise. Do NOT respond to the message; analyze it.\n\
You are not a participant in the conversation; you are the analyst.\n\
\n\
Output ONLY the JSON object. No prose before or after. No code fences.";

#[cfg(test)]
mod tests {
    //! Pure-logic tests — no inference calls. Validate parser, cache
    //! key stability, and intent parsing. End-to-end inference tests
    //! happen via the chat-path validation gate Joel set.
    use super::*;

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
        assert_eq!(parsed.summary, "User asks about cache invalidation strategy");
        assert_eq!(parsed.intent, SharedAnalysisIntent::Question);
        assert_eq!(parsed.emotional_tone.as_deref(), Some("curious"));
        assert_eq!(parsed.suggested_angles.get("code").map(String::as_str), Some("Direct relevance — caching is a code-architecture topic."));
        assert_eq!(parsed.suggested_angles.get("general").map(String::as_str), Some(""));
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
        assert_eq!(parsed.suggested_angles.get("general").map(String::as_str), Some("context covers chat"));
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
        assert_eq!(SharedAnalysisIntent::parse_lenient("question"), SharedAnalysisIntent::Question);
        assert_eq!(SharedAnalysisIntent::parse_lenient("QUESTION"), SharedAnalysisIntent::Question);
        assert_eq!(SharedAnalysisIntent::parse_lenient("nonsense"), SharedAnalysisIntent::Other);
        assert_eq!(SharedAnalysisIntent::parse_lenient(""), SharedAnalysisIntent::Other);
    }

    #[test]
    fn cache_key_is_deterministic() {
        let input = AnalysisInput {
            message_id: Uuid::nil(),
            room_id: Uuid::nil(),
            text: "hello".to_string(),
            recent_history: vec![],
            known_specialties: vec!["code".to_string(), "general".to_string()],
        };
        let k1 = compute_cache_key(&input);
        let k2 = compute_cache_key(&input);
        assert_eq!(k1, k2);
    }

    #[test]
    fn cache_key_differs_on_message_change() {
        let mut a = AnalysisInput {
            message_id: Uuid::nil(),
            room_id: Uuid::nil(),
            text: "hello".to_string(),
            recent_history: vec![],
            known_specialties: vec!["code".to_string()],
        };
        let k1 = compute_cache_key(&a);
        a.text = "goodbye".to_string();
        let k2 = compute_cache_key(&a);
        assert_ne!(k1, k2);
    }

    #[test]
    fn cache_key_stable_under_specialty_reorder() {
        let a = AnalysisInput {
            message_id: Uuid::nil(),
            room_id: Uuid::nil(),
            text: "hello".to_string(),
            recent_history: vec![],
            known_specialties: vec!["code".to_string(), "general".to_string()],
        };
        let b = AnalysisInput {
            known_specialties: vec!["general".to_string(), "code".to_string()],
            ..a.clone()
        };
        // Specialties are sorted before hashing → reorder is the same key.
        assert_eq!(compute_cache_key(&a), compute_cache_key(&b));
    }
}
