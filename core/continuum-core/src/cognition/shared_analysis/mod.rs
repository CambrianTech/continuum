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

pub mod error;
pub mod prompt;
pub mod types;

pub use error::AnalysisError;
pub use types::{AnalysisInput, RecentMessage};

use crate::ai::{ChatMessage, MessageContent, TextGenerationRequest};
use crate::cognition::types::SharedAnalysis;
use crate::concurrency::{ConcurrencyPolicy, TokioConcurrencyPolicy};
use crate::modules::ai_provider::{generate_text, global_registry};
use dashmap::DashMap;
use futures::FutureExt;
use once_cell::sync::Lazy;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::SystemTime;

use prompt::{
    build_prompt, parse_model_output, strip_think_blocks, ANALYSIS_TEMPERATURE, SYSTEM_PROMPT,
};

/// Per-process cache of analyses, keyed by `cache_key` (content-addressable).
/// DashMap = lock-free concurrent reads; multiple personas hitting the
/// same message read in parallel without serializing.
static ANALYSIS_CACHE: Lazy<Arc<DashMap<String, SharedAnalysis>>> =
    Lazy::new(|| Arc::new(DashMap::new()));

/// Shared single-flight policy. When persona A starts analyzing message M and
/// persona B requests the same analysis a few ms later, B awaits A's result
/// instead of firing a second inference.
static ANALYSIS_CONCURRENCY: Lazy<
    Arc<dyn ConcurrencyPolicy<String, SharedAnalysis, AnalysisError>>,
> = Lazy::new(|| Arc::new(TokioConcurrencyPolicy::new()));

/// Cache size cap. Old entries evicted FIFO when over.
const CACHE_MAX_ENTRIES: usize = 200;

/// Stale after 5 minutes — chat moves; old analysis stops representing
/// the conversation state. Same TTL pattern as the embedding cache used.
const CACHE_TTL_MS: u64 = 5 * 60 * 1000;

