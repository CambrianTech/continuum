//! Lane — the unit of inference budget per
//! [[INFERENCE-LANES-REALISTIC.md]].
//!
//! Joel (2026-05-31): "I think we weren't clever enough with our
//! lanes. The goal should be to ideally cover the needs of the
//! persona, while being realistic."
//!
//! A lane is `(persona, TaskKind, ThroughputLease)` over the shared
//! base model. Multiple lanes share the loaded model bytes; only KV
//! cache + persona-scoped state differs per lane. Continuous
//! batching multiplexes lanes through the same forward pass.
//!
//! This is the substrate's "ONE model, N lanes" cleverness — the
//! prior-attempt failure mode was conceiving lanes as separate
//! model loads. Lanes are recipe-budgeted KV slots, not weight
//! copies.
//!
//! ### Composition (no reinvention)
//!
//! Lane sits at the intersection of pre-shipped primitives:
//!
//! - [`crate::cognition::throughput_lease::ThroughputLease`] — the
//!   slot primitive, including the revocation policy (Pinned /
//!   Graceful / Hard) that the pressure broker honors.
//! - [`crate::inference::recipe_budget::TaskKind`] — the canonical
//!   per-task seed budget table.
//! - [`crate::identity::PeerId`] — the substrate's
//!   persona identity type.
//! - [`HandleRef`] — the inference handle the caller threads
//!   through `ai/inference/{open,generate,close}`.
//!
//! The Lane glues these together. The InferenceCoordinator
//! (`coordinator.rs`, lands next) owns a `DashMap<handle_id, Lane>`
//! and decides admission via the existing `AdaptiveThroughputPlanner`.
//!
//! ### Doctrine alignment
//!
//! - [[inference-scarcity-economics]] §"commands are dumb" — Lane is
//!   internal substrate state, never visible at the command surface.
//!   `ai/inference/open` returns a HandleRef; the Lane is bookkeeping
//!   behind it.
//! - [[host-the-seemingly-impossible]] — Lane is the unit through
//!   which one model serves 16 personas on commodity hardware. No
//!   tier-down; clever lane multiplexing.
//! - [[observability-is-half-the-architecture]] — Lane lifecycle
//!   events (admitted, evicted, demoted to Bench) emit through the
//!   coordinator's capture sink (lands with the coordinator).

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::cognition::throughput_lease::{ThroughputLease, ThroughputLeaseRevocationPolicy};
use crate::identity::PeerId;
use crate::inference::recipe_budget::TaskKind;

/// One persona's budgeted inference slot, served by the shared
/// base-model adapter on this host. The lane's lifetime parallels
/// the HandleRef it's bound to.
///
/// Fields aren't `pub` so external code goes through accessors —
/// keeps the coordinator the only mutator of the lease state.
#[derive(Debug, Clone)]
pub struct Lane {
    persona: PeerId,
    task: TaskKind,
    lease: ThroughputLease,
    /// Bound HandleRef's UUID. The coordinator's
    /// `DashMap<handle_id, Lane>` keys on this.
    handle_id: Uuid,
    /// Persona-class metadata flowing through to the daemon's
    /// scheduling decisions (per
    /// [[inference-scarcity-economics]] §"commands cannot
    /// negotiate this" — this gets DERIVED from task + persona
    /// context, never passed as a command param).
    class: LaneClass,
}

/// Coarse class the substrate uses to pick the lease revocation
/// policy + sit the lane in the right pressure response. This is
/// substrate-internal — callers never set it directly; the
/// coordinator derives it from `task` + persona state (e.g. is
/// this persona currently engaged in a live voice/video turn?).
///
/// Mapped to `ThroughputLeaseRevocationPolicy` via
/// `class.revocation_policy()`. The mapping is the
/// substrate's pressure-response contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/inference/LaneClass.ts"
)]
pub enum LaneClass {
    /// Active video/voice chat. Pressure broker MUST NOT evict
    /// mid-turn. Maps to `Pinned`.
    Realtime,
    /// Live chat reply, idle voice (engaged but no realtime
    /// constraint). Maps to `Graceful` — notify + evict OK.
    Interactive,
    /// Reflection, summarization, scheduled tasks. Maps to `Hard`
    /// — evict immediately under pressure.
    Background,
    /// Adversarial review, audits. Maps to `Hard` so realtime
    /// always wins, but coordinator prefers running these to
    /// completion when there's headroom.
    Sentinel,
}

impl LaneClass {
    /// The substrate's contract for what pressure does to a lane
    /// in this class. Matches the realistic-lane doc's revocation
    /// table.
    pub fn revocation_policy(self) -> ThroughputLeaseRevocationPolicy {
        match self {
            LaneClass::Realtime => ThroughputLeaseRevocationPolicy::Pinned,
            LaneClass::Interactive => ThroughputLeaseRevocationPolicy::Graceful,
            LaneClass::Background | LaneClass::Sentinel => ThroughputLeaseRevocationPolicy::Hard,
        }
    }

