//! Multi-persona stress baseline.
//!
//! Per Joel 2026-06-02: the substrate must run well on M5 with 6-12
//! personas in video chat; on Intel Mac (this machine) at least
//! functional for multiple personas; on typical M-series decently
//! useful and intelligent. We need DATA before guessing at latency
//! vectors — this test exercises the real supervisor +
//! `materialize_adapters` + `serve_persona_loop` pipeline under
//! controlled load and prints structured timings.
//!
//! ## Why this lives outside `#[cfg(test)]` unit tests
//!
//! This is an integration test that intentionally spans multiple
//! substrate layers in one execution — the unit test boundaries are
//! the wrong shape for "what does the end-to-end loop look like
//! with N personas competing for the tokio runtime."
//!
//! ## What it exercises
//!
//! - `materialize_adapters`: N adapters built sequentially (today's
//!   shape per the substrate doctrine of not loading N GGUFs in
//!   parallel on a memory-constrained host).
//! - `HeuristicInferenceAdapter::warmup`: counted via
//!   `ObservedCounts`; should equal N.
//! - `serve_persona_loop`: N concurrent loops, each driven by a
//!   `ScriptedConversation` feeding M messages. Per-turn cost
//!   includes the simulated 50ms adapter delay
//!   (`HeuristicInferenceAdapter::with_delay_ms(50)`).
//! - `tokio` scheduling: N tasks running concurrently; contention
//!   between persona loops is real and load-bearing for the
//!   "decently useful on M-series" target.
//!
//! ## Output shape
//!
//! Structured `eprintln!` lines tagged `stress::*` so Joel can grep
//! them out of test output. Run with:
//!
//! ```bash
//! cargo test --test multi_persona_stress_baseline \
//!     --no-default-features --features livekit-webrtc,llama/mac-cpu-only \
//!     -- --nocapture
//! ```
//!
//! The `--nocapture` is load-bearing — the test PRINTS timings; it
//! doesn't assert specific wall-clock numbers (those vary per host).
//! It DOES assert structural invariants (every persona materializes,
//! every turn replies, warmup counter matches persona count).
//!
//! ## Doctrine
//!
//! Per [[test-fixtures-are-system-primitives]] every fixture here
//! leases the system primitives: no bespoke factories, no bespoke
//! conversations. Per [[init-once-handle-then-lease-zero-copy-refs]]
//! the timings here are what verifies the substrate's claim that
//! cold-start cost lands at boot, not on hot path.

use continuum_core::ai::HeuristicInferenceAdapter;
use continuum_core::modules::persona_instance_manager::PersonaInstanceInfo;
use continuum_core::persona::airc_citizen::StubAircCitizen;
use continuum_core::persona::airc_source::AircTranscriptReader;
use continuum_core::persona::hw_tier_descriptor::HwTierCategory;
use continuum_core::persona::identity_provider::PersonaIdentitySource;
use continuum_core::persona::inference_profile::{
    PersonaInferenceProfile, SamplingProfile,
};
use continuum_core::persona::role_template::RoleId;
use continuum_core::persona::scripted_adapter_factory::ScriptedPersonaAdapterFactory;
use continuum_core::persona::scripted_conversation::ScriptedConversation;
use continuum_core::persona::service_loop::{
    serve_persona_loop, IncomingMessage, PersonaConversation, ServeOptions,
};
use continuum_core::persona::spawner_module::MaterializedPersonaPlan;
use continuum_core::persona::supervisor::{materialize_adapters, HostedPersona};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;

/// Build N planned personas with the LCD profile shape. Same profile
/// every slot — the variation under test is COUNT, not heterogeneity.
fn build_plans(count: usize) -> Vec<MaterializedPersonaPlan> {
    (0..count)
        .map(|i| {
            let persona_id = Uuid::new_v4();
            MaterializedPersonaPlan {
                role: RoleId::Helper,
                instance: PersonaInstanceInfo {
                    persona_id,
                    agent_name: format!("Stress-{i}"),
                    peer_id: Uuid::new_v4(),
                    home: PathBuf::from(format!("/tmp/stress-{i}")),
                    default_room: Uuid::nil(),
                    source: PersonaIdentitySource::FreshlyMinted,
                },
                profile: Ok(PersonaInferenceProfile {
                    persona_id,
                    persona_name: format!("Stress-{i}"),
                    model_id: "stress-model".to_string(),
                    gguf_local_path: None,
                    tier_category: HwTierCategory::Compat,
                    tier_id: "stress_tier".to_string(),
                    context_length: 2048,
                    n_ubatch: 512,
                    n_batch: 2048,
                    n_seq_max: 1,
                    n_gpu_layers: 0,
                    sampling: SamplingProfile::chat_defaults(),
                    chat_template: None,
                    stop_sequences: vec![],
                }),
            }
        })
        .collect()
}

/// Drive one persona's loop with M scripted messages + sentinel
/// `Ok(None)` so the loop ends. Returns the ServeOutcome.
async fn run_persona_loop(
    ctx: HostedPersona,
    message_count: usize,
) -> Result<continuum_core::persona::service_loop::ServeOutcome, String> {
    let other_peer = Uuid::new_v4();
    let events: Vec<_> = (0..message_count)
        .map(|i| {
            Ok(Some(IncomingMessage {
                lamport: (i + 1) as u64,
                peer_id: other_peer,
                text: format!("ping-{i}"),
            }))
        })
        .chain(std::iter::once(Ok(None)))
        .collect();

    let mut conversation = ScriptedConversation::new().with_events(events);
    conversation
        .prime()
        .await
        .map_err(|e| format!("prime failed: {e}"))?;

    let reader: Arc<dyn AircTranscriptReader> =
        Arc::new(StubAircCitizen::new(Uuid::new_v4()));
    let opts = ServeOptions {
        page_recent_limit: 10,
        rag_fetch_limit: 10,
        now_ms: || 1_700_000_000_000,
    };

    serve_persona_loop(&ctx, &mut conversation, reader, opts).await
}

