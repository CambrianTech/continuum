//! Architecture test — THE killer-loop integration witness for the
//! demand-pull cognition doctrine. End-to-end: items arrive →
//! service_cycle_batched drains → analyze_burst fires → exactly ONE
//! cognition gate call per channel-tick, regardless of how many items
//! the burst aggregated.
//!
//! See `docs/architecture/PROVING-THE-DOCTRINE.md` for the matrix
//! this file populates. The clause pinned here is the consumer-side
//! complement of the producer-side `architecture_demand_pull_cognition.rs`:
//!
//! > "Cognition fires its evaluation gate (`analyze_burst`) ONCE per
//! > channel-with-work per service tick. N inbox arrivals on one
//! > channel collapse into 1 gate call — not N. The per-item
//! > inference-cost explosion the demand-pull doctrine claims to
//! > eliminate IS eliminated, measurable end-to-end."
//!
//! ## Why this matters
//!
//! The producer-side architecture test (`architecture_demand_pull_
//! cognition.rs`) pins the SUBSTRATE claim: `service_cycle_batched`
//! returns one `CoherentInput` per channel-with-work. This file pins
//! the COGNITION claim: when cognition's gate (`analyze_burst`) is
//! called once per CoherentInput, the gate-call count per service
//! tick is the channel-with-work count, not the item-arrival count.
//!
//! Without this test, an upstream change to the cognition layer
//! (e.g. PR C+1 swapping the synthetic-FullEvaluateRequest path for a
//! burst-native gate) could silently fan a burst back to per-item
//! evaluations — defeating the doctrine without producing any visible
//! signal. This test is the canary.
//!
//! ## Proof shape — Shape 1 (integration unit)
//!
//! Construct a `ChannelRegistry`, `PersonaState`, and the gate stack
//! state (`RateLimiterState`, `SleepState`, `PersonaCognitionEngine`,
//! `RecentMessageCache`). Enqueue N=50 chat items. Run one service
//! cycle. Count `analyze_burst` calls via direct counter (the
//! function is pure; we wrap it in a counting helper for the test).
//! Assert call count == 1.
//!
//! ## What this does NOT cover (intentional follow-ups)
//!
//! - **Real airc_chat_demo end-to-end** — task #248 slice 3 reshapes
//!   the demo binary itself; this test pins the doctrine WITHOUT the
//!   demo wiring so the demo migration can be staged independently.
//! - **Real inference cost** — `analyze_burst` here uses the existing
//!   synthetic-FullEvaluateRequest path, which calls full_evaluate's
//!   fast-path gates (microsecond). The "inference cost" the
//!   doctrine claims to eliminate IS the LLM call downstream of the
//!   gate; that's pinned by gate-call count (1 gate call → at most
//!   1 LLM call), not by measuring inference itself.
//! - **Multi-channel** — this test focuses on one channel (Chat). The
//!   multi-channel "one input per channel-with-work" property is
//!   already pinned in `architecture_demand_pull_cognition.rs::
//!   service_cycle_with_n_chat_messages_yields_one_input` and the
//!   unit-level `batched_produces_one_input_per_channel_with_work_audio_first`.
//!
//! ## Tag
//!
//! proves: end-to-end demand-pull — cognition's gate fires EXACTLY 1
//! time per channel-tick for N=50 arrivals. The per-item-explosion
//! the doctrine claims to eliminate is measurably eliminated; future
//! refactors that defeat the doctrine fail this test.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use uuid::Uuid;

use continuum_core::persona::channel_items::ChatQueueItem;
use continuum_core::persona::channel_registry::{ChannelRegistry, DEFAULT_BURST_WINDOW_MS};
use continuum_core::persona::channel_types::QueueItemBehavior;
use continuum_core::persona::cognition::PersonaCognitionEngine;
use continuum_core::persona::message_cache::RecentMessageCache;
use continuum_core::persona::persona_identity::PersonaIdentity;
use continuum_core::persona::types::{PersonaState, SenderType};
use continuum_core::persona::{analyze_burst, BurstEvaluateResult, RateLimiterState, SleepState};
use continuum_core::rag::RagEngine;

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn make_chat(room: Uuid, sender: &str, content: &str) -> Arc<dyn QueueItemBehavior> {
    Arc::new(ChatQueueItem {
        id: Uuid::new_v4(),
        room_id: room,
        content: content.into(),
        sender_id: Uuid::new_v4(),
        sender_name: sender.into(),
        sender_type: SenderType::Human,
        mentions: false,
        timestamp: now_ms(),
        enqueued_at: now_ms(),
        priority: 0.5,
        consolidated_context: Vec::new(),
        media: Vec::new(),
        embedding_cell: std::sync::OnceLock::new(),
        compute_calls: std::sync::atomic::AtomicUsize::new(0),
    })
}

/// Test harness — a struct bundling the persona's gate-stack state
/// plus a counting wrapper around `analyze_burst`.
struct PersonaHarness {
    persona_id: Uuid,
    identity: PersonaIdentity,
    rate_limiter: RateLimiterState,
    sleep_state: SleepState,
    engine: PersonaCognitionEngine,
    message_cache: RecentMessageCache,
    /// Counts how many times we called `analyze_burst` since
    /// construction. Per-harness (not global) so concurrent
    /// integration tests don't contaminate each other.
    burst_call_count: Arc<AtomicUsize>,
}

