#![cfg(test)]

use super::*;
use crate::ai::inference_error::InferenceError;
use crate::inference::llama_server::ServingSnapshot;

const MAIN: &str = "http://127.0.0.1:8080/v1";
const SIDECAR: &str = "http://127.0.0.1:8090/v1";

fn gateway(base: &str) -> OpenAICompatibleAdapter {
    OpenAICompatibleAdapter::new(OpenAICompatibleConfig {
        provider_id: crate::inference::llama_server::PROVIDER_ID.into(),
        base_url: base.into(),
        tail_tools: true,
        capabilities: [crate::model_registry::Capability::ToolUse].into_iter().collect(),
        ..test_adapter().config
    })
}

/// The main lane serves `mind`; when `eyes_on_sidecar`, a vision sidecar serves `eyes`.
fn serving(active: &str, eyes_on_sidecar: bool) -> ServingSnapshot {
    ServingSnapshot {
        active_model: Some(active.into()),
        ready: true,
        base_url: MAIN.into(),
        vision_ready: eyes_on_sidecar,
        vision_base_url: eyes_on_sidecar.then(|| SIDECAR.into()),
        vision_model: eyes_on_sidecar.then(|| "eyes".into()),
        ..ServingSnapshot::empty()
    }
}

// what this catches (Codex on #5491): the capability decided for the main lane while the request
// goes elsewhere. A request for the vision sidecar's model is routed to `vision_base_url`, an
// engine whose build the gateway reconcile never read, so it must not carry tail verbs; nor may a
// dedicated lane, a provider outside the fork family, or a lane whose build is not verified.
#[test]
fn tail_tools_are_decided_for_the_engine_the_request_reaches() {
    let snap = serving("mind", true);
    let adapter = gateway(MAIN);
    let base = adapter.endpoints();
    let main = adapter.route_for("mind", &base, &snap, true);
    assert!(main.tail_tools && main.base.root() == OpenAiBase::new(MAIN).root(), "the main lane, verified");
    let eyes = adapter.route_for("eyes", &base, &snap, true);
    assert!(!eyes.tail_tools && eyes.base.root() == OpenAiBase::new(SIDECAR).root(), "routed to the unverified sidecar");
    assert!(!adapter.route_for("mind", &base, &snap, false).tail_tools, "the main lane's build is not verified");
    let dedicated = gateway(MAIN).with_dedicated_lane();
    assert!(!dedicated.route_for("mind", &dedicated.endpoints(), &snap, true).tail_tools, "a dedicated lane");
    let other = gateway("http://127.0.0.1:9999/v1");
    assert!(!other.route_for("mind", &other.endpoints(), &snap, true).tail_tools, "another server");
    let cloud = OpenAICompatibleAdapter::new(OpenAICompatibleConfig { base_url: MAIN.into(), ..test_adapter().config });
    assert!(!cloud.route_for("mind", &cloud.endpoints(), &snap, true).tail_tools, "outside the fork-gateway family");
}

// what this catches (Codex on #5491): a TRANSITION between selection and send. Her head was built
// while `eyes` was served by the main lane (tail accepted); by send time the sidecar serves it.
// The send decides URL and gate from ONE snapshot, so the pair is consistent (sidecar, no tail),
// and the tail she was promised is refused for a rebuild instead of being silently dropped.
#[test]
fn a_route_that_moves_between_selection_and_send_refuses_the_tail_never_drops_it() {
    let adapter = gateway(MAIN);
    let base = adapter.endpoints();
    let at_selection = adapter.route_for("eyes", &base, &serving("eyes", false), true);
    assert!(at_selection.tail_tools, "selected while the main lane served her model");

    let at_send = adapter.route_for("eyes", &base, &serving("mind", true), true);
    assert_eq!(at_send.base.root(), OpenAiBase::new(SIDECAR).root(), "the send goes to the sidecar now");
    assert!(!at_send.tail_tools, "and the gate comes from the same snapshot as that URL");

    let spec = |name: &str| crate::ai::types::NativeToolSpec {
        name: name.to_owned(),
        description: format!("{name} does one thing"),
        input_schema: crate::ai::types::ToolInputSchema {
            schema_type: "object".to_owned(),
            properties: serde_json::json!({}),
            required: None,
            definitions: None,
        },
    };
    let request = TextGenerationRequest {
        tools: Some(vec![spec("commands_help")]),
        tail_tools: Some(vec![spec("code_git_apply")]),
        ..Default::default()
    };
    let mut body = serde_json::json!({ "model": "eyes" });
    let sent = crate::inference::request_body::finish_body(&adapter.config, &request, "eyes", at_send.tail_tools, &mut body);
    assert_eq!(sent, Err(InferenceError::TailToolsUnreachable { model: "eyes".into() }));
    assert!(!InferenceError::TailToolsUnreachable { model: "eyes".into() }.is_retryable_unchanged(), "rebuilt, never resent as is");
}
