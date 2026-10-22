//! Cold routes must be repaired before prompt sizing and admission, without a grid tick.
use super::*;
use crate::ai::adapter::AIProviderAdapter;
use crate::ai::heuristic_adapter::HeuristicInferenceAdapter;
use crate::ai::types::TextGenerationRequest;
use crate::inference::airc_remote::peer_breaker::COLD_AFTER_DEADLINES;
use crate::inference::airc_remote::{
    AircRemoteInferenceAdapter, RemoteInferenceError, StubInferenceTransport,
};
use crate::persona::hw_tier_descriptor::HwTierCategory;
use crate::persona::inference_profile::{PersonaInferenceProfile, SamplingProfile};
use crate::persona::scripted_adapter_factory::ScriptedPersonaAdapterFactory;

fn switch(
    factory: Arc<ScriptedPersonaAdapterFactory>,
    remote: Arc<AircRemoteInferenceAdapter>,
) -> PlacementSwitch {
    let profile = PersonaInferenceProfile {
        persona_id: Uuid::new_v4(),
        persona_name: "cold route fixture".into(),
        model_id: "fixture".into(),
        gguf_local_path: None,
        tier_category: HwTierCategory::Compat,
        tier_id: "fixture".into(),
        context_length: 8192,
        n_ubatch: 512,
        n_batch: 512,
        n_seq_max: 1,
        n_gpu_layers: 0,
        sampling: SamplingProfile::chat_defaults(),
        chat_template: None,
        stop_sequences: vec![],
    };
    PlacementSwitch::new(
        profile.persona_id,
        profile.persona_name.clone(),
        Uuid::new_v4(),
        remote,
        factory,
        profile,
        None,
        None,
    )
}

fn remote() -> Arc<AircRemoteInferenceAdapter> {
    Arc::new(AircRemoteInferenceAdapter::new(
        StubInferenceTransport::always_failing(RemoteInferenceError::Timeout { elapsed_ms: 1 }),
    ))
}

async fn trip(remote: &AircRemoteInferenceAdapter) {
    for _ in 0..COLD_AFTER_DEADLINES {
        assert!(remote
            .generate_text(TextGenerationRequest::default())
            .await
            .is_err());
    }
    assert!(remote.is_cold());
}

#[tokio::test]
async fn cold_route_prepares_home_once_and_preserves_breaker(
) -> Result<(), Box<dyn std::error::Error>> {
    let home: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let home_for_factory = Arc::clone(&home);
    let factory = Arc::new(ScriptedPersonaAdapterFactory::custom(move |_| {
        Ok(Arc::clone(&home_for_factory))
    }));
    let remote = remote();
    let switch = switch(Arc::clone(&factory), Arc::clone(&remote));
    let prior_target = switch.generation_target().ok_or("missing remote target")?;
    trip(&remote).await;
    switch.prepare_generation().await;
    let target = switch.generation_target().ok_or("missing home target")?;
    assert_eq!(switch.seat(), Seat::Home);
    assert!(Arc::ptr_eq(&target, &home));
    assert_eq!(target.generation_admission(), home.generation_admission());
    assert_eq!(switch.live_served_window(), home.live_served_window());
    assert!(
        remote.is_cold(),
        "preparation must not reset or replace the breaker"
    );
    assert!(
        !Arc::ptr_eq(&prior_target, &home),
        "already selected calls keep their target"
    );
    switch.prepare_generation().await;
    assert_eq!(factory.build_count(), 1);
    Ok(())
}

#[tokio::test]
async fn warm_route_does_not_materialize_home() {
    let factory = Arc::new(ScriptedPersonaAdapterFactory::heuristic());
    let switch = switch(Arc::clone(&factory), remote());
    switch.prepare_generation().await;
    assert_eq!(switch.seat(), Seat::Remote);
    assert_eq!(factory.build_count(), 0);
}

#[tokio::test]
async fn cold_route_without_home_keeps_honest_refusal() {
    let factory = Arc::new(ScriptedPersonaAdapterFactory::always_fails("no home lane"));
    let remote = remote();
    let switch = switch(Arc::clone(&factory), Arc::clone(&remote));
    trip(&remote).await;
    switch.prepare_generation().await;
    switch.prepare_generation().await;
    assert_eq!(switch.seat(), Seat::Remote);
    assert!(remote.is_cold());
    assert_eq!(
        factory.build_count(),
        1,
        "a failed home build is not a retry spin"
    );
}