/// Run or retrieve the cached SharedAnalysis for a chat message.
///
/// Concurrent calls for the same `cache_key` collapse into a single
/// inference via `IN_FLIGHT` — persona A starts analyzing, persona B
/// awaits the same future, both get the same result.
///
/// Returns `Err(AnalysisError)` if the model output can't be parsed
/// into the contract shape — failing loud is right; silent fallback
/// to a degraded analysis would mask a real model regression. Typed
/// error so callers can pattern-match on the failure mode (#1207):
///   - MissingEnvelope: model emitted prose, not JSON
///   - MissingField / EmptyField: structural shape OK but content gap
///   - InferenceFailed: provider-side failure (timeout, API error, etc.)
pub async fn analyze(input: AnalysisInput) -> Result<SharedAnalysis, AnalysisError> {
    // Fast path: shared analysis exists FOR orchestration across
    // specialties. When there's no orchestration to do (room has
    // <=1 known specialty, e.g. a single-persona substrate or a
    // private 1:1 turn), the LLM inference is pure waste — empty
    // suggested_angles is the right answer and we can synthesize
    // it without paying ~50s on Intel CPU. Per
    // [[intent-driven-api-not-hot-patches]] + Joel 2026-06-03
    // "make this fast and intelligent, even with the dumber llms":
    // the substrate's job is to skip work that doesn't change the
    // outcome.
    //
    // Multi-persona rooms ALWAYS go through the real inference
    // path so the orchestrator can score specialties against the
    // model's actual concept extraction. Single-persona rooms get
    // the no-op stub. ResponseOrchestrator handles empty
    // suggested_angles gracefully (one-specialty rooms have no
    // angle to inject — the render proceeds with system_prompt
    // alone).
    // RTOS-debugger breakpoint: analyze entry. The subconscious
    // stage gating every persona's "should I weigh in" decision.
    // Captures the input fingerprint so an operator can correlate
    // multiple personas hitting the same single-flight cache.
    // Per docs/architecture/RTOS-DEBUGGER-PROBES.md class taxonomy.
    crate::probe!(
        class = "cognition.analyze.enter",
        message_id = %input.message_id,
        room_id = %input.room_id,
        text_len = input.text.len(),
        history_count = input.recent_history.len(),
        known_specialties_count = input.known_specialties.len(),
        "analyze entry"
    );

    if input.known_specialties.len() <= 1 {
        // RTOS-debugger breakpoint: the substrate skipped the LLM
        // call because a single-specialty room has nothing for
        // orchestration to do. The persona's render proceeds with
        // empty suggested_angles. Critical to distinguish "I chose
        // silence because no angle matched" vs "I chose silence
        // because analyze was skipped" — they look identical
        // downstream without this probe.
        crate::probe!(
            class = "cognition.analyze.noop_single_specialty",
            message_id = %input.message_id,
            specialties_seen = ?input.known_specialties,
            "skipped LLM — single-specialty room"
        );
        let now = now_ms();
        return Ok(SharedAnalysis {
            message_id: input.message_id,
            room_id: input.room_id,
            cache_key: format!("noop-{}", input.message_id),
            generated_at_ms: now,
            summary: input.text.clone(),
            key_concepts: Vec::new(),
            intent: crate::cognition::types::SharedAnalysisIntent::Other,
            emotional_tone: None,
            suggested_angles: std::collections::HashMap::new(),
            relevant_context: None,
            duration_ms: 0,
            model_used: "noop-single-persona".to_string(),
            from_cache: false,
        });
    }

    let cache_key = compute_cache_key(&input);

    // L1 hit: return immediately, mark from_cache for telemetry.
    if let Some(cached) = ANALYSIS_CACHE.get(&cache_key) {
        if !is_stale(&cached) {
            let mut hit = cached.clone();
            hit.from_cache = true;
            // RTOS-debugger breakpoint: cache hit means N-1 personas
            // in this room skip the LLM call entirely — one of the
            // substrate's biggest correctness/perf wins. If hit-rate
            // is low across a run, the cache key is too granular
            // (continuum#1206 history-inclusion bug).
            crate::probe!(
                class = "cognition.analyze.cache_hit",
                message_id = %input.message_id,
                cache_key = %cache_key,
                model_used = %hit.model_used,
                angles_count = hit.suggested_angles.len(),
                "L1 cache hit"
            );
            return Ok(hit);
        }
        // Stale: drop and fall through to re-analysis.
        drop(cached);
        ANALYSIS_CACHE.remove(&cache_key);
    }

    // Single-flight via the shared concurrency policy. The policy owns
    // the Shared<BoxFuture> map; this module only supplies the analysis
    // work and successful-result cache publication.
    let input = Arc::new(input);
    let result = ANALYSIS_CONCURRENCY
        .single_flight(cache_key.clone(), {
            let input = Arc::clone(&input);
            let cache_key = cache_key.clone();
            async move {
                let result = run_analysis(&input, &cache_key).await;
                if let Ok(ref analysis) = result {
                    cache_put(cache_key, analysis.clone());
                }
                result
            }
            .boxed()
        })
        .await;

    result
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

async fn run_analysis(
    input: &AnalysisInput,
    cache_key: &str,
) -> Result<SharedAnalysis, AnalysisError> {
    let start = SystemTime::now();
    let prompt_text = build_prompt(input);

    // Model binding — the SINGLE source of truth (#76, Joel 2026-07-16 smell hunt):
    // the caller's `model_override` wins ([[intent-driven-api-not-hot-patches]]);
    // otherwise resolve the DISCOVERED served model from the serving daemon's
    // published ServingSnapshot — NEVER a hardcoded id that may not exist on this
    // misfit host. Mirrors `generate_response.rs` (the proven un-hardcoding); fails
    // loud if nothing is served ([[fallbacks-are-illegal-fail-loud]]). This also
    // fixes a live bug: the old hardcoded `qwen3.5-4b` + `provider:"local"` were
    // rejected downstream whenever the resident model was anything else (the
    // `single_resident_model` guard), silently failing analysis.
    let model = crate::cognition::inference_session::resolve_model(input.model_override.clone())
        .await
        .map_err(|e| AnalysisError::from_inference(e.to_string()))?;

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
        // Resolved above from the single source (caller override → served snapshot).
        model: Some(model),
        // The llama-server gateway is the sole local inference path (the in-process
        // "local" adapter is gated off). One source of truth for the gateway id,
        // same as generate_response — NOT the stale hardcoded "local" that would
        // hard-fail select() the moment anything actually routed through it.
        provider: Some(crate::inference::llama_server::PROVIDER_ID.to_string()),
        temperature: Some(ANALYSIS_TEMPERATURE),
        // Model owns its length (None → adapter forwards no ceiling). This call
        // is the canonical proof of why: a flat cap (was 500, bumped to 2500)
        // truncated qwen3.5 mid-reasoning → zero JSON → silent persona failure.
        // The model runs to its own stop token; the JSON envelope is the bound.
        max_tokens: None,
        top_p: None,
        top_k: None,
        repeat_penalty: None,
        frequency_penalty: None,
        repeat_last_n: None,
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
    // Provider-side errors are opaque strings (the provider has its
    // own typed-error space we don't want to leak). Wrap into the
    // typed InferenceFailed variant so callers can pattern-match.
    let response = generate_text(&registry_guard, request)
        .await
        .map_err(AnalysisError::from_inference)?;

    // qwen3.5-family models emit <think>...</think> reasoning before the
    // user-visible output. parse_model_output wants the JSON envelope; if
    // we feed it the raw response, the leading <think> trips the JSON
    // detector and we fail the whole analysis. Strip thinks first so the
    // parser sees the actual structured output.
    let stripped = strip_think_blocks(&response.text);
    let parsed = parse_model_output(&stripped, &input.known_specialties)?;
    let duration_ms = start.elapsed().map(|d| d.as_millis() as u64).unwrap_or(0);

    // RTOS-debugger breakpoint: the analyze LLM finished and we
    // parsed its output. Surfaces the per-specialty angle decision
    // (count of empty vs non-empty angles) — the key signal
    // shaping which personas the orchestrator picks as responders.
    // If at LCD tier every angle comes back non-empty for trivial
    // messages, this probe is where the diagnosis starts.
    let empty_angles: usize = parsed
        .suggested_angles
        .values()
        .filter(|v| v.is_empty())
        .count();
    let non_empty_angles = parsed.suggested_angles.len().saturating_sub(empty_angles);
    crate::probe!(
        class = "cognition.analyze.parse",
        message_id = %input.message_id,
        model_used = %response.model,
        analyze_duration_ms = duration_ms,
        angles_total = parsed.suggested_angles.len(),
        angles_non_empty = non_empty_angles,
        angles_empty = empty_angles,
        intent = ?parsed.intent,
        summary_len = parsed.summary.len(),
        key_concepts_count = parsed.key_concepts.len(),
        "parsed analyze output"
    );

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
    use std::collections::HashMap;
    use uuid::Uuid;

    #[test]
    fn cache_key_is_deterministic() {
        let input = AnalysisInput {
            message_id: Uuid::nil(),
            room_id: Uuid::nil(),
            text: "hello".to_string(),
            recent_history: vec![],
            known_specialties: vec!["code".to_string(), "general".to_string()],
            model_override: None,
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
            model_override: None,
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
            model_override: None,
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
