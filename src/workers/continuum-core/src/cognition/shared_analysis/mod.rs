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
//!
//! Layout (split 2026-04-21 per the modularize-at-layer-boundaries rule):
//! - `types.rs` — public input types (`RecentMessage`, `AnalysisInput`).
//! - `prompt.rs` — text wrangling: prompt build, parse, sanitize,
//!   SYSTEM_PROMPT, tuning consts, `<think>`-block stripping.
//! - `mod.rs` (this file) — orchestration: `analyze` entry, cache +
//!   single-flight concurrency, inference call, cache-layer tests.

pub mod prompt;
pub mod types;

pub use types::{AnalysisInput, RecentMessage};

use crate::ai::{ChatMessage, MessageContent, TextGenerationRequest};
use crate::cognition::types::SharedAnalysis;
use crate::modules::ai_provider::{generate_text, global_registry};
use dashmap::DashMap;
use once_cell::sync::Lazy;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::Mutex as TokioMutex;

use prompt::{
    build_prompt, parse_model_output, strip_think_blocks, ANALYSIS_MAX_TOKENS,
    ANALYSIS_TEMPERATURE, SYSTEM_PROMPT,
};

/// Per-process cache of analyses, keyed by `cache_key` (content-addressable).
/// DashMap = lock-free concurrent reads; multiple personas hitting the
/// same message read in parallel without serializing.
static ANALYSIS_CACHE: Lazy<Arc<DashMap<String, SharedAnalysis>>> =
    Lazy::new(|| Arc::new(DashMap::new()));

/// In-flight single-flight tracker. When persona A starts analyzing
/// message M and persona B requests the same analysis a few ms later,
/// B awaits A's result instead of firing a second inference. Same
/// shape as PagedResourcePool's load_or_share.
static IN_FLIGHT: Lazy<
    Arc<TokioMutex<HashMap<String, Arc<TokioMutex<Option<Result<SharedAnalysis, String>>>>>>>,
> = Lazy::new(|| Arc::new(TokioMutex::new(HashMap::new())));

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
    let prompt_text = build_prompt(input);

    let request = TextGenerationRequest {
        messages: vec![
            ChatMessage {
                role: "system".to_string(),
                content: MessageContent::Text(SYSTEM_PROMPT.to_string()),
                name: None,
            },
            ChatMessage {
                role: "user".to_string(),
                content: MessageContent::Text(prompt_text),
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
        // Shared analysis is room-wide cognition (not attributable to one
        // persona); registry treats this seq's KV as un-attributed.
        persona_id: None,
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
    let duration_ms = start.elapsed().map(|d| d.as_millis() as u64).unwrap_or(0);

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

#[cfg(test)]
mod tests {
    //! Cache + key tests. Pure-logic tests on the text-wrangling layer
    //! live in `prompt::tests`. End-to-end inference tests happen via
    //! the chat-path validation gate Joel set.
    use super::*;
    use crate::cognition::types::SharedAnalysisIntent;
    use uuid::Uuid;

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

    // ─── NEW tests unlocked by the split — pin cache-layer invariants
    // previously only documented in prose comments ────────────────────

    #[test]
    fn is_stale_honors_cache_ttl_boundary() {
        // What this catches: the CACHE_TTL_MS comparison direction. An
        // inverted operator (`>` → `<`) would treat old entries as
        // fresh and fresh entries as stale — silent serving of stale
        // analyses to personas, with no log signal because the cache
        // layer treats it as a hit. Impacts every persona downstream of
        // shared_cognition. The test fixture constructs a synthetic
        // SharedAnalysis with generated_at_ms at boundaries either side
        // of CACHE_TTL_MS.
        //
        // Validated 2026-04-21: mutation = flip the comparison in
        // `is_stale` from `> CACHE_TTL_MS` to `< CACHE_TTL_MS` → the
        // `fresh` assertion fails (fresh entry now reported as stale)
        // and the `stale` assertion fails (stale entry now reported as
        // fresh). Reverted.
        let now = now_ms();
        let fresh = SharedAnalysis {
            message_id: Uuid::nil(),
            room_id: Uuid::nil(),
            cache_key: "k".to_string(),
            generated_at_ms: now.saturating_sub(CACHE_TTL_MS / 2), // Half-TTL old.
            summary: String::new(),
            key_concepts: vec![],
            intent: SharedAnalysisIntent::Other,
            emotional_tone: None,
            suggested_angles: HashMap::new(),
            relevant_context: None,
            duration_ms: 0,
            model_used: String::new(),
            from_cache: false,
        };
        let stale = SharedAnalysis {
            generated_at_ms: now.saturating_sub(CACHE_TTL_MS + 1_000), // Over TTL + 1s.
            ..fresh.clone()
        };
        assert!(!is_stale(&fresh), "entry half-TTL old should be fresh");
        assert!(is_stale(&stale), "entry over TTL+1s old should be stale");
    }

    // TODO(follow-up): cache_put FIFO eviction invariant. First attempt
    // at this test deadlocked the DashMap under the shared-static setup
    // (parallel test runner + the `while len() > cap; iter().next();
    // remove()` eviction loop). The fix is to extract the eviction logic
    // into a pure `fn enforce_cap(map: &DashMap<...>, cap: usize)` taking
    // the map by reference so tests can drive it on an isolated DashMap.
    // Filed as a separate commit rather than growing this refactor's
    // scope. What the future test should catch: `while → if` mutation
    // letting the cache grow unbounded under burst inserts exceeding the
    // cap by more than 1 (observed 2026-04-19 live).
}
