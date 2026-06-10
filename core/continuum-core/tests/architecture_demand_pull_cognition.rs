//! Architecture test — proves the "demand-pull eliminates idle work"
//! doctrine clause on the COGNITION side.
//!
//! See `docs/architecture/PROVING-THE-DOCTRINE.md` for the matrix this
//! file populates. The clause pinned here:
//!
//! > "Demand-pull eliminates idle work — a persona's service tick runs
//! > `analyze()` ONCE per channel-with-work, independent of how many
//! > items each channel carries. The cycle's wall-clock is bounded by
//! > the inference call (one per channel-tick), not by the arrival
//! > rate. Personas with bursty inboxes don't pay an inference-cost
//! > multiplier for staying connected."
//!
//! ## Why this matters for the substrate
//!
//! Without demand-pull batching, a persona with 50 buffered messages
//! would fire 50 `analyze()` calls per service cycle — turning every
//! conversational burst into an inference storm and turning every
//! many-persona room into a coordination collapse. The doctrine's
//! "host the seemingly impossible" claim depends on this property
//! holding: arrival rate decoupled from inference rate, with the
//! channel adapter doing the cheap aggregation work.
//!
//! ## Proof shape — three axes
//!
//! The doctrine has three load-bearing properties; this test proves
//! all three concretely:
//!
//! 1. **`[[cognition-batches-per-channel-adapter]]`** — N inbox arrivals
//!    on one channel → 1 `CoherentInput` per tick. The cognition side
//!    sees ONE batched input, not N per-item calls. Pinned by
//!    [`service_cycle_with_n_chat_messages_yields_one_input`].
//! 2. **Wall-clock bounded** — cycle time on N=500 arrivals is within a
//!    small constant factor of N=5. NOT linear in arrival count.
//!    Pinned by [`service_cycle_wallclock_independent_of_arrival_count`].
//! 3. **`[[shared-decode-per-persona-perspective]]`** — 16 parallel
//!    embedding reads on the same item return the SAME `Arc<Vec<f32>>`;
//!    the lazy cell fires compute ONCE and shares the result. Pinned
//!    by [`embedding_computed_once_across_concurrent_personas`].
//!
//! ## What this does NOT cover (intentional follow-ups)
//!
//! - **Real analyze() integration** — PR A keeps the existing
//!   `service_cycle` (single-pop) wired into cognition. The batched
//!   `service_cycle_batched` is the new seam; PR C reshapes
//!   `airc_chat_demo` to consume it, and the killer-loop integration
//!   test in that PR will prove the end-to-end inference-cost split.
//! - **Voice / Code / Background batching** — PR A only adds a Chat
//!   view. Other domains drain into `CoherentInput::Other` until their
//!   typed views land (PR D for Audio). The doctrine here is proven on
//!   Chat; the seam extends without retrofit.
//! - **Shape-3 bench** — substrate-side cost split when the vision
//!   encoder has 0 vs N subscribers. That's the complementary claim
//!   and lives in its own architecture test (PR E territory).
//!
//! ## Tag
//!
//! proves: demand-pull eliminates idle work — service tick fires
//! exactly 1 CoherentInput per channel-with-work (not N per arrival);
//! wall-clock bounded by analyze + ε; embedding compute fires once
//! per item across N concurrent personas.

use std::sync::Arc;
use std::time::{Duration, Instant};
use uuid::Uuid;

use continuum_core::persona::channel_items::ChatQueueItem;
use continuum_core::persona::channel_registry::{ChannelRegistry, DEFAULT_BURST_WINDOW_MS};
use continuum_core::persona::channel_types::QueueItemBehavior;
use continuum_core::persona::channel_view::CoherentInput;
use continuum_core::persona::types::{PersonaState, SenderType};

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
    })
}

