#![cfg(test)]

use super::*;
use crate::ai::{ChatMessage, MessageContent};
use crate::model_registry::types::{Arch, MultiPartyChatStrategy};
use crate::model_registry::Model;
use std::collections::BTreeSet;

fn lcd_compat_launch() -> LlamaCppLaunch {
    LlamaCppLaunch {
        model_id: "continuum-ai/qwen2.5-0.5b-instruct-GGUF".to_string(),
        gguf_path: PathBuf::from("/tmp/test-qwen2.5-0.5b-instruct-q4_k_m.gguf"),
        context_length: 2048,
        n_seq_max: 1,
        n_ubatch: 512,
        n_gpu_layers: 0,
    }
}

/// `for_launch` produces an adapter with every override field set
/// from the launch. Without this, the substrate's intent-driven
/// guarantee per [[intent-driven-api-not-hot-patches]] breaks:
/// hardcoded defaults silently override what the spawner resolved.
#[test]
fn for_launch_populates_all_overrides_from_the_launch() {
    let launch = lcd_compat_launch();
    let adapter = LlamaCppAdapter::for_launch(&launch);
    assert_eq!(
        adapter.model_path,
        PathBuf::from("/tmp/test-qwen2.5-0.5b-instruct-q4_k_m.gguf")
    );
    assert_eq!(adapter.default_model, launch.model_id);
    assert_eq!(adapter.context_length_override, Some(2048));
    assert_eq!(adapter.n_seq_max_override, Some(1));
    assert_eq!(adapter.n_ubatch_override, Some(512));
    assert_eq!(adapter.n_gpu_layers_override, Some(0));
}

fn text_request(response_format: Option<ResponseFormat>) -> TextGenerationRequest {
    TextGenerationRequest {
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: MessageContent::Text("Return JSON.".to_string()),
            name: None,
        }],
        system_prompt: None,
        model: None,
        provider: None,
        temperature: None,
        max_tokens: None,
        reasoning_budget: None,
        top_p: None,
        top_k: None,
        repeat_penalty: None,
        frequency_penalty: None,
        repeat_last_n: None,
        stop_sequences: None,
        tools: None,
        tail_tools: None,
        view_messages: None,
        tool_choice: None,
        response_format,
        active_adapters: None,
        request_id: None,
        user_id: None,
        room_id: None,
        purpose: None,
        persona_id: None,
        turn_bound: None,
    }
}

fn synthetic_llamacpp_local_model(id: &str, gguf_path: Option<PathBuf>) -> Model {
    Model {
        weights_bytes: None,
        mmproj_bytes: None,
        id: id.into(),
        name: None,
        provider: LLAMACPP_PROVIDER_ID.into(),
        arch: Arch::Qwen35,
        context_window: 32_768,
        max_output_tokens: 4096,
        tokens_per_second: 33.0,
        capabilities: BTreeSet::new(),
        cost_input_per_1k: 0.0,
        cost_output_per_1k: 0.0,
        gguf_hint: None,
        hf_source: None,
        gguf_local_path: gguf_path,
        mmproj_local_path: None,
        chat_template: None,
        multi_party_strategy: MultiPartyChatStrategy::default(),
        stop_sequences: vec![],
        parameter_count: 0,
        sampling: crate::model_registry::types::ModelSampling::default(),
        persona_serving_eligible: true,
        serving: Default::default(), // test/fixture literal: substrate defaults (text-only main lane, unverified kv-shift)
    }
}

#[test]
fn try_new_from_errors_when_no_llamacpp_local_rows() {
    // Empty iterator — no llamacpp-local rows at all (the worst-case
    // install state continuum-8e97 saw on RTX 5090: install seeded
    // only voice-models, registry has no llamacpp-local Qwen row).
    let models: Vec<Model> = vec![];
    match LlamaCppAdapter::try_new_from(models.iter()) {
        Err(err) => {
            assert_eq!(err.provider_id, LLAMACPP_PROVIDER_ID);
            assert_eq!(err.rows_in_registry, 0);
            assert_eq!(err.rows_with_gguf_local_path, 0);
            // Error message must name the actionable next step so
            // operators see what to do (run model-init / seed manually).
            let msg = format!("{err}");
            assert!(
                msg.contains("model-init"),
                "error must name the actionable remediation: {msg}"
            );
        }
        Ok(_) => panic!("expected NoLocalModelLoadable on empty registry"),
    }
}

