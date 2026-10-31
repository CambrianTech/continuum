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
    let cycle = registry.register_from_cfg(cfg_for(id, "Ivar")).expect("resident");
    let (original, original_model) = cycle.current_model_route().expect("binding");
    let airc = crate::identity::citizen_path::citizen_home_path(root.path(), crate::identity::IdentityKind::Persona, None, "Ivar");
    let home = crate::cognition::home::PersonaHome::from_root(airc.parent().expect("parent").to_path_buf());
    let remote = PersonaModelOverride::new_remote("remote/model", None, 1, Uuid::new_v4().to_string());
    remote.write(&home).expect("remote override");
    let replacement: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    for corrupt in [false, true] {
        if corrupt { std::fs::write(home.model_override_json(), b"{").expect("corrupt"); }
        assert_eq!(registry.re_home_all_at(root.path(), Some(replacement.clone()), "new/local", 4096), 0);
        let (actual, model) = cycle.current_model_route().expect("preserved");
        assert!(Arc::ptr_eq(&original, &actual));
        assert_eq!(model, original_model);
    }
    PersonaModelOverride::new("local/model", None, 2).write(&home).expect("local override");
    assert_eq!(registry.re_home_all_at(root.path(), Some(replacement.clone()), "new/local", 4096), 1);
    let (actual, model) = cycle.current_model_route().expect("rebound");
    assert!(Arc::ptr_eq(&replacement, &actual));
    assert_eq!(model.as_deref(), Some("new/local"));
}

// what this catches (the M5, 2026-10-29, core 83c000606): residents materialized while the
// engine was still loading kept their provisional 2048 window, and the reconciler keyed
// only on the model name, so the first ready snapshot never corrected it; working memory
// budgeted 2048*4 chars and discarded every result body past ~4k. The served window alone
// must rebind a local resident (same model, her own adapter), a resident already bound to
// (model, window) must cost nothing, and an off-box resident must keep her binding.
#[tokio::test]
async fn a_resident_bound_at_the_provisional_window_is_rehomed_to_the_served_window() {
    use crate::cognition::persona_workspace::RehomeReason;
    crate::persona::cognition_extensions::install();
    let root = tempfile::tempdir().expect("root");
    let _native = crate::paths::NativeHomeOverride::install(root.path());
    let registry = PersonaWorkspaceRegistry::new();
    let local = registry.register_from_cfg(cfg_for(Uuid::new_v4(), "Aiko")).expect("local resident");
    let remote = registry.register_from_cfg(cfg_for(Uuid::new_v4(), "Kira")).expect("remote resident");
    let airc = crate::identity::citizen_path::citizen_home_path(root.path(), crate::identity::IdentityKind::Persona, None, "Kira");
    let home = crate::cognition::home::PersonaHome::from_root(airc.parent().expect("parent").to_path_buf());
    PersonaModelOverride::new_remote("remote/model", None, 1, Uuid::new_v4().to_string()).write(&home).expect("remote override");
    let before = local.current_binding().expect("local binding");
    let remote_before = remote.current_binding().expect("remote binding");
    let model = before.requested_model().to_string();
    assert_eq!(before.context_window, crate::inference::planning::MIN_SERVE_CTX, "bound at the provisional window");
    let served = 60_672;

    assert_eq!(registry.rehome_reason_at(root.path(), &model, served), Some(RehomeReason::WindowChanged));
    assert_eq!(registry.re_home_all_at(root.path(), None, &model, served), 1, "only the local resident moves");
    let after = local.current_binding().expect("rebound");
    assert_eq!(after.context_window, served);
    assert_eq!(after.requested_model(), model);
    assert!(Arc::ptr_eq(&before.adapter, &after.adapter), "a window-only correction keeps her adapter");
    let remote_after = remote.current_binding().expect("remote kept");
    assert!(Arc::ptr_eq(&remote_before, &remote_after), "the off-box resident keeps her binding");

    assert_eq!(registry.rehome_reason_at(root.path(), &model, served), None, "steady state needs nothing");
    assert_eq!(registry.re_home_all_at(root.path(), None, &model, served), 0, "already bound: untouched");
    assert!(Arc::ptr_eq(&after, &local.current_binding().expect("still bound")), "no store on a matching binding");
}

/// A resident's brain config: the deterministic heuristic adapter, no grounding, no hands.
fn cfg_for(persona_id: uuid::Uuid, name: &str) -> crate::cognition::persona_workspace::PersonaBrainConfig {
    crate::cognition::persona_workspace::PersonaBrainConfig {
        persona_id,
        persona_name: name.to_string(),
        system_prompt: format!("You are {name}, an engineer on the grid."),
        admission: crate::cognition::act_observe::test_support::admission(),
        adapter: std::sync::Arc::new(crate::ai::HeuristicInferenceAdapter::new()),
        grounding_sources: Vec::new(),
        embedder: None,
        tool_executor: None,
        experience: None,
        context_window: crate::inference::planning::MIN_SERVE_CTX,
        defer_recall: false,
        defer_grounding: false,
        suppress_recall: false,
        forked: false,
        read_receipt: None,
    }
}