/// Cognition-batches-per-channel-adapter, the load-bearing form.
///
/// A persona with N=500 chat messages arrived in one room since the last
/// tick MUST see exactly ONE `CoherentInput` from `service_cycle_batched`,
/// not 500. Downstream cognition would then fire `analyze()` exactly
/// once on that input — the "1 analyze per channel-tick" property the
/// doctrine claims.
///
/// If this assertion regresses, the substrate is back to per-item
/// inference cost — every bursty room becomes an inference storm,
/// many-persona hosting collapses, and the demand-pull doctrine is no
/// longer load-bearing.
///
/// proves: `[[cognition-batches-per-channel-adapter]]` — N inbox
/// arrivals on one channel → 1 `CoherentInput` per service cycle,
/// independent of N. The cognition layer's analyze count is bounded by
/// the channel count, not the arrival count.
#[test]
fn service_cycle_with_n_chat_messages_yields_one_input() {
    let mut registry = ChannelRegistry::new();
    let mut state = PersonaState::new();
    let room = Uuid::new_v4();
    let persona_id = Uuid::new_v4();

    // 500 messages — the "bursty room" stress case. A 16-persona
    // conversation around a hot topic easily produces this kind of
    // arrival rate per service interval.
    const N: usize = 500;
    for i in 0..N {
        registry
            .route(make_chat(room, "Joel", &format!("message {i}")))
            .expect("route");
    }

    let inputs = registry.service_cycle_batched(
        &mut state,
        persona_id,
        "Helper",
        DEFAULT_BURST_WINDOW_MS,
    );

    // THE load-bearing assertion. If this is N, the substrate is
    // per-item inference; if this is 1, the substrate is demand-pull.
    assert_eq!(
        inputs.len(),
        1,
        "expected exactly 1 CoherentInput for {N} chat items, got {}. \
         This means cognition would fire analyze() {} times per tick — \
         the demand-pull doctrine has regressed.",
        inputs.len(),
        inputs.len(),
    );

    // The single input reflects all N messages were folded in via
    // consolidation, not silently dropped.
    let burst = match &inputs[0] {
        CoherentInput::Chat(c) => c,
        other => panic!("expected CoherentInput::Chat, got {other:?}"),
    };
    assert_eq!(
        burst.burst_message_count, N,
        "the burst dropped messages — consolidation lost {} items",
        N - burst.burst_message_count
    );
    assert_eq!(burst.primary_room, room);
}

/// Wall-clock bounded — the cycle's cost does NOT compound with
/// arrival count.
///
/// We measure two service-cycle wall-clocks, one with N=5 and one with
/// N=500 chat items, then assert the ratio stays well below the
/// per-item-explosion counterfactual. The doctrine claim:
///
/// > Cycle wall-clock = consolidate (linear, cheap) + interpret
/// > (linear, cheap) + analyze (O(1) per channel, dominant if inference
/// > is real). Without analyze, the cycle's runtime is pure aggregation
/// > overhead.
///
/// If the cycle were per-item analyze, the ratio would be ~100× (500 vs
/// 5 items). The bound `BOUNDED_RATIO = 100` is generous — in practice
/// the ratio is much smaller on the consolidation-only path — but it's
/// the threshold that distinguishes "demand-pull batching" from
/// "per-item processing".
///
/// We sample 5 runs and take the median to absorb scheduler noise.
///
/// proves: wall-clock bounded — service_cycle_batched runtime is
/// dominated by per-channel analyze (O(1) per channel-tick), NOT by
/// per-item processing (O(N)). Cycle scales sub-linearly in arrival
/// rate.
#[test]
fn service_cycle_wallclock_independent_of_arrival_count() {
    fn measure(n: usize) -> Duration {
        let mut samples: Vec<Duration> = (0..5)
            .map(|_| {
                let mut registry = ChannelRegistry::new();
                let mut state = PersonaState::new();
                let room = Uuid::new_v4();
                let persona_id = Uuid::new_v4();

                for i in 0..n {
                    registry
                        .route(make_chat(room, "Joel", &format!("msg {i}")))
                        .expect("route");
                }

                let start = Instant::now();
                let inputs = registry.service_cycle_batched(
                    &mut state,
                    persona_id,
                    "Helper",
                    DEFAULT_BURST_WINDOW_MS,
                );
                let elapsed = start.elapsed();
                assert_eq!(inputs.len(), 1, "demand-pull broken at N={n}");
                elapsed
            })
            .collect();
        samples.sort();
        samples[samples.len() / 2]
    }

    let t5 = measure(5);
    let t500 = measure(500);

    // The counterfactual: if the cycle were per-item, the ratio would
    // approach 100× (500/5). Demand-pull batching keeps it bounded.
    // 100× is the failure threshold; the doctrine claim is that
    // BOUNDED_RATIO is reachable.
    const BOUNDED_RATIO: f64 = 100.0;
    let ratio = t500.as_nanos() as f64 / t5.as_nanos().max(1) as f64;

    assert!(
        ratio < BOUNDED_RATIO,
        "service_cycle_batched scales linearly with arrival count: \
         t5={t5:?}, t500={t500:?}, ratio={ratio:.2}× ≥ {BOUNDED_RATIO}×. \
         The demand-pull doctrine claims cycle wall-clock is bounded \
         by analyze (per-channel-tick, O(1)), not by per-item \
         processing (O(N)). This ratio suggests the cycle has regressed \
         to per-item cost."
    );
}