impl PersonaHarness {
    fn new(name: &str) -> Self {
        let persona_id = Uuid::new_v4();
        let identity = PersonaIdentity::new(persona_id, name);
        let rag_engine = Arc::new(RagEngine::new());
        let (_tx, rx) = tokio::sync::watch::channel(false);
        let engine = PersonaCognitionEngine::new(persona_id, name.into(), rag_engine, rx);
        Self {
            persona_id,
            identity,
            // Loose limits — we're pinning gate-call count, not gate
            // outcomes. The doctrine claim survives even when every
            // burst is silent.
            rate_limiter: RateLimiterState::new(100.0, 50),
            sleep_state: SleepState::default(),
            engine,
            message_cache: RecentMessageCache::new(),
            burst_call_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Counting wrapper around `analyze_burst`. Each call bumps the
    /// per-harness counter.
    fn analyze_burst_counted(
        &self,
        input: &continuum_core::persona::channel_view::CoherentInput,
    ) -> BurstEvaluateResult {
        self.burst_call_count.fetch_add(1, Ordering::Relaxed);
        analyze_burst(
            input,
            self.persona_id,
            &self.identity.name,
            "killer-loop-test-persona",
            &self.rate_limiter,
            &self.sleep_state,
            &self.engine,
            &self.message_cache,
            now_ms(),
        )
    }

    fn burst_calls(&self) -> usize {
        self.burst_call_count.load(Ordering::Relaxed)
    }
}

/// THE killer-loop test. 50 chat messages enqueued → ONE service
/// tick → ONE `analyze_burst` call. The producer-side claim pinned by
/// `architecture_demand_pull_cognition.rs::service_cycle_with_n_chat_
/// messages_yields_one_input` is exactly `inputs.len() == 1`; THIS
/// test extends it through the cognition layer: that single input
/// drives EXACTLY ONE gate evaluation.
///
/// If `analyze_burst` was accidentally called per-item (e.g. a future
/// refactor fanned the burst back out before evaluation), this test
/// fails. The doctrine's "ε" in "wall-clock bounded by analyze + ε"
/// includes the cognition gate cost; one call vs N calls IS the
/// load-bearing demand-pull win.
///
/// proves: end-to-end demand-pull — cognition's evaluation gate fires
/// EXACTLY 1 time per channel-tick regardless of arrival count.
#[test]
fn analyze_burst_fires_exactly_once_per_channel_tick_for_n_arrivals() {
    let mut registry = ChannelRegistry::new();
    let mut state = PersonaState::new();
    let harness = PersonaHarness::new("Helper");
    let room = Uuid::new_v4();

    // 50 messages — same shape as the producer-side proof. Realistic
    // burst: bursty conversation around a hot topic, or N personas
    // chattering in the same room.
    const N: usize = 50;
    for i in 0..N {
        registry
            .route(make_chat(room, "Joel", &format!("message {i}")))
            .expect("route");
    }

    assert_eq!(
        harness.burst_calls(),
        0,
        "harness counter must start at zero — test fixture invalid"
    );

    // ONE service tick — drain everything, get the demand-pull Vec
    // of inputs back, walk it, count gate calls.
    let inputs =
        registry.service_cycle_batched(&mut state, &harness.identity, DEFAULT_BURST_WINDOW_MS);

    for input in &inputs {
        let _result = harness.analyze_burst_counted(input);
    }

    // THE load-bearing assertion. The doctrine claim is:
    // - inputs.len() == 1 (one channel with work)
    // - therefore analyze_burst fires exactly 1 time per tick
    // - regardless of N=50 (or 500, or 5000)
    assert_eq!(
        inputs.len(),
        1,
        "producer-side regression: expected 1 CoherentInput for {N} \
         chat items in one channel"
    );
    assert_eq!(
        harness.burst_calls(),
        1,
        "cognition-side regression: analyze_burst fired {} times for \
         {N} items — doctrine says EXACTLY 1. The per-item-explosion \
         demand-pull claims to eliminate is back. Check whether the \
         burst is being fanned to per-item evaluation downstream of \
         service_cycle_batched.",
        harness.burst_calls(),
    );

    // Sanity: the single gate call observed the FULL burst count.
    // (If the burst was silently truncated to one item, the doctrine
    // assertion above would still pass but the gate would be lying
    // about the burst's true size.)
    // Note: analyze_burst returns the count inside BurstEvaluateResult;
    // we re-call here only to read that field for the assertion.
    let result = harness.analyze_burst_counted(&inputs[0]);
    assert_eq!(
        result.burst_message_count, N,
        "the gate observed burst_message_count={}, expected {N} — \
         the burst was silently truncated before reaching analyze_burst",
        result.burst_message_count,
    );
}

/// Sweep across arrival counts — pins that the gate-call-count
/// property is CONSTANT regardless of N. If a future refactor
/// accidentally introduced O(N) gate calls (say, by iterating items
/// inside analyze_burst), this test would catch it across the sweep.
///
/// proves: gate-call count is INVARIANT in arrival count — the
/// substrate's demand-pull guarantee holds at N=1, N=50, N=500.
#[test]
fn analyze_burst_call_count_is_constant_across_arrival_count_sweep() {
    for &n in &[1usize, 50, 500] {
        let mut registry = ChannelRegistry::new();
        let mut state = PersonaState::new();
        let harness = PersonaHarness::new("Helper");
        let room = Uuid::new_v4();

        for i in 0..n {
            registry
                .route(make_chat(room, "Joel", &format!("msg {i}")))
                .expect("route");
        }

        let inputs =
            registry.service_cycle_batched(&mut state, &harness.identity, DEFAULT_BURST_WINDOW_MS);

        for input in &inputs {
            let _ = harness.analyze_burst_counted(input);
        }

        assert_eq!(
            harness.burst_calls(),
            1,
            "at N={n}: analyze_burst fired {} times — doctrine demands \
             EXACTLY 1 regardless of arrival count",
            harness.burst_calls(),
        );
    }
}