    /// Reasonable default class for a fresh `(persona, task)`
    /// open without explicit context. The coordinator can
    /// override based on persona's live-turn state.
    pub fn default_for_task(task: TaskKind) -> Self {
        match task {
            // Voice / video are realtime by default — they
            // come from live sensory pipelines.
            TaskKind::VoiceChat | TaskKind::VideoChat => LaneClass::Realtime,
            // Text chat + game-NPC-engaged are interactive by
            // default — they want low latency but aren't
            // realtime-frame-locked.
            TaskKind::Chat | TaskKind::GameNpcEngaged => LaneClass::Interactive,
            // Reflective / background tasks.
            TaskKind::CodingSmall
            | TaskKind::CodingLarge
            | TaskKind::GameNpcIdle
            | TaskKind::AcademyStudent => LaneClass::Background,
            // Sentinel work has its own class.
            TaskKind::SentinelEasy | TaskKind::SentinelHard => LaneClass::Sentinel,
        }
    }
}

impl Lane {
    /// Construct a lane with the given bindings. The coordinator
    /// builds the `ThroughputLease` via the existing
    /// `FootprintRegistry::acquire_lease` path before calling here
    /// — Lane itself doesn't touch the registry.
    pub fn new(
        persona: PeerId,
        task: TaskKind,
        lease: ThroughputLease,
        handle_id: Uuid,
        class: LaneClass,
    ) -> Self {
        Self {
            persona,
            task,
            lease,
            handle_id,
            class,
        }
    }

    pub fn persona(&self) -> PeerId {
        self.persona
    }
    pub fn task(&self) -> TaskKind {
        self.task
    }
    pub fn handle_id(&self) -> Uuid {
        self.handle_id
    }
    pub fn class(&self) -> LaneClass {
        self.class
    }
    pub fn lease(&self) -> &ThroughputLease {
        &self.lease
    }
    pub fn lease_id(&self) -> &str {
        &self.lease.lease_id
    }

    /// KV budget for this lane in tokens, from the canonical
    /// recipe_budget table. The coordinator uses this when sizing
    /// the lease's `cost_units` + when sizing the KV cache
    /// allocation in the adapter.
    pub fn seed_kv_tokens(&self) -> u32 {
        self.task.default_seed_tokens()
    }

    /// Maximum the lane is allowed to grow to (paging policy
    /// pulls toward seed; demand signals grow up to max). Same
    /// table.
    pub fn max_kv_tokens(&self) -> u32 {
        self.task.default_max_tokens()
    }

    /// Whether this lane's lease is pinned (pressure broker must
    /// not evict). The coordinator's eviction walk respects this.
    pub fn is_pinned(&self) -> bool {
        self.lease.revocation_policy == ThroughputLeaseRevocationPolicy::Pinned
    }

    /// True when the lease has expired against the given clock.
    /// Coordinator's tick prunes expired lanes.
    pub fn is_expired(&self, now_ms: u64) -> bool {
        self.lease.is_expired(now_ms)
    }

