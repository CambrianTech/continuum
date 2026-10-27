#![cfg(test)]

// The LoRA page-in resolver (slice 1 of the Model Endpoint Fabric). These
// exercise the PURE name→server-id matcher; the probe/HTTP + fail-loud
// paths need a live llama-server and are covered by the organism eval, not
// a unit test (the integration path, per RULE 1).

use super::*;

fn catalog() -> Vec<(i64, String)> {
    vec![
        (0, "/genome/coder-4b-keystone/adapter.gguf".to_string()),
        (1, "/genome/asha-selfverify/adapter.gguf".to_string()),
    ]
}

// what this catches: an exact path match resolves to the server's own
// load-index — the field the `"lora":[{id,scale}]` body needs.
#[test]
fn exact_path_resolves_to_server_index() {
    let id = OpenAICompatibleAdapter::match_lora_index(
        &catalog(),
        "asha-selfverify",
        "/genome/asha-selfverify/adapter.gguf",
    );
    assert_eq!(id, Some(1));
}

// what this catches: the common "persona names the short slug, server
// stores the full path" case still resolves via substring on name.
#[test]
fn short_name_resolves_via_substring() {
    let id = OpenAICompatibleAdapter::match_lora_index(&catalog(), "coder-4b-keystone", "");
    assert_eq!(id, Some(0));
}

// what this catches: an adapter the custodian has NOT registered is a
// miss (None) — the caller turns this into a fail-loud, never a silent
// drop. (Silent drop was the original LIFT=0 no-op.)
#[test]
fn unregistered_adapter_is_a_miss() {
    let id = OpenAICompatibleAdapter::match_lora_index(&catalog(), "does-not-exist", "");
    assert_eq!(id, None);
}

// what this catches (charter S1): a gene loaded at runtime is listed under its
// staged --train-dir name, not its genome path; without this arm the request
// for it misses and fails loud although the engine holds it.
#[test]
fn a_runtime_loaded_gene_resolves_by_its_staged_name() {
    let gene = "/genome/kimi-coursework/adapter.gguf";
    let stage = crate::inference::llama_server::gene_stage_name(std::path::Path::new(gene))
        .expect("stageable");
    let mut loaded = catalog();
    loaded.push((2, format!("/home/x/.continuum/cache/engine-train/{stage}")));
    assert_eq!(OpenAICompatibleAdapter::match_lora_index(&loaded, "kimi", gene), Some(2));
}

// what this catches: an empty catalog (server loaded no adapters at
// launch) never spuriously matches.
#[test]
fn empty_catalog_never_matches() {
    assert_eq!(
        OpenAICompatibleAdapter::match_lora_index(&[], "anything", "/some/path"),
        None
    );
}
