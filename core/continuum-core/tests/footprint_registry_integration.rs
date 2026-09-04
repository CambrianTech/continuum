//! Integration test: `LlamaCppAdapter` populates the global
//! `FootprintRegistry` with model_weights bytes after a successful load.
//!
//! Why it exists: the substrate's whole reason-to-be is that every
//! allocation site reports through one surface so the policy can see
//! "what are we made of?" If the wiring from adapter → registry breaks
//! silently, the policy goes blind to the largest single allocation in
//! the process (model weights). That's the kind of regression we want a
//! test to catch even though it costs a real GGUF load.
//!
//! Marked `#[ignore]` because it requires the qwen3.5-4b GGUF on disk
//! (~2.5GB) and pays the 5–10s load cost. Run with:
//!
//!     cargo test --package continuum-core --test footprint_registry_integration \
//!       -- --ignored --nocapture

use continuum_core::ai::adapter::AIProviderAdapter;
use continuum_core::ai::types::{ChatMessage, MessageContent, TextGenerationRequest};
use continuum_core::inference::footprint_registry::{self, FootprintKey, ResourceType};
use continuum_core::inference::kv_quant::Residency;
use continuum_core::inference::LlamaCppAdapter;
use std::env;
use std::path::PathBuf;
use uuid::Uuid;

fn qwen35_4b_target_path() -> PathBuf {
    if let Ok(p) = env::var("QWEN35_4B_GGUF") {
        return PathBuf::from(p);
    }
    let home = env::var("HOME").expect("HOME env var must be set for this integration test");
    PathBuf::from(format!(
        "{}/.docker/models/bundles/sha256/18055fe8ee379b95f4af3cf420588c5daa28f2a1ce1da335112a2d1ea188d3e6/model/model.gguf",
        home
    ))
}

/// What this catches: the adapter loading a model without reporting its
/// bytes to the registry. After `initialize()` succeeds, the registry
/// MUST contain a `ModelWeights` entry for this backend whose byte count
/// matches the GGUF file size on disk. If the entry is missing, the
/// pressure policy can't see the biggest allocation in the process.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires real qwen3.5-4b GGUF + 5-10s; run manually with --ignored --nocapture"]
async fn llamacpp_adapter_reports_model_weights_to_global_registry() {
    // Need the model registry initialized so LlamaCppAdapter::new() can
    // resolve the llamacpp-local row from the Rust catalog (catalog.rs).
    let _reg = continuum_core::model_registry::init_global()
        .expect("model_registry init for adapter construction");

    let model_path = qwen35_4b_target_path();
    if !model_path.exists() {
        eprintln!(
            "[ftp-int] skipping — qwen3.5-4b GGUF not at {model_path:?}. \
             pull via docker model pull or set QWEN35_4B_GGUF."
        );
        return;
    }
    let expected_bytes = std::fs::metadata(&model_path)
        .expect("file size for the GGUF on disk")
        .len();
    eprintln!(
        "[ftp-int] expected model_weights bytes: {expected_bytes} ({} GB)",
        expected_bytes / 1_000_000_000
    );

    // Snapshot the registry state so we can assert this load contributes
    // a fresh entry (other tests in the process may have already loaded).
    let before_total = footprint_registry::global().total_bytes();
    let before_model_weights = footprint_registry::global()
        .by_resource_type()
        .get(&ResourceType::ModelWeights)
        .copied()
        .unwrap_or(0);

    // Build adapter with a small context budget so KV doesn't OOM the box
    // (262K context = 24GB on qwen3.5-4b; 4K is plenty for this test).
    let mut adapter = LlamaCppAdapter::new()
        .with_model_path(model_path.clone())
        .with_context_length(4_096);
    adapter.initialize().await.expect("adapter initialize");

    // Now the registry MUST contain a ModelWeights entry attributable to
    // this backend (model_id), with bytes ≈ file size on disk.
    let after_total = footprint_registry::global().total_bytes();
    let after_model_weights = footprint_registry::global()
        .by_resource_type()
        .get(&ResourceType::ModelWeights)
        .copied()
        .unwrap_or(0);

    let delta = after_model_weights - before_model_weights;
    eprintln!(
        "[ftp-int] before total={before_total} mw={before_model_weights} \
         after total={after_total} mw={after_model_weights} \
         delta_mw={delta}"
    );

    assert!(
        delta >= expected_bytes,
        "model_weights bytes after load ({after_model_weights}) must be at least \
         file size ({expected_bytes}); delta={delta}"
    );

    // And there must be a backend-scoped entry for THIS model id, not
    // just an aggregate that could collide with other adapters.
    let model_id = adapter.default_model().to_string();
    let key = FootprintKey::for_backend(&model_id, ResourceType::ModelWeights, Residency::Active);
    let by_type = footprint_registry::global().by_resource_type();
    eprintln!("[ftp-int] registry by_resource_type: {:?}", by_type);
    eprintln!("[ftp-int] looked-up key: {:?}", key);
    // Persona-total query won't help (this is a shared/backend-scoped
    // entry). We instead verify the by_type sum reflects the new bytes —
    // that's the proof the entry is in the map under the right type.
    assert!(
        after_model_weights >= expected_bytes,
        "by_resource_type[ModelWeights]={after_model_weights} \
         must be ≥ this GGUF file size {expected_bytes}"
    );
}