    /// Reclaimable in the lease's own sense (expired OR not
    /// pinned). Pressure-broker-side eviction walks lanes
    /// matching this AND not currently mid-generation.
    pub fn is_reclaimable(&self, now_ms: u64) -> bool {
        self.lease.is_reclaimable(now_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cognition::{ResourceClass, TargetSilicon};

    fn persona() -> PeerId {
        PeerId::from_uuid(Uuid::from_u128(0xAAAA))
    }

    fn make_lease(policy: ThroughputLeaseRevocationPolicy) -> ThroughputLease {
        ThroughputLease {
            lease_id: "test-lease-1".to_string(),
            artifact_key: "qwen-2.5-3b".to_string(),
            resource_class: ResourceClass::LocalGeneration,
            target_silicon: TargetSilicon::UnifiedMemory,
            holder_id: persona().as_uuid().to_string(),
            cost_units: 100,
            acquired_at_ms: 1_000_000,
            expires_at_ms: 2_000_000,
            revocation_policy: policy,
        }
    }

    fn lane_with(task: TaskKind, class: LaneClass) -> Lane {
        Lane::new(
            persona(),
            task,
            make_lease(class.revocation_policy()),
            Uuid::from_u128(0xBBBB),
            class,
        )
    }

    // ── LaneClass → revocation policy ────────────────────────────

    #[test]
    fn realtime_maps_to_pinned() {
        assert_eq!(
            LaneClass::Realtime.revocation_policy(),
            ThroughputLeaseRevocationPolicy::Pinned
        );
    }

    #[test]
    fn interactive_maps_to_graceful() {
        assert_eq!(
            LaneClass::Interactive.revocation_policy(),
            ThroughputLeaseRevocationPolicy::Graceful
        );
    }

    #[test]
    fn background_and_sentinel_map_to_hard() {
        assert_eq!(
            LaneClass::Background.revocation_policy(),
            ThroughputLeaseRevocationPolicy::Hard
        );
        assert_eq!(
            LaneClass::Sentinel.revocation_policy(),
            ThroughputLeaseRevocationPolicy::Hard
        );
    }

    // ── default_for_task ─────────────────────────────────────────

    #[test]
    fn voice_and_video_default_to_realtime() {
        assert_eq!(
            LaneClass::default_for_task(TaskKind::VoiceChat),
            LaneClass::Realtime
        );
        assert_eq!(
            LaneClass::default_for_task(TaskKind::VideoChat),
            LaneClass::Realtime
        );
    }

    #[test]
    fn chat_and_npc_engaged_default_to_interactive() {
        assert_eq!(
            LaneClass::default_for_task(TaskKind::Chat),
            LaneClass::Interactive
        );
        assert_eq!(
            LaneClass::default_for_task(TaskKind::GameNpcEngaged),
            LaneClass::Interactive
        );
    }

    #[test]
    fn coding_npc_idle_and_academy_default_to_background() {
        assert_eq!(
            LaneClass::default_for_task(TaskKind::CodingSmall),
            LaneClass::Background
        );
        assert_eq!(
            LaneClass::default_for_task(TaskKind::CodingLarge),
            LaneClass::Background
        );
        assert_eq!(
            LaneClass::default_for_task(TaskKind::GameNpcIdle),
            LaneClass::Background
        );
        assert_eq!(
            LaneClass::default_for_task(TaskKind::AcademyStudent),
            LaneClass::Background
        );
    }

    #[test]
    fn sentinel_tasks_default_to_sentinel_class() {
        assert_eq!(
            LaneClass::default_for_task(TaskKind::SentinelEasy),
            LaneClass::Sentinel
        );
        assert_eq!(
            LaneClass::default_for_task(TaskKind::SentinelHard),
            LaneClass::Sentinel
        );
    }

    // ── Lane field accessors ─────────────────────────────────────

    #[test]
    fn lane_reports_its_persona_task_handle_class() {
        let l = lane_with(TaskKind::Chat, LaneClass::Interactive);
        assert_eq!(l.persona(), persona());
        assert_eq!(l.task(), TaskKind::Chat);
        assert_eq!(l.handle_id(), Uuid::from_u128(0xBBBB));
        assert_eq!(l.class(), LaneClass::Interactive);
        assert_eq!(l.lease_id(), "test-lease-1");
    }

    // ── KV budget surfaces ───────────────────────────────────────

    #[test]
    fn lane_seed_kv_tokens_match_recipe_budget_table() {
        assert_eq!(
            lane_with(TaskKind::Chat, LaneClass::Interactive).seed_kv_tokens(),
            8 * 1024
        );
        assert_eq!(
            lane_with(TaskKind::VoiceChat, LaneClass::Realtime).seed_kv_tokens(),
            8 * 1024
        );
        assert_eq!(
            lane_with(TaskKind::GameNpcIdle, LaneClass::Background).seed_kv_tokens(),
            4 * 1024
        );
        assert_eq!(
            lane_with(TaskKind::CodingLarge, LaneClass::Background).seed_kv_tokens(),
            128 * 1024
        );
    }

    #[test]
    fn lane_max_kv_tokens_match_recipe_budget_table() {
        assert_eq!(
            lane_with(TaskKind::Chat, LaneClass::Interactive).max_kv_tokens(),
            16 * 1024
        );
        assert_eq!(
            lane_with(TaskKind::CodingLarge, LaneClass::Background).max_kv_tokens(),
            256 * 1024
        );
        assert_eq!(
            lane_with(TaskKind::GameNpcIdle, LaneClass::Background).max_kv_tokens(),
            8 * 1024
        );
    }

    // ── Pin / reclaim semantics ──────────────────────────────────

    #[test]
    fn realtime_lane_is_pinned() {
        let l = lane_with(TaskKind::VoiceChat, LaneClass::Realtime);
        assert!(l.is_pinned());
    }

    #[test]
    fn interactive_and_background_lanes_are_not_pinned() {
        assert!(!lane_with(TaskKind::Chat, LaneClass::Interactive).is_pinned());
        assert!(!lane_with(TaskKind::CodingLarge, LaneClass::Background).is_pinned());
    }

    #[test]
    fn expired_lease_marks_lane_expired() {
        let l = lane_with(TaskKind::Chat, LaneClass::Interactive);
        // Lease expires at 2_000_000; before that, not expired.
        assert!(!l.is_expired(1_999_999));
        assert!(l.is_expired(2_000_000));
        assert!(l.is_expired(3_000_000));
    }

    #[test]
    fn realtime_lane_is_not_reclaimable_while_active() {
        let l = lane_with(TaskKind::VoiceChat, LaneClass::Realtime);
        // Pinned + not expired → not reclaimable.
        assert!(!l.is_reclaimable(1_500_000));
        // Once expired, reclaimable even if pinned (lease-expiry
        // overrides pin per ThroughputLease::is_reclaimable).
        assert!(l.is_reclaimable(3_000_000));
    }

    #[test]
    fn background_lane_is_reclaimable_immediately() {
        let l = lane_with(TaskKind::GameNpcIdle, LaneClass::Background);
        // Not expired but not pinned → reclaimable any time.
        assert!(l.is_reclaimable(1_500_000));
        assert!(l.is_reclaimable(3_000_000));
    }
}