/// Run one stress trial: N personas × M messages each, with a 50ms
/// per-turn adapter delay. Captures wall-clock timings and asserts
/// substrate-correct invariants.
async fn stress_trial(persona_count: usize, message_count: usize) {
    eprintln!(
        "stress::trial begin personas={persona_count} messages={message_count} \
         delay_ms_per_turn=50"
    );

    let plans = build_plans(persona_count);
    let (factory, counts) = ScriptedPersonaAdapterFactory::heuristic_with_counters();

    // Factory closure used by the supervisor for adapter build. We need
    // delay-injecting adapters but ALSO observer counters, so build the
    // closure inline rather than using `heuristic_with_counters` alone.
    let warmups = counts.warmups.clone();
    let generates = counts.generates.clone();
    let factory_with_delay = ScriptedPersonaAdapterFactory::custom(move |_p| {
        Ok(Arc::new(
            HeuristicInferenceAdapter::new()
                .with_delay_ms(50)
                .with_warmup_observer(warmups.clone())
                .with_generate_observer(generates.clone()),
        ))
    });

    // ── PHASE 1: materialize ─────────────────────────────────────
    let materialize_start = Instant::now();
    let hosted = materialize_adapters(
        plans,
        &factory_with_delay,
        StubAircCitizen::fresh_lookup(),
    )
    .await;
    let materialize_elapsed = materialize_start.elapsed();
    let _ = factory; // factory was just for counter ownership; not used in build

    eprintln!(
        "stress::materialize wall_ms={} per_persona_ms={:.2}",
        materialize_elapsed.as_millis(),
        materialize_elapsed.as_secs_f64() * 1000.0 / persona_count as f64,
    );

    // Every slot materialized.
    let materialized: Vec<HostedPersona> = hosted
        .into_iter()
        .enumerate()
        .map(|(i, r)| r.unwrap_or_else(|e| panic!("slot {i} failed: {e:?}")))
        .collect();
    assert_eq!(materialized.len(), persona_count);
    // Warmup ran once per slot.
    assert_eq!(
        counts.warmups(),
        persona_count,
        "warmup ran once per materialized adapter per \
         [[init-once-handle-then-lease-zero-copy-refs]]"
    );

    // ── PHASE 2: concurrent serve loops ──────────────────────────
    let serve_start = Instant::now();
    let mut handles = Vec::with_capacity(persona_count);
    for ctx in materialized {
        handles.push(tokio::spawn(run_persona_loop(ctx, message_count)));
    }
    let mut outcomes = Vec::with_capacity(persona_count);
    for h in handles {
        outcomes.push(h.await.expect("loop join").expect("loop ok"));
    }
    let serve_elapsed = serve_start.elapsed();

    let total_turns: usize = outcomes.iter().map(|o| o.turns_replied).sum();
    let total_errored: usize = outcomes.iter().map(|o| o.turns_errored).sum();
    let total_skipped: usize = outcomes.iter().map(|o| o.turns_skipped).sum();

    eprintln!(
        "stress::serve wall_ms={} total_turns_replied={} total_errored={} total_skipped={}",
        serve_elapsed.as_millis(),
        total_turns,
        total_errored,
        total_skipped,
    );

    // Every persona replied to every message.
    assert_eq!(
        total_turns,
        persona_count * message_count,
        "every persona replied to every message"
    );
    assert_eq!(total_errored, 0, "no errors under controlled load");

    // ── Latency stats per persona ───────────────────────────────
    for (i, o) in outcomes.iter().enumerate() {
        eprintln!(
            "stress::persona[{i}] turns={} mean_ms={:.2} min_ms={} max_ms={}",
            o.turn_latency.count,
            o.turn_latency.mean_ms().unwrap_or(0.0),
            o.turn_latency.min_ms.unwrap_or(0),
            o.turn_latency.max_ms.unwrap_or(0),
        );
    }

    // Aggregate across all personas.
    let agg_total_ms: u64 = outcomes.iter().map(|o| o.turn_latency.total_ms).sum();
    let agg_count: usize = outcomes.iter().map(|o| o.turn_latency.count).sum();
    let agg_mean = if agg_count > 0 {
        agg_total_ms as f64 / agg_count as f64
    } else {
        0.0
    };
    let agg_max: u64 = outcomes
        .iter()
        .filter_map(|o| o.turn_latency.max_ms)
        .max()
        .unwrap_or(0);
    eprintln!(
        "stress::aggregate turns={agg_count} mean_ms={agg_mean:.2} max_ms={agg_max}"
    );

    // Sanity: the 50ms injected delay means per-turn floor is ~50ms.
    // Under tokio contention with N personas competing, expected mean
    // can drift higher. Generous bound — this catches "the substrate
    // went off a cliff" without false-failing on noisy hosts.
    assert!(
        agg_mean >= 30.0,
        "mean ({agg_mean:.2}ms) below floor — adapter delay not being honored?"
    );
    // generates counter sanity
    assert_eq!(
        counts.generates(),
        persona_count * message_count,
        "every persona × every message → exactly one generate call"
    );

    eprintln!("stress::trial end\n");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn baseline_2_personas() {
    stress_trial(2, 10).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn baseline_4_personas() {
    stress_trial(4, 10).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn baseline_8_personas() {
    stress_trial(8, 5).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn baseline_12_personas() {
    stress_trial(12, 5).await;
}
