//! The Proctored Exam Session's ACQUIRE phase — the strategic serving decision, made
//! explicit at the exam seam.
//!
//! The living-persona benchmark used to acquire a bare [`ServingSteadyHold`] and *assume*
//! it was measuring a co-tenant slot on the persona's own lane. The comment said
//! "= `Placement::ShareLane`" but nothing computed it — the strategic decision was
//! implicit. This module makes it real: it runs the model-aware admission kernel
//! ([`plan_placement`]) against the live resident set, HOLDS the lane steady when the
//! verdict is a share, and CARRIES the verdict so the decision is observable + capturable
//! (the curriculum tie-in) instead of a silent assumption.
//!
//! This is the seam the strategic layer plugs into. For the single-GPU living-persona exam
//! the verdict is deterministically `ShareLane` (her base is already resident — sharing is
//! nearly free and is the difference between fitting and OOMing a second 24B copy, the
//! incident that started this arc). The `SpawnLane`/`CpuSpill` arms are the rails a
//! DIFFERENT-base exam or a grid placement rides on: the decision is computed here even
//! when this node's ACQUIRE doesn't itself drive the preemption (the ephemeral-lane / grid
//! path owns the spawn), so a strategic verdict is never missing again.
//! [[proctored-exam-session-dependable-benchmark]] [[lane-admission-planner-scenario-driven]]

use crate::modules::serving_daemon::ServingSteadyHold;
use crate::resources::placement::{plan_placement, LaneDemand, Placement, ResidentLane};

/// The verdict the exam ACQUIRE reached — carried for observability + the curriculum
/// ledger, so the strategic decision behind every measurement is inspectable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExamAcquire {
    /// The exam base is already resident → she is measured as a co-tenant decode slot on
    /// the live lane (`lane_id`), NO second weight copy. The lane is held steady for the
    /// exam (grow-back relaunch suppressed). The validated single-GPU path. `reclaim` names
    /// any lower-tier lanes the daemon should tier down first (empty in the common case).
    SharedLane { lane_id: String, reclaim: Vec<String> },
    /// The exam base is NOT resident → a fresh copy is needed (its own weights). This
    /// node's ACQUIRE does not spawn it here — the caller's ephemeral-lane / grid path owns
    /// that — but the strategic verdict is recorded, including the `reclaim` lanes a
    /// single-GPU spawn would have to tier down first (the incident's missing decision).
    Spawn { reclaim: Vec<String> },
    /// Won't fit even after tiering down everything preemptible → honest CPU/degrade with a
    /// named reason, never an OOM of the resident set.
    CpuSpill { reason: String },
}

impl ExamAcquire {
    /// A short, log/ledger-friendly tag for the verdict.
    pub fn tag(&self) -> &'static str {
        match self {
            ExamAcquire::SharedLane { .. } => "share",
            ExamAcquire::Spawn { .. } => "spawn",
            ExamAcquire::CpuSpill { .. } => "cpu-spill",
        }
    }
}

/// A held exam serving context. While alive, an exam that shares the live lane keeps it
/// steady — no grow-back relaunch can connection-refuse the measurement mid-flight (the
/// hard-rs 0/8 bounce). Dropping restores normal serving (RAII). Non-share verdicts hold
/// nothing (the caller's lane owns them), but the DECISION is explicit either way.
pub struct ExamServingContext {
    acquire: ExamAcquire,
    /// `Some` only for a `SharedLane` verdict: the RAII grow-back suppressor. Dropped with
    /// the context, restoring the daemon's normal re-home behavior.
    _hold: Option<ServingSteadyHold>,
}

impl ExamServingContext {
    /// Run the admission decision for an exam lane against the live resident set and act on
    /// it: hold the lane steady on a share, record the verdict otherwise. Pure inputs
    /// (`capacity` = the device's physical ceiling, already net of external reserve;
    /// `resident` = the physically-resident lanes; `demand` = the exam's lane demand), so
    /// the decision is unit-testable against the placement oracle.
    pub fn acquire(capacity: u64, resident: &[ResidentLane], demand: &LaneDemand) -> Self {
        let acquire = match plan_placement(capacity, resident, demand) {
            Placement::ShareLane { lane_id, reclaim, .. } => {
                ExamAcquire::SharedLane { lane_id, reclaim }
            }
            Placement::SpawnLane { reclaim } => ExamAcquire::Spawn { reclaim },
            Placement::CpuSpill { reason } => ExamAcquire::CpuSpill { reason },
        };
        // Hold the lane steady ONLY for a share — that's the case measured on the live lane,
        // where a grow-back relaunch mid-exam is the failure. Spawn/spill run on a separate
        // (ephemeral / peer) lane the daemon never re-homes.
        let hold = matches!(acquire, ExamAcquire::SharedLane { .. }).then(ServingSteadyHold::acquire);
        crate::probe!(
            class = "exam.acquire",
            verdict = acquire.tag(),
            held_steady = hold.is_some(),
            "proctored exam serving context acquired (strategic admission decision)"
        );
        Self { acquire, _hold: hold }
    }