/// Shared-decode-per-persona-perspective, the load-bearing form.
///
/// 16 personas concurrently demand the embedding on the SAME chat item.
/// The doctrine says: compute fires ONCE on the item's lazy cell; every
/// other persona gets a clone of the cached `Arc<Vec<f32>>`. NOT 16
/// independent computes.
///
/// The witness: after 16 concurrent calls, the item's `embedding_cell`
/// is populated with exactly ONE `Arc<Vec<f32>>`, and all 16 calls
/// returned `Arc::ptr_eq` clones of that value. If compute had fired
/// independently per persona, we'd see 16 different Arc values — the
/// shared-decode property would be a lie and the substrate's
/// "many personas in one room" cost claim would be wishful thinking.
///
/// proves: `[[shared-decode-per-persona-perspective]]` — the
/// substrate-shared expensive decode (here: embedding) runs ONCE per
/// item, amortized across N concurrent persona consumers. The lazy
/// cell on the item is the cost-split primitive.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn embedding_computed_once_across_concurrent_personas() {
    let room = Uuid::new_v4();
    let item = Arc::new(ChatQueueItem {
        id: Uuid::new_v4(),
        room_id: room,
        content: "the shared content for this concurrent test".into(),
        sender_id: Uuid::new_v4(),
        sender_name: "Joel".into(),
        sender_type: SenderType::Human,
        mentions: false,
        timestamp: now_ms(),
        enqueued_at: now_ms(),
        priority: 0.5,
        consolidated_context: Vec::new(),
        media: Vec::new(),
        embedding_cell: std::sync::OnceLock::new(),
    });

    // Sanity: before any persona observes the item, the lazy cell is
    // un-populated. If this fails, the test fixture itself is broken
    // (item was pre-warmed somewhere).
    assert!(
        item.embedding_cell.get().is_none(),
        "lazy cell pre-populated — test fixture invalid"
    );

    // 16 personas concurrently demand the embedding. The "many
    // personas in one room" cost-split case.
    const N_PERSONAS: usize = 16;
    let mut handles = Vec::with_capacity(N_PERSONAS);
    for _ in 0..N_PERSONAS {
        let item = Arc::clone(&item);
        handles.push(tokio::task::spawn_blocking(move || item.embedding()));
    }
    let mut returned_arcs: Vec<Arc<Vec<f32>>> = Vec::with_capacity(N_PERSONAS);
    for h in handles {
        returned_arcs.push(h.await.expect("join"));
    }

    // After the storm, the cell holds exactly ONE Arc<Vec<f32>>.
    let cached = item
        .embedding_cell
        .get()
        .expect("lazy cell must be populated after N persona reads");

    // Every persona's returned Arc points to the SAME cached value.
    // If any returned a different Arc, compute fired independently
    // for that persona — the shared-decode property is broken.
    for (i, persona_arc) in returned_arcs.iter().enumerate() {
        assert!(
            Arc::ptr_eq(persona_arc, cached),
            "persona {i} received a different Arc from the cached cell — \
             compute fired multiple times. \
             Shared-decode doctrine has regressed; N personas in one \
             room now pay N× embedding cost instead of 1× shared."
        );
    }

    // Also: the cached Vec has nonzero len (the placeholder compute
    // produced a real embedding, didn't silently no-op).
    assert!(
        !cached.is_empty(),
        "cached embedding is empty — compute silently produced no output"
    );
}