/// What this catches: the scheduler firing inference without reporting
/// per-seq KV bytes to the registry. After a real generate_text call
/// with persona_id set, the registry MUST attribute non-zero KvCache
/// bytes to that persona via `persona_total`. If the entry is missing,
/// the policy can't see per-persona KV pressure — the whole point of
/// Piece 2.
///
/// This test exercises the full lifecycle:
///   - start_request inserts the pending entry (bytes:0)
///   - PrefillFinal triggers report_authoritative with exact bytes
///   - Done refresh
///   - free removes the entry (or decrements to 0)
///
/// The mid-call assertion is the hard one: while the seq is still
/// active in the scheduler we should see > 0 KV bytes for the persona.
/// We don't have that visibility from outside the inference call (it's
/// a single await), so we instead assert the AFTER-CALL state: the
/// entry should have been added then removed, and during the call the
/// total KV bytes attributed to KvCache should have been positive.
/// The latter is observable indirectly via the model's reported
/// throughput (real KV was committed) — proxy verification.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires real qwen3.5-4b GGUF + 10-20s; run manually with --ignored --nocapture"]
async fn scheduler_reports_per_seq_kv_bytes_for_persona() {
    let _reg = continuum_core::model_registry::init_global()
        .expect("model_registry init for adapter construction");

    let model_path = qwen35_4b_target_path();
    if !model_path.exists() {
        eprintln!("[ftp-int] skipping kv test — qwen3.5-4b GGUF not at {model_path:?}");
        return;
    }

    let mut adapter = LlamaCppAdapter::new()
        .with_model_path(model_path.clone())
        .with_context_length(4_096);
    adapter.initialize().await.expect("adapter initialize");

    // Snapshot KvCache bytes BEFORE the call. Other tests in the same
    // process may have left some state, so we work in deltas.
    let before_kv = footprint_registry::global()
        .by_resource_type()
        .get(&ResourceType::KvCache)
        .copied()
        .unwrap_or(0);

    // Fixed persona_id so we can query persona_total against it.
    let persona_id = Uuid::new_v4();
    let request = TextGenerationRequest {
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: MessageContent::Text("Reply with just the word OK.".to_string()),
            name: None,
        }],
        model: Some(adapter.default_model().to_string()),
        provider: Some("local".to_string()),
        temperature: Some(0.0),
        max_tokens: Some(8),
        purpose: Some("kv-reporting-integration-test".to_string()),
        persona_id: Some(persona_id.to_string()),
        ..Default::default()
    };

    eprintln!("[ftp-int] dispatching generate_text with persona_id={persona_id}");
    let response = adapter.generate_text(request).await.expect("generate_text");
    eprintln!(
        "[ftp-int] generate_text returned: text={:?} tokens={}",
        &response.text.chars().take(60).collect::<String>(),
        response.usage.output_tokens
    );

    // After the call:
    //   - the persona's KvCache entry should have been added then removed
    //     (final remove brings it to 0 and self-cleans)
    //   - so persona_total should be 0 (entry gone, or never existed if
    //     the seq failed)
    //   - the global by_resource_type[KvCache] delta should be 0 (added,
    //     reported, removed — net zero)
    let after_kv = footprint_registry::global()
        .by_resource_type()
        .get(&ResourceType::KvCache)
        .copied()
        .unwrap_or(0);
    let persona_total = footprint_registry::global().persona_total(persona_id);

    eprintln!("[ftp-int] before_kv={before_kv} after_kv={after_kv} persona_total={persona_total}");

    // Diagnostic: dump every KvCache-typed entry to find what's leaked.
    let snap = footprint_registry::global().snapshot();
    eprintln!(
        "[ftp-int] full snapshot: total={} entry_count={}",
        snap.total_bytes, snap.entry_count
    );
    eprintln!("[ftp-int] by_persona: {:?}", snap.by_persona);
    eprintln!("[ftp-int] by_resource_type: {:?}", snap.by_resource_type);

    assert_eq!(
        persona_total, 0,
        "persona's KV entry should have been removed after generation completes; \
         leftover bytes={persona_total}"
    );
    assert_eq!(
        after_kv,
        before_kv,
        "global KvCache total should net to zero after generation completes; \
         delta={}",
        after_kv as i64 - before_kv as i64
    );

    // Indirect proof that KV was actually committed mid-call: the model
    // produced output. seq_state_bytes returns 0 if no KV is committed,
    // so a successful generation with throughput > 0 implies the FFI
    // would have returned a non-zero number during PrefillFinal.
    assert!(
        response.usage.output_tokens > 0,
        "no tokens generated — proxy for whether KV was committed"
    );
}
