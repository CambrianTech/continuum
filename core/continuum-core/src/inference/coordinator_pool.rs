//! `CoordinatorResourcePool` — adapts `InferenceCoordinator` to the
//! `paging::pool::ResourcePool` trait so `PressureBroker` can drive
//! lane eviction automatically when host memory tightens.
//!
//! Joel (2026-05-31): "Yeah keep going and keep merging." This is
//! the substrate-side glue that closes the realistic-lane build
//! plan's pressure response — without it, the coordinator has the
//! `evict_under_pressure` method but no one calls it. With it, the
//! PressureBroker's tier-monitoring loop fires the substrate's
//! pressure-driven eviction the same way it fires VRAM eviction on
//! the Docker tier.
//!
//! ### Why a wrapper instead of impl-on-coordinator
//!
//! The coordinator doesn't depend on `paging` (the doctrine layering
//! goes inference → paging, not the other way). Implementing
//! `ResourcePool` directly on `InferenceCoordinator` would push that
//! dependency upstream. The wrapper sits in the inference module,
//! depends on both, and stays small + auditable.
//!
//! ### Doctrine alignment
//!
//! - [[inference-scarcity-economics]] §"commands cannot negotiate
//!   this" — the wrapper is internal substrate plumbing. Callers
//!   never see it; pressure response is automatic when the wrapper
//!   is registered with the broker.
//! - [[observability-is-half-the-architecture]] — every
//!   `evict_at_least` call surfaces through the coordinator's
//!   existing `LaneCaptureSink` (LaneEvicted events fire per
//!   evicted lane). The wrapper itself stays thin.

use std::sync::Arc;

use crate::inference::coordinator::InferenceCoordinator;
use crate::paging::pool::{ResourcePool, ResourcePoolEntry};

/// Canonical tier name registered with the PressureBroker. Operators
/// see this in pressure dashboards + broker logs.
pub const TIER_NAME: &str = "inference-lanes";

/// Closure-typed clock so tests can inject deterministic time. The
/// production constructor uses wall-clock-now; the broker calls
/// `evict_at_least` synchronously without a clock argument so the
/// wrapper supplies its own.
type ClockFn = Box<dyn Fn() -> u64 + Send + Sync>;

/// Wraps an `Arc<InferenceCoordinator>` as a `ResourcePool` so the
/// PressureBroker can register + drive it.
pub struct CoordinatorResourcePool {
    coordinator: Arc<InferenceCoordinator>,
    clock: ClockFn,
    tier_name: String,
    /// Optional capacity override for the PressureBroker's
    /// pressure threshold. Decouples the broker's "when should I
    /// act?" budget from the coordinator's admission budget — the
    /// substrate may want to act ON pressure earlier than admission
    /// denies (e.g., wrapper reports 32GB capacity to the broker
    /// while admission allows up to 64GB of lane configuration).
    /// `None` = default to `coordinator.capacity_bytes()`.
    capacity_override: Option<u64>,
}

impl CoordinatorResourcePool {
    /// Construct with the default wall-clock and canonical tier name.
    pub fn new(coordinator: Arc<InferenceCoordinator>) -> Self {
        Self {
            coordinator,
            clock: Box::new(wall_clock_now_ms),
            tier_name: TIER_NAME.to_string(),
            capacity_override: None,
        }
    }

    /// Override the tier name (useful when a process hosts multiple
    /// coordinators — e.g. one per persona group — and dashboards
    /// need to distinguish them).
    pub fn with_tier_name(mut self, name: impl Into<String>) -> Self {
        self.tier_name = name.into();
        self
    }

    /// Inject a deterministic clock for tests. Production paths use
    /// the default wall-clock.
    pub fn with_clock<F: Fn() -> u64 + Send + Sync + 'static>(mut self, clock: F) -> Self {
        self.clock = Box::new(clock);
        self
    }

    /// Override the capacity the wrapper reports to the
    /// PressureBroker. Default = `coordinator.capacity_bytes()`.
    /// Useful when the substrate wants the broker to start acting
    /// BEFORE the coordinator's full admission budget is reached
    /// (e.g., host has 32GB RAM, admission allows 64GB of lane
    /// configurations, broker should evict when usage > 28GB).
    pub fn with_capacity_bytes(mut self, capacity: u64) -> Self {
        self.capacity_override = Some(capacity);
        self
    }
}

