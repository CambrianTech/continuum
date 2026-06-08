//! HippocampusModule — the memory region of the cognitive substrate.
//!
//! L0-3a.1 (this slice): the skeleton. Implements both `ServiceModule`
//! (for the runtime's command/event dispatch) and `BrainRegion` (for
//! the substrate governor's cognitive tick). Tick body is **idle** —
//! algorithms 1-5 from `docs/architecture/COGNITION-ALGORITHMS.md` land
//! in L0-3a.2 through L0-3a.7. Command surface is **empty** — the
//! existing [`MemoryModule`](super::memory::MemoryModule) continues to
//! handle `memory/*` commands; migration is L0-3a.1b.
//!
//! ## Doctrine
//!
//! From `docs/architecture/BRAIN-REGIONS-SUBSTRATE.md`:
//!
//! > No region of cognition runs on the hot path. Each region is its
//! > own RTOS task with its own tick. The handler dispatches and reads
//! > pre-staged results. The handler never blocks on recall, embedding,
//! > planning, or admission — those are continuously produced by their
//! > owning regions, in parallel, governed by SubstrateGovernor.
//!
//! HippocampusModule will eventually publish [`EngramPrefetch`] entries
//! into its [`engram_prefetch`](HippocampusModule::engram_prefetch)
//! ready-buffer on every tick, keyed by `(persona_id, channel_id)`.
//! Handlers will `peek` synchronously — never blocking on the tick.
//!
//! ## Outlier-validation hedge
//!
//! Per the CLAUDE.md outlier-validation strategy: the BrainRegion trait
//! in #1471 has only one implementation candidate today (this one). To
//! prevent the trait surface ossifying around hippocampus specifically,
//! the design is checked against two other plausible regions:
//!
//! - **Motor cortex** (L0-4a, planned): continuous candidate-utterance
//!   ranking off the partial-message stream. Differs from hippocampus
//!   in that the tick body is *latency-sensitive* — late candidates are
//!   useless. The trait's `CadenceHint::Faster` shape (in TickOutcome)
//!   accommodates this. The ReadyBuffer's per-key freshness semantic
//!   (publish overwrites, evict_stale prunes) also fits — motor cortex
//!   keeps only the freshest candidate set per channel.
//!
//! - **Attention** (L0-4b, planned): salience-map maintenance. Differs
//!   in that it doesn't publish to its own ready-buffer — it writes to
//!   shared `PersonaCognition.salience` (CRDT counters), which other
//!   regions *read* but it doesn't have a per-key prefetch shape. The
//!   trait still fits because publication-target isn't a trait concern;
//!   `BrainRegion::tick` returns `TickOutcome { published: N }` whether
//!   N counts ready-buffer publishes OR shared-state writes.
//!
//! Both alternative shapes fit the same trait without forcing. The
//! trait surface is proven for at least 3 distinct region behaviors.

use super::memory::MemoryState;
use crate::runtime::{
    BrainRegion, CommandResult, ComputeClass, DashMapReadyBuffer, MemoryClass, ModuleConfig,
    ModuleContext, ModulePriority, PressureProfile, PressureSignalKind, RegionContext, RegionId,
    ServiceModule, TickOutcome,
};
use async_trait::async_trait;
use serde_json::Value;
use std::any::Any;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use uuid::Uuid;

// ─── Placeholder ready-buffer value type ────────────────────────────

/// Placeholder for the engram-prefetch payload produced by the
/// hippocampus tick. The real shape (engram set + scoring metadata +
/// genome blend hint) lands in L0-3a.2 once Engram types exist.
///
/// Keeping this as a typed-but-empty struct now means downstream code
/// can already reference the ready-buffer's `Value` type without
/// waiting for L0-3a.2.
#[derive(Debug, Clone, Default)]
pub struct EngramPrefetch {
    /// Tick number this prefetch was produced on. Lets handlers detect
    /// stale buffers without timestamp comparison.
    pub produced_at_tick: u64,
}

/// Key shape for the engram-prefetch ready-buffer. The hippocampus
/// pre-stages prefetch sets per `(persona, channel)` pair; handlers
/// read the freshest one when they servicing a turn on that channel.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct EngramPrefetchKey {
    pub persona_id: Uuid,
    pub channel_id: Uuid,
}

// ─── HippocampusModule ──────────────────────────────────────────────

/// The hippocampus brain region.
///
/// Implements both [`ServiceModule`] (so it can absorb `memory/*`
/// commands in a later slice — currently empty surface, all `memory/*`
/// routes still through [`MemoryModule`](super::memory::MemoryModule))
/// and [`BrainRegion`] (so the substrate governor can call its
/// cognitive tick).
///
/// Shares state with `MemoryModule` via `Arc<MemoryState>` so when
/// L0-3a.1b absorbs command handling, the migration is structurally
/// trivial.
pub struct HippocampusModule {
    /// Shared with [`MemoryModule`](super::memory::MemoryModule).
    /// Holds the `PersonaMemoryManager` that backs recall / admission.
    #[allow(dead_code)] // wired in L0-3a.3 when salience updates the manager
    state: Arc<MemoryState>,