#[test]
fn json_object_response_format_enables_json_grammar() {
    let sampling =
        sampling_config_from_request(&text_request(Some(ResponseFormat::JsonObject)));
    assert_eq!(sampling.grammar.as_deref(), Some(JSON_GRAMMAR));
}

#[test]
fn text_response_format_leaves_grammar_unconstrained() {
    let sampling = sampling_config_from_request(&text_request(Some(ResponseFormat::Text)));
    assert!(sampling.grammar.is_none());
}

#[test]
fn try_new_from_errors_when_llamacpp_rows_exist_but_none_have_gguf_path() {
    // Registry has llamacpp-local rows but artifact resolver couldn't
    // find the GGUF on disk for any of them — `gguf_local_path` is
    // None for every row. This is the SAME observable state as
    // "registry empty" from the adapter's perspective: nothing to
    // load. Operator-actionable signal must distinguish "registry is
    // wrong" (zero rows) from "files aren't seeded" (rows exist,
    // paths unresolved).
    let models = vec![
        synthetic_llamacpp_local_model("qwen3.5-4b-code-forged-GGUF", None),
        synthetic_llamacpp_local_model("qwen2-vl-7b-instruct", None),
    ];
    match LlamaCppAdapter::try_new_from(models.iter()) {
        Err(err) => {
            assert_eq!(err.provider_id, LLAMACPP_PROVIDER_ID);
            assert_eq!(err.rows_in_registry, 2);
            assert_eq!(err.rows_with_gguf_local_path, 0);
        }
        Ok(_) => panic!("expected NoLocalModelLoadable when no row has gguf_local_path"),
    }
}

// ── n_seq_max coordinator wiring (task #109, step 4) ────────

#[test]
fn n_seq_max_defaults_to_none_for_single_seq_backcompat() {
    // The adapter without a configured override stays in the
    // historical single-seq mode. Qwen3.5 + recurrent / GDN
    // models stay safe; older callers' behavior is unchanged.
    let adapter = LlamaCppAdapter::with_model_id(
        PathBuf::from("/tmp/no-such-file.gguf"),
        "test-model".to_string(),
    );
    assert_eq!(adapter.n_seq_max(), None);
}

#[test]
fn n_seq_max_override_round_trips_through_builder() {
    // Coordinator wiring sets this from
    // CoordinatorConfig.lane_budgets.max_concurrency.
    let adapter = LlamaCppAdapter::with_model_id(
        PathBuf::from("/tmp/no-such-file.gguf"),
        "test-model".to_string(),
    )
    .with_n_seq_max(4);
    assert_eq!(adapter.n_seq_max(), Some(4));
}

#[test]
fn n_seq_max_zero_clamps_to_one() {
    // Zero would be a config error — the backend's scheduler
    // can't serve any seq with n_seq_max=0. Clamping to 1
    // matches the back-compat default and avoids load-time
    // panics from inside llama.cpp.
    let adapter = LlamaCppAdapter::with_model_id(
        PathBuf::from("/tmp/no-such-file.gguf"),
        "test-model".to_string(),
    )
    .with_n_seq_max(0);
    assert_eq!(adapter.n_seq_max(), Some(1));
}

#[test]
fn n_seq_max_builder_composes_with_other_overrides() {
    // Builders should chain — coordinator wiring sets context,
    // KV quant, AND n_seq_max in one builder pipeline.
    let adapter = LlamaCppAdapter::with_model_id(
        PathBuf::from("/tmp/no-such-file.gguf"),
        "test-model".to_string(),
    )
    .with_context_length(16_384)
    .with_n_seq_max(4)
    .with_kv_quant_policy(crate::inference::kv_quant::KvQuantPolicy::default());
    assert_eq!(adapter.n_seq_max(), Some(4));
}

#[test]
fn try_new_from_succeeds_with_at_least_one_resolved_path() {
    // Mixed registry: one row has the path resolved, one doesn't.
    // Adapter should pick the resolved row (matches the existing
    // production behavior of legacy `new()`).
    let resolved_path = PathBuf::from("/tmp/synthetic-test-only.gguf");
    let models = vec![
        synthetic_llamacpp_local_model("qwen3.5-4b-code-forged-GGUF", None),
        synthetic_llamacpp_local_model("qwen2-vl-7b-instruct", Some(resolved_path.clone())),
    ];
    match LlamaCppAdapter::try_new_from(models.iter()) {
        Ok(adapter) => {
            assert_eq!(adapter.model_path, resolved_path);
            assert_eq!(adapter.default_model, "qwen2-vl-7b-instruct");
        }
        Err(err) => panic!("expected Ok with resolved path; got {err:?}"),
    }
}