    /// The live serving inputs weren't resolvable (ungoverned host, model row missing, or
    /// the lane isn't ready yet) — fall back to the historical behavior: hold the live lane
    /// steady on the assumption it's a share, but MARK the verdict `unresolved` so the gap is
    /// visible rather than silently assumed. Behavior never regresses vs the old blind
    /// steady-hold; the strategic decision is just recorded as "couldn't compute" this time.
    pub fn steady_fallback() -> Self {
        crate::probe!(
            class = "exam.acquire.fallback",
            "exam serving inputs unresolved — holding the live lane steady on the share assumption"
        );
        Self {
            acquire: ExamAcquire::SharedLane {
                lane_id: "live(unresolved)".to_string(),
                reclaim: Vec::new(),
            },
            _hold: Some(ServingSteadyHold::acquire()),
        }
    }

    /// The strategic verdict — for the ledger / capture record.
    pub fn verdict(&self) -> &ExamAcquire {
        &self.acquire
    }

    /// True while the live lane is held steady (a share verdict). False for spawn/spill.
    pub fn holds_steady(&self) -> bool {
        self._hold.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::serving_daemon::serving_held_steady;
    use crate::resources::placement::DemandTier;

    const GIB: u64 = 1 << 30;
    const KV_PER_TOKEN: u64 = 160 * 1024;
    const WEIGHTS: u64 = 14 * GIB;
    const COMPUTE: u64 = GIB;

    fn live_lane(base: &str) -> ResidentLane {
        ResidentLane {
            lane_id: "live".into(),
            base_model_id: base.into(),
            weights_bytes: WEIGHTS,
            slots: 2,
            window: 8192,
            kv_per_token: KV_PER_TOKEN,
            compute_buffer: COMPUTE,
            tier: DemandTier::Live,
            pinned: true,
        }
    }

    fn exam_demand(base: &str) -> LaneDemand {
        LaneDemand {
            base_model_id: base.into(),
            weights_bytes: WEIGHTS,
            slots: 1,
            window: 8192,
            kv_per_token: KV_PER_TOKEN,
            compute_buffer: COMPUTE,
            tier: DemandTier::Eval,
        }
    }

    // what this catches: the living-persona exam (same base already resident) resolves to a
    // SHARE on her real lane AND holds it steady for the duration — never a second weight
    // copy, never a grow-back relaunch mid-exam. This is the strategic decision made explicit
    // that the (None,None) branch used to only assume. [[proctored-exam-session-dependable-benchmark]]
    #[test]
    fn living_persona_exam_shares_the_live_lane_and_holds_it_steady() {
        let resident = [live_lane("devstral-24b")];
        let ctx = ExamServingContext::acquire(40 * GIB, &resident, &exam_demand("devstral-24b"));
        assert!(matches!(ctx.verdict(), ExamAcquire::SharedLane { lane_id, .. } if lane_id == "live"));
        assert!(ctx.holds_steady(), "a shared-lane exam must hold the live lane steady");
        assert!(serving_held_steady(), "the global steady gauge reflects the held exam");
        drop(ctx);
        assert!(!serving_held_steady(), "dropping the context restores grow-back (RAII)");
    }

    // what this catches: a DIFFERENT-base exam does NOT try to co-tenant her lane — the
    // strategic verdict is Spawn (a fresh copy is needed), and this node's ACQUIRE holds
    // NOTHING steady (the ephemeral/peer lane owns it). The decision is still explicit, which
    // is the incident's missing piece — no silent second-copy grab.
    #[test]
    fn different_base_exam_is_a_spawn_verdict_holding_nothing_local() {
        let resident = [live_lane("devstral-24b")];
        // Room for a second copy → SpawnLane with no reclaim (a bigger box).
        let ctx = ExamServingContext::acquire(64 * GIB, &resident, &exam_demand("qwen-coder-32b"));
        assert!(matches!(ctx.verdict(), ExamAcquire::Spawn { reclaim } if reclaim.is_empty()));
        assert!(!ctx.holds_steady(), "a spawn verdict holds no local steady-hold");
    }

    // what this catches: when even a share can't be made to fit (a tiny device already full
    // of a pinned live lane), the verdict is an honest CpuSpill with a reason — never an OOM
    // of the resident set. The exam is told, loudly, that the accelerator can't host it.
    #[test]
    fn cant_fit_even_a_share_is_an_honest_cpu_spill() {
        let resident = [live_lane("devstral-24b")];
        // Capacity below the resident lane's own footprint ⇒ zero free ⇒ a 1-slot share's KV
        // can't be freed (the only lane is pinned+Live, never a victim) ⇒ CpuSpill.
        let capacity = resident[0].footprint().saturating_sub(GIB);
        let ctx = ExamServingContext::acquire(capacity, &resident, &exam_demand("devstral-24b"));
        assert!(matches!(ctx.verdict(), ExamAcquire::CpuSpill { .. }));
        assert!(!ctx.holds_steady(), "a spill holds no steady-hold");
    }
}