    /// Pre-staged prefetch results, published by `tick` and consumed
    /// by handlers via `peek`. L0-3a.7 wires the publish path; L0-3a.1
    /// just owns the buffer so the structural shape is observable.
    engram_prefetch: DashMapReadyBuffer<EngramPrefetchKey, EngramPrefetch>,

    /// Monotonic tick counter, used in `EngramPrefetch.produced_at_tick`
    /// and `RegionContext.tick_number`.
    tick_counter: AtomicU64,
}

impl HippocampusModule {
    pub fn new(state: Arc<MemoryState>) -> Self {
        Self {
            state,
            engram_prefetch: DashMapReadyBuffer::new(),
            tick_counter: AtomicU64::new(0),
        }
    }

    /// Expose the prefetch buffer so other modules (or tests) can
    /// `peek` without going through the trait object. Sharing is via
    /// the buffer's internal `Arc` (cheap clone).
    pub fn engram_prefetch(&self) -> DashMapReadyBuffer<EngramPrefetchKey, EngramPrefetch> {
        self.engram_prefetch.clone()
    }
}

// ─── ServiceModule (empty cmd surface, registers with runtime) ──────

#[async_trait]
impl ServiceModule for HippocampusModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "hippocampus",
            // Cognition priority — same as the existing cognition
            // module. Tick cadence and thread affinity flow from here.
            priority: ModulePriority::High,
            // Empty for now — L0-3a.1b migrates memory/* over from
            // MemoryModule. Keeping this empty here is what makes the
            // slice landable in isolation.
            command_prefixes: &[],
            event_subscriptions: &[],
            // ServiceModule's tick is what the runtime will eventually
            // call into; we leave the actual cognitive cycle to the
            // BrainRegion::tick impl below. Default scheduling.
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
        // Nothing to initialize in the skeleton. L0-3a.7 wires the
        // predictor's view of channel activity here.
        Ok(())
    }

    async fn handle_command(&self, command: &str, _params: Value) -> Result<CommandResult, String> {
        // Defensive: command_prefixes is empty, so the dispatcher
        // should never route anything here. If it does, fail loudly
        // rather than silently no-op.
        Err(format!(
            "HippocampusModule: no command surface yet (slice L0-3a.1); received `{command}`. \
             Routing bug — memory/* should still route to MemoryModule until L0-3a.1b."
        ))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

// ─── BrainRegion (idle tick, real pressure profile) ─────────────────

#[async_trait]
impl BrainRegion for HippocampusModule {
    fn id(&self) -> RegionId {
        RegionId::from_static("hippocampus")
    }

    fn pressure_profile(&self) -> PressureProfile {
        PressureProfile {
            // Hippocampus owns the engram graph, working memory ring,
            // salience map snapshots, and the prefetch ready-buffer —
            // it's the heaviest memory footprint of any region.
            memory_class: MemoryClass::Heavy,
            // The tick body will do scoring + activation spreading +
            // similarity matching — CPU-vectorized work. Inference
            // calls would push this to InferenceLight; algorithm 5's
            // predictor in L0-3a.7 may need that bump.
            compute_class: ComputeClass::CpuVectorized,
            // Memory pressure forces consolidation depth to drop;
            // inference queue depth forces predictor to back off so
            // hot-path inference isn't starved.
            responds_to: vec![
                PressureSignalKind::SystemMemHigh,
                PressureSignalKind::InferenceQueueDepth,
            ],
        }
    }

    async fn tick(&self, _ctx: &RegionContext) -> TickOutcome {
        // Idle. Algorithms 1-5 from COGNITION-ALGORITHMS.md drop into
        // this body across L0-3a.2 through L0-3a.7. Each algorithm
        // brings its own metric and test surface.
        //
        // We still bump the tick counter so future-slice telemetry
        // shows non-zero ticks from day one.
        let _tick_number = self.tick_counter.fetch_add(1, Ordering::Relaxed);
        TickOutcome::idle()
    }

    // `on_signal` defaults to no-op. Hippocampus will react to
    // `SleepTransition` in L0-4d (deeper consolidation when persona
    // moves to Sleep phase) but that's a future slice.
}

// ─── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::embedding::EmbeddingError;
    use crate::memory::{EmbeddingProvider, PersonaMemoryManager};
    use crate::runtime::ReadyBuffer;

    /// Stub embedding provider for tests — mirrors the one in
    /// `crate::memory::tests` since that one's not pub. The skeleton
    /// doesn't actually call the manager, but `MemoryState` requires
    /// constructing one to share with `MemoryModule` in later slices.
    struct StubEmbedding;

    impl EmbeddingProvider for StubEmbedding {
        fn name(&self) -> &str {
            "hippocampus-test-stub"
        }
        fn dimensions(&self) -> usize {
            384
        }
        fn embed(&self, _text: &str) -> Result<Vec<f32>, EmbeddingError> {
            Ok(vec![0.0; 384])
        }
        fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
            Ok(texts.iter().map(|_| vec![0.0; 384]).collect())
        }
    }

    fn make_module() -> HippocampusModule {
        let manager = Arc::new(PersonaMemoryManager::new(Arc::new(StubEmbedding)));
        let state = Arc::new(MemoryState::new(manager));
        HippocampusModule::new(state)
    }

    #[tokio::test]
    async fn region_id_is_stable_static_string() {
        let h = make_module();
        assert_eq!(h.id().as_str(), "hippocampus");
    }

    #[test]
    fn pressure_profile_declares_memory_heavy_compute_vectorized() {
        let h = make_module();
        let profile = h.pressure_profile();
        assert_eq!(profile.memory_class, MemoryClass::Heavy);
        assert_eq!(profile.compute_class, ComputeClass::CpuVectorized);
        // Both pressure kinds the hippocampus cares about must be present.
        assert!(profile
            .responds_to
            .contains(&PressureSignalKind::SystemMemHigh));
        assert!(profile
            .responds_to
            .contains(&PressureSignalKind::InferenceQueueDepth));
    }

    #[tokio::test]
    async fn idle_tick_returns_idle_outcome_and_bumps_counter() {
        let h = make_module();
        let ctx = RegionContext::global(0);

        // Disambiguate from ServiceModule::tick (which the runtime
        // calls separately and ignores in this slice) — we want the
        // cognitive tick specifically.
        let outcome_first = BrainRegion::tick(&h, &ctx).await;
        assert_eq!(outcome_first.published, 0);
        assert_eq!(outcome_first.consumed_since_last, 0);
        assert!(outcome_first.pressure_observed.is_none());
        assert!(outcome_first.cadence_hint.is_none());

        // Tick counter is observable via subsequent EngramPrefetch
        // publishes in later slices; verify it monotonically advances.
        let counter_after_first = h.tick_counter.load(Ordering::Relaxed);
        let _outcome_second = BrainRegion::tick(&h, &ctx).await;
        let counter_after_second = h.tick_counter.load(Ordering::Relaxed);
        assert_eq!(counter_after_second, counter_after_first + 1);
    }

    #[test]
    fn engram_prefetch_buffer_roundtrip() {
        let h = make_module();
        let buf = h.engram_prefetch();

        let key = EngramPrefetchKey {
            persona_id: Uuid::new_v4(),
            channel_id: Uuid::new_v4(),
        };
        let payload = EngramPrefetch {
            produced_at_tick: 42,
        };

        assert!(buf.peek(&key).is_none());
        buf.publish(key.clone(), payload.clone());
        let read = buf.peek(&key).expect("prefetch should be staged");
        assert_eq!(read.produced_at_tick, 42);
    }

    #[test]
    fn engram_prefetch_handle_is_shared_via_arc() {
        let h = make_module();
        // The handle exposed publicly is an Arc-shared clone. Two
        // callers see the same underlying storage — that's the contract
        // motor cortex / attention will rely on when they read the
        // hippocampus's prefetch buffer.
        let handle_a = h.engram_prefetch();
        let handle_b = h.engram_prefetch();

        let key = EngramPrefetchKey {
            persona_id: Uuid::new_v4(),
            channel_id: Uuid::new_v4(),
        };
        handle_a.publish(
            key.clone(),
            EngramPrefetch {
                produced_at_tick: 7,
            },
        );
        let via_b = handle_b
            .peek(&key)
            .expect("handle_b should see handle_a's write");
        assert_eq!(via_b.produced_at_tick, 7);
    }

    #[tokio::test]
    async fn service_module_handle_command_errors_for_unrouted_commands() {
        let h = make_module();
        let result = h
            .handle_command("memory/recall", serde_json::json!({}))
            .await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("no command surface yet"),
            "error should explain the empty surface; got: {err}"
        );
    }

    #[test]
    fn service_module_config_has_empty_cmd_and_event_surfaces() {
        let h = make_module();
        let config = h.config();
        assert_eq!(config.name, "hippocampus");
        assert_eq!(config.priority, ModulePriority::High);
        assert!(
            config.command_prefixes.is_empty(),
            "L0-3a.1: empty cmd surface (migration is L0-3a.1b)"
        );
        assert!(
            config.event_subscriptions.is_empty(),
            "L0-3a.1: no event subscriptions yet"
        );
    }
}