impl ResourcePool for CoordinatorResourcePool {
    fn tier_name(&self) -> &str {
        &self.tier_name
    }

    fn capacity_bytes(&self) -> u64 {
        self.capacity_override
            .unwrap_or_else(|| self.coordinator.capacity_bytes())
    }

    fn usage_bytes(&self) -> u64 {
        self.coordinator.lanes_usage_bytes()
    }

    fn evict_at_least(&self, want_bytes: u64) -> u64 {
        let now_ms = (self.clock)();
        let result = self.coordinator.evict_under_pressure(want_bytes, now_ms);
        result.bytes_freed
    }

    fn snapshot(&self) -> Vec<ResourcePoolEntry> {
        self.coordinator.lanes_snapshot()
    }
}

fn wall_clock_now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::adapter::AIProviderAdapter;
    use crate::ai::heuristic_adapter::HeuristicInferenceAdapter;
    use crate::cognition::adaptive_throughput::{
        ResourceClass, TargetSilicon, ThroughputLaneBudget,
    };
    use crate::identity::PeerId;
    use crate::inference::coordinator::{CoordinatorConfig, OpenLaneRequest};
    use crate::inference::footprint_registry::FootprintRegistry;
    use crate::inference::handle_store::InferenceHandleStore;
    use crate::inference::lane::LaneClass;
    use crate::inference::recipe_budget::TaskKind;
    use uuid::Uuid;

    fn persona(id: u128) -> PeerId {
        PeerId::from_uuid(Uuid::from_u128(id))
    }

    fn build_coordinator() -> Arc<InferenceCoordinator> {
        // Generous budget so multiple lanes admit; tiny bytes_per_token
        // so memory math is trivial (8K Chat = 8K bytes).
        let config = CoordinatorConfig {
            lane_budgets: vec![ThroughputLaneBudget {
                resource_class: ResourceClass::LocalGeneration,
                target_silicon: TargetSilicon::Cpu,
                max_concurrency: 16,
                max_cost_units: 100_000,
            }],
            bytes_per_token: 1,
            lease_duration_ms: 5_000_000,
            default_target_silicon: TargetSilicon::Cpu,
        };
        Arc::new(InferenceCoordinator::new(
            Arc::new(FootprintRegistry::new()),
            Arc::new(InferenceHandleStore::new()),
            config,
        ))
    }

    fn open_with_class(
        c: &InferenceCoordinator,
        persona_id: u128,
        task: TaskKind,
        class: LaneClass,
    ) {
        c.open_lane(OpenLaneRequest {
            persona: persona(persona_id),
            task,
            adapter: Arc::new(HeuristicInferenceAdapter::new()) as Arc<dyn AIProviderAdapter>,
            model: None,
            system_prompt: None,
            active_adapters: None,
            class_override: Some(class),
            now_ms: 1_000_000,
        })
        .unwrap();
    }

    // ── trait surface ───────────────────────────────────────────

    #[test]
    fn tier_name_defaults_to_canonical_constant() {
        let c = build_coordinator();
        let pool = CoordinatorResourcePool::new(c);
        assert_eq!(pool.tier_name(), TIER_NAME);
    }

    #[test]
    fn tier_name_override_takes_effect() {
        let c = build_coordinator();
        let pool = CoordinatorResourcePool::new(c).with_tier_name("inference-paige");
        assert_eq!(pool.tier_name(), "inference-paige");
    }

    #[test]
    fn capacity_bytes_sums_lane_budgets_times_bytes_per_token() {
        // Config: 100_000 max_cost_units × 1 byte_per_token = 100_000.
        let c = build_coordinator();
        let pool = CoordinatorResourcePool::new(c);
        assert_eq!(pool.capacity_bytes(), 100_000);
    }

    #[test]
    fn usage_bytes_zero_with_no_lanes_open() {
        let c = build_coordinator();
        let pool = CoordinatorResourcePool::new(c);
        assert_eq!(pool.usage_bytes(), 0);
    }

    #[test]
    fn usage_bytes_scales_with_open_lanes() {
        let c = build_coordinator();
        open_with_class(&c, 1, TaskKind::Chat, LaneClass::Interactive); // 8K
        let pool = CoordinatorResourcePool::new(c.clone());
        assert_eq!(pool.usage_bytes(), 8 * 1024);

        open_with_class(&c, 2, TaskKind::GameNpcIdle, LaneClass::Background); // 4K
        assert_eq!(pool.usage_bytes(), (8 + 4) * 1024);
    }

    #[test]
    fn snapshot_returns_one_entry_per_lane_with_handle_id_as_key() {
        let c = build_coordinator();
        open_with_class(&c, 1, TaskKind::Chat, LaneClass::Interactive);
        open_with_class(&c, 2, TaskKind::VoiceChat, LaneClass::Realtime);
        let pool = CoordinatorResourcePool::new(c);
        let entries = pool.snapshot();
        assert_eq!(entries.len(), 2);
        // The Realtime lane's entry has pinned_count=1; the Interactive
        // lane's is pinned_count=0.
        let pinned_total: u32 = entries.iter().map(|e| e.pinned_count).sum();
        assert_eq!(pinned_total, 1);
        // All entries have non-empty handle_id keys + non-zero size.
        for e in &entries {
            assert!(!e.key.is_empty());
            assert!(e.size_bytes > 0);
        }
    }

    // ── evict_at_least delegation ───────────────────────────────

    #[test]
    fn evict_at_least_delegates_to_coordinator_evict_under_pressure() {
        let c = build_coordinator();
        open_with_class(&c, 1, TaskKind::Chat, LaneClass::Interactive); // 8K
        open_with_class(&c, 2, TaskKind::CodingSmall, LaneClass::Background); // 32K
        let pool = CoordinatorResourcePool::new(c.clone()).with_clock(|| 1_500_000);

        // Target 1 byte — should evict the Background lane (Hard
        // wins over Graceful) freeing 32K.
        let freed = pool.evict_at_least(1);
        assert_eq!(freed, 32 * 1024);
        assert_eq!(c.lane_count(), 1);
    }

    #[test]
    fn evict_at_least_returns_actual_bytes_freed_not_target() {
        let c = build_coordinator();
        open_with_class(&c, 1, TaskKind::Chat, LaneClass::Interactive); // 8K
        let pool = CoordinatorResourcePool::new(c.clone()).with_clock(|| 1_500_000);

        // Target 100K, but only one 8K Interactive lane available
        // (no Pinned to skip; no Hard to take). The Interactive lane
        // yields under PressureGraceful — freeing 8K.
        let freed = pool.evict_at_least(100_000);
        assert_eq!(freed, 8 * 1024);
        assert_eq!(c.lane_count(), 0);
    }

    #[test]
    fn evict_at_least_with_only_pinned_lanes_frees_zero() {
        let c = build_coordinator();
        open_with_class(&c, 1, TaskKind::VoiceChat, LaneClass::Realtime);
        open_with_class(&c, 2, TaskKind::VoiceChat, LaneClass::Realtime);
        let pool = CoordinatorResourcePool::new(c.clone()).with_clock(|| 1_500_000);

        let freed = pool.evict_at_least(1_000_000);
        assert_eq!(freed, 0);
        assert_eq!(c.lane_count(), 2);
    }

    // ── pressure ratio sanity ──────────────────────────────────

    #[test]
    fn pressure_default_impl_returns_usage_over_capacity() {
        let c = build_coordinator();
        open_with_class(&c, 1, TaskKind::Chat, LaneClass::Interactive); // 8K of 100K
        let pool = CoordinatorResourcePool::new(c);
        let p = pool.pressure();
        // 8192 / 100_000 = 0.08192
        assert!((p - 0.08192).abs() < 1e-6, "pressure = {p}");
    }

    // ── PressureBroker end-to-end ──────────────────────────────

    #[test]
    fn broker_relief_evicts_through_coordinator_pool_when_pressure_high() {
        use crate::paging::broker::{BrokerConfig, PressureBroker};

        // Coordinator budget is GENEROUS so admission accepts the
        // lanes. The wrapper reports a SMALLER capacity to the
        // PressureBroker — so the broker sees over-budget pressure
        // and acts. This decoupling lets the substrate's admission
        // threshold and pressure-relief threshold be tuned
        // independently per [[inference-scarcity-economics]].
        let c = build_coordinator();
        open_with_class(&c, 1, TaskKind::Chat, LaneClass::Interactive); // 8K
        open_with_class(&c, 2, TaskKind::CodingSmall, LaneClass::Background); // 32K

        // Total usage = 40K. Pool advertises 16K capacity → pressure
        // = 2.5 → Critical tier → broker acts.
        let pool: Arc<dyn ResourcePool> = Arc::new(
            CoordinatorResourcePool::new(c.clone())
                .with_clock(|| 1_500_000)
                .with_capacity_bytes(16 * 1024),
        );
        let broker = PressureBroker::new(BrokerConfig::default());
        broker.register(pool);

        assert_eq!(c.lane_count(), 2);
        let pressure_before = broker.global_pressure();
        assert!(
            pressure_before > 1.0,
            "expected over-budget; got {pressure_before}"
        );

        let report = broker.relieve();
        assert!(
            report.triggered,
            "broker should have acted on critical pressure"
        );
        assert!(
            report.bytes_freed >= 32 * 1024,
            "expected >= 32K freed; got {}",
            report.bytes_freed
        );
        // The Hard-class Background went; the Interactive may or may
        // not have been pulled too depending on how aggressively the
        // broker targeted bytes.
        assert!(c.lane_count() <= 1);
    }

    #[test]
    fn broker_relief_with_only_pinned_lanes_emits_zero_freed_alert() {
        use crate::paging::broker::{BrokerConfig, PressureBroker};

        let c = build_coordinator();
        // Two Realtime (pinned) VoiceChat lanes — 8K each = 16K usage.
        open_with_class(&c, 1, TaskKind::VoiceChat, LaneClass::Realtime);
        open_with_class(&c, 2, TaskKind::VoiceChat, LaneClass::Realtime);

        // Wrapper advertises tiny 4K capacity → pressure = 4.0
        // (Critical). Broker will try to evict; all lanes pinned →
        // freed = 0.
        let pool: Arc<dyn ResourcePool> = Arc::new(
            CoordinatorResourcePool::new(c.clone())
                .with_clock(|| 1_500_000)
                .with_capacity_bytes(4 * 1024),
        );
        let broker = PressureBroker::new(BrokerConfig::default());
        broker.register(pool);

        let report = broker.relieve();
        // Triggered is FALSE because no pool freed any bytes (the
        // broker classifies "triggered" as "at least one pool freed
        // bytes"). The pinned-realtime guarantee holds — the
        // substrate's defining promise to active voice/video chat
        // personas survives even when pressure is over-target.
        assert!(!report.triggered);
        assert_eq!(report.bytes_freed, 0);
        assert_eq!(c.lane_count(), 2);
    }

    #[test]
    fn pressure_is_zero_when_capacity_is_zero() {
        // Coordinator with empty lane_budgets list has capacity 0.
        let config = CoordinatorConfig {
            lane_budgets: vec![],
            bytes_per_token: 1,
            lease_duration_ms: 5_000_000,
            default_target_silicon: TargetSilicon::Cpu,
        };
        let c = Arc::new(InferenceCoordinator::new(
            Arc::new(FootprintRegistry::new()),
            Arc::new(InferenceHandleStore::new()),
            config,
        ));
        let pool = CoordinatorResourcePool::new(c);
        assert_eq!(pool.pressure(), 0.0);
    }
}