/// what this catches: that the OWNED in-process page-in (`ensure_adapter`
/// → scheduler `set_loras` before decode, committed d763da1b7) actually
/// LOADS a GGUF-lora produced by our own conversion path
/// (`forge::lora_convert::mlx_to_gguf_lora`) against the real dense base
/// and CHANGES the generated text vs the bare base. This is the last
/// unproven link in the owned genome loop: train (mlx_lm) → transpose
/// (Rust) → GGUF-lora (owned) → PAGE-IN (this). Greedy decode (temp 0) on
/// both arms so any output difference is the gene, not sampling noise.
///
/// `#[ignore]` — loads a ~5.8 GB base + runs two forward passes; needs the
/// metal/accelerate build and the on-disk artifacts. Run explicitly:
///   cargo test -p continuum-core --features metal,accelerate \
///     --lib owned_gene_page_in_changes_output -- --ignored --nocapture
/// Override paths via CONTINUUM_BASE_GGUF / CONTINUUM_GENE_GGUF.
#[tokio::test]
#[ignore]
async fn owned_gene_page_in_changes_output() {
    use crate::ai::adapter::AIProviderAdapter;
    use crate::ai::types::ActiveAdapterRequest;

    let home = std::env::var("HOME").expect("HOME");
    let env_or = |k: &str, d: String| std::env::var(k).unwrap_or(d);
    let base = PathBuf::from(env_or(
        "CONTINUUM_BASE_GGUF",
        format!("{home}/.continuum/models/qwen2.5-coder-3b-instruct-f16.gguf"),
    ));
    let gene = env_or(
        "CONTINUUM_GENE_GGUF",
        format!("{home}/.continuum/forge/gguf-lora/coder-3b-dense.gguf"),
    );
    assert!(base.exists(), "base GGUF missing at {base:?}");
    assert!(
        std::path::Path::new(&gene).exists(),
        "gene GGUF-lora missing at {gene}"
    );

    // A code-write prompt from the gym's wheelhouse — the gene was trained
    // on the coder curriculum, so its influence should surface here.
    let prompt = "Write a Rust function `fn is_palindrome(s: &str) -> bool` \
        that ignores case and non-alphanumeric characters. Reply with only the code.";
    let make_req = |adapters: Option<Vec<ActiveAdapterRequest>>| TextGenerationRequest {
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: MessageContent::Text(prompt.to_string()),
            name: None,
        }],
        system_prompt: None,
        model: None,
        provider: None,
        temperature: Some(0.0), // greedy — isolate the gene from sampling
        max_tokens: Some(256),
        reasoning_budget: None,
        top_p: None,
        top_k: None,
        repeat_penalty: None,
        frequency_penalty: None,
        repeat_last_n: None,
        stop_sequences: None,
        tools: None,
        tail_tools: None,
        view_messages: None,
        tool_choice: None,
        response_format: Some(ResponseFormat::Text),
        active_adapters: adapters,
        request_id: None,
        user_id: None,
        room_id: None,
        purpose: None,
        persona_id: Some(uuid::Uuid::nil().to_string()),
        turn_bound: None,
    };

    // context_length MUST be set explicitly — the scheduler refuses the
    // GGUF's n_ctx_train (262144) fallback (would crush Metal KV alloc).
    let adapter =
        LlamaCppAdapter::with_model_id(base.clone(), "qwen2.5-coder-3b-instruct".to_string())
            .with_context_length(2048);

    let out_base = adapter
        .generate_text(make_req(None))
        .await
        .expect("base generation failed")
        .text;
    let out_gene = adapter
        .generate_text(make_req(Some(vec![ActiveAdapterRequest {
            name: "coder-3b-dense".to_string(),
            path: gene.clone(),
            domain: "code".to_string(),
            scale: 1.0,
        }])))
        .await
        .expect("gene generation failed")
        .text;

    println!("=== BASE ===\n{out_base}\n=== GENE ===\n{out_gene}\n=== END ===");
    assert!(!out_base.trim().is_empty(), "base produced empty output");
    assert!(!out_gene.trim().is_empty(), "gene produced empty output");
    // The gene MUST change greedy output — identical text means set_loras
    // silently no-op'd (the exact failure the owned page-in must not have).
    assert_ne!(
        out_base, out_gene,
        "gene paged in but output is byte-identical to base — set_loras did not apply"
    );
}
