#![cfg(test)]
//! From `persona::remote_lane_factory`: core's `AircModule`, built from a healthy discovery,
//! registers this node's id before any persona build, so a durable override naming this
//! node retires and she is born home. From `cognition::persona_workspace`: a re-home keeps a
//! resident's binding until her assignment reads as local. Each asserts exactly what it
//! asserted before its move.

use std::sync::atomic::Ordering;

use std::sync::Arc;

use uuid::Uuid;

use crate::ai::adapter::AIProviderAdapter;
use crate::ai::HeuristicInferenceAdapter;
use crate::cognition::persona_workspace::PersonaWorkspaceRegistry;
use crate::persona::model_override::PersonaModelOverride;
use crate::persona::remote_lane_factory::test_support::*;
use crate::persona::supervisor::PersonaAdapterFactory;

// what this catches (card 2500d2f1, the M5 2026-09-19): a durable override naming
// one of THIS node's own ids as her remote seat. With no airc attached a remote seat
// would REFUSE (the test above); a seat that is herself must instead be retired and
// she born home — the local factory reached once, the record gone so the next boot
// does not bear her on a loopback lane again.
#[tokio::test]
async fn an_override_naming_this_node_is_retired_at_build_and_she_is_born_home() {
    let tmp = tempfile::tempdir().unwrap();
    let (f, calls) = factory_over(tmp.path()); // cell is empty: a real remote seat would refuse here
    let home = home_for(tmp.path(), "Aris");
    // The id enters the self set the way it does at boot: the airc module built from
    // a Healthy discovery registers the id this node beacons under — before any
    // persona build, before any echo (Cormac's condition on #4229).
    let me = uuid::Uuid::new_v4();
    let _module = crate::modules::airc::AircModule::from_discovery(&crate::airc::AircDiscovery::Healthy {
        socket: tmp.path().join("airc.sock"),
        default_room: airc_core::RoomId::from_uuid(uuid::Uuid::new_v4()),
        room_name: "general".to_string(),
        peer_id: me,
    });
    PersonaModelOverride::new_remote("ggml-org/Qwen3.8-27B-GGUF", Some("placement:fleet".to_string()), 1, me.to_string())
        .write(&home)
        .expect("write override");

    let _ = f.build_adapter(&profile("Aris")).await;
    assert_eq!(calls.load(Ordering::SeqCst), 1, "born home: the local factory is reached once");
    assert!(
        PersonaModelOverride::load(&home).expect("readable").is_none(),
        "the record naming her own node as a seat is retired"
    );
}

// what this catches (from cognition::persona_workspace, mind split step 7): serving-edge rehome must preserve a resident's binding when its assignment is corrupt.
#[tokio::test]
async fn rehome_requires_a_readable_local_assignment() {
    crate::persona::cognition_extensions::install(); // her remote binding is persona's to read
    let root = tempfile::tempdir().expect("root");
    let _native = crate::paths::NativeHomeOverride::install(root.path());
    let registry = PersonaWorkspaceRegistry::new();
    let id = Uuid::new_v4();
    let cycle = registry.register_from_cfg(cfg_for(id)).expect("resident");
    let (original, original_model) = cycle.current_model_route().expect("binding");
    let airc = crate::identity::citizen_path::citizen_home_path(root.path(), crate::identity::IdentityKind::Persona, None, "Ivar");
    let home = crate::cognition::home::PersonaHome::from_root(airc.parent().expect("parent").to_path_buf());
    let remote = PersonaModelOverride::new_remote("remote/model", None, 1, Uuid::new_v4().to_string());
    remote.write(&home).expect("remote override");
    let replacement: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    for corrupt in [false, true] {
        if corrupt { std::fs::write(home.model_override_json(), b"{").expect("corrupt"); }
        assert_eq!(registry.re_home_all_at(root.path(), replacement.clone(), Some("new/local".into()), 4096), 0);
        let (actual, model) = cycle.current_model_route().expect("preserved");
        assert!(Arc::ptr_eq(&original, &actual));
        assert_eq!(model, original_model);
    }
    PersonaModelOverride::new("local/model", None, 2).write(&home).expect("local override");
    assert_eq!(registry.re_home_all_at(root.path(), replacement.clone(), Some("new/local".into()), 4096), 1);
    let (actual, model) = cycle.current_model_route().expect("rebound");
    assert!(Arc::ptr_eq(&replacement, &actual));
    assert_eq!(model.as_deref(), Some("new/local"));
}

/// A resident's brain config: the deterministic heuristic adapter, no grounding, no hands.
fn cfg_for(persona_id: uuid::Uuid) -> crate::cognition::persona_workspace::PersonaBrainConfig {
    crate::cognition::persona_workspace::PersonaBrainConfig {
        persona_id,
        persona_name: "Ivar".to_string(),
        system_prompt: "You are Ivar, an engineer on the grid.".to_string(),
        admission: crate::cognition::act_observe::test_support::admission(),
        adapter: std::sync::Arc::new(crate::ai::HeuristicInferenceAdapter::new()),
        capacity: None,
        grounding_sources: Vec::new(),
        embedder: None,
        tool_executor: None,
        experience: None,
        context_window: crate::inference::planning::MIN_SERVE_CTX,
        defer_recall: false,
        defer_grounding: false,
        suppress_recall: false,
        forked: false,
    }
}
