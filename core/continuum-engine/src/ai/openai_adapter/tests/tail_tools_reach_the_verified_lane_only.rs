#![cfg(test)]

use super::*;
use crate::inference::llama_server::ServingSnapshot;

fn gateway(base: &str) -> OpenAICompatibleAdapter {
    OpenAICompatibleAdapter::new(OpenAICompatibleConfig {
        provider_id: crate::inference::llama_server::PROVIDER_ID.into(),
        base_url: base.into(),
        tail_tools: true,
        ..test_adapter().config
    })
}

// what this catches (Codex on #5491): the capability decided for the main lane while the request
// goes elsewhere. A request for the vision sidecar's model is routed to `vision_base_url`, an
// engine whose build the gateway reconcile never read, so it must not carry tail verbs; nor may a
// dedicated lane, a provider outside the fork family, or a lane whose build is not verified.
#[test]
fn tail_tools_are_decided_for_the_engine_the_request_reaches() {
    let snap = ServingSnapshot {
        active_model: Some("mind".into()),
        ready: true,
        base_url: "http://127.0.0.1:8080/v1".into(),
        vision_ready: true,
        vision_base_url: Some("http://127.0.0.1:8090/v1".into()),
        vision_model: Some("eyes".into()),
        ..ServingSnapshot::empty()
    };
    let adapter = gateway("http://127.0.0.1:8080/v1");
    assert!(adapter.tail_tools_reach("mind", &snap, true), "the main lane, verified");
    assert!(!adapter.tail_tools_reach("eyes", &snap, true), "routed to the unverified sidecar");
    assert!(!adapter.tail_tools_reach("mind", &snap, false), "the main lane's build is not verified");
    assert!(!gateway("http://127.0.0.1:8080/v1").with_dedicated_lane().tail_tools_reach("mind", &snap, true), "a dedicated lane");
    assert!(!gateway("http://127.0.0.1:9999/v1").tail_tools_reach("mind", &snap, true), "another server");
    let cloud = OpenAICompatibleAdapter::new(OpenAICompatibleConfig { base_url: "http://127.0.0.1:8080/v1".into(), ..test_adapter().config });
    assert!(!cloud.tail_tools_reach("mind", &snap, true), "outside the fork-gateway family");
}
