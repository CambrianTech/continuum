//! The Proctored Exam Session's ACQUIRE phase — the strategic serving decision, made
//! explicit at the exam seam, and the LUDICROUS window-grow that gives a benchmark the
//! biggest window the model+machine allow.
//!
//! The living-persona benchmark used to grab a bare steady-hold and *assume* it was a
//! co-tenant slot; the comment said "= `Placement::ShareLane`" but nothing computed it. This
//! module makes it real: it runs the model-aware admission kernel ([`plan_placement`]), and
//! for a share it drives the correct GROW → SETTLE → PIN sequence:
//!   1. **GROW** — declare LUDICROUS ([`ServingLudicrousHold`] → `PowerMode::Performance`), so
//!      the daemon re-plans the shared lane to the biggest window the machine fits. Never a
//!      starved boot-window (the 2048-eco incident).
//!   2. **SETTLE** — wait for any grow-relaunch to finish
//!      ([`wait_for_serving_window_settle`](crate::inference::llama_server::wait_for_serving_window_settle)).
//!      A naive "force Performance then run" thrashes: acquiring Ludicrous is itself a re-plan
//!      → relaunch, and a relaunch mid-exam connection-refuses every generation (the reverted
//!      `95dcec669`). Settling BEFORE the tasks run absorbs that relaunch once, up front.
//!   3. **PIN** — take the steady-hold so no FURTHER relaunch bounces the running exam.
//! The verdict is CARRIED for observability + the curriculum ledger.
//!
//! The `SpawnLane`/`CpuSpill` arms are the rails a DIFFERENT-base exam or a grid placement
//! rides on: the strategic decision is computed here even when this node's ACQUIRE doesn't
//! drive the spawn (the ephemeral-lane / grid path owns it), so a verdict is never missing.
//! [[proctored-exam-session-dependable-benchmark]] [[serving-mode-follows-activity-ludicrous-to-dream]]
//! [[benchmark-window-must-be-big-not-a-clamped-prompt]] [[lane-admission-planner-scenario-driven]]

use std::time::Duration;

use crate::modules::serving_daemon::{ServingLudicrousHold, ServingSteadyHold};
use crate::resources::placement::{plan_placement, LaneDemand, Placement, ResidentLane};

/// How long to watch for a grow-relaunch to START (lane goes not-ready) after declaring
/// Ludicrous. Must EXCEED the serving daemon's 5s re-plan `TICK` so a relaunch has time to
/// fire; if none does in this window, the lane was already at the target window (no grow).
const LUDICROUS_SETTLE_GRACE: Duration = Duration::from_secs(8);

/// Bound for the whole grow-relaunch to finish (kill + respawn + model reload). A 24B reload
/// is tens of seconds; generous so a real grow always completes, but never hangs the exam.
const LUDICROUS_SETTLE_TIMEOUT: Duration = Duration::from_secs(180);

/// The verdict the exam ACQUIRE reached — carried for observability + the curriculum ledger,
/// so the strategic decision behind every measurement is inspectable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExamAcquire {
    /// The exam base is already resident → she is measured as a co-tenant decode slot on the
    /// live lane (`lane_id`), NO second weight copy. The lane is grown to LUDICROUS
    /// (Performance → biggest window), settled, then held steady for the exam. The validated
    /// single-GPU path. `reclaim` names any lower-tier lanes the daemon should tier down first
    /// (empty in the common case).
    SharedLane { lane_id: String, reclaim: Vec<String> },
    /// The exam base is NOT resident → a fresh copy is needed (its own weights). This node's
    /// ACQUIRE does not spawn it here — the caller's ephemeral-lane / grid path owns that — but
    /// the strategic verdict is recorded, including the `reclaim` lanes a single-GPU spawn would
    /// have to tier down first (the incident's missing decision).
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

/// The RAII holds a shared-lane exam binds for its whole run: LUDICROUS (serve at Performance)
/// + STEADY (no further relaunch). Both release together when the context drops, reverting
/// serving to the live pressure-adaptive mode.
struct ExamHold {
    _ludicrous: ServingLudicrousHold,
    _steady: ServingSteadyHold,
}

/// A held exam serving context. While alive, a shared-lane exam serves at the biggest window
/// the machine allows and holds it stable (no relaunch bounce). Dropping restores normal
/// serving (RAII). Non-share verdicts hold nothing (the caller's lane owns them), but the
/// DECISION is explicit either way.
pub struct ExamServingContext {
    acquire: ExamAcquire,
    _hold: Option<ExamHold>,
}

impl ExamServingContext {
    /// The pure placement verdict for these inputs — no I/O, unit-testable against the oracle.
    /// `capacity` = the device's physical ceiling (net of external reserve); `resident` = the
    /// physically-resident lanes; `demand` = the exam's lane demand.
    fn decide(capacity: u64, resident: &[ResidentLane], demand: &LaneDemand) -> ExamAcquire {
        match plan_placement(capacity, resident, demand) {
            Placement::ShareLane { lane_id, reclaim, .. } => {
                ExamAcquire::SharedLane { lane_id, reclaim }
            }
            Placement::SpawnLane { reclaim } => ExamAcquire::Spawn { reclaim },
            Placement::CpuSpill { reason } => ExamAcquire::CpuSpill { reason },
        }
    }

    /// Run the admission decision and, for a share, drive GROW → SETTLE → PIN so the exam runs
    /// on the biggest window the machine allows without a mid-exam relaunch bounce. Async
    /// because settling the grow-relaunch is a wait (the whole reason the naive synchronous
    /// version thrashed). Spawn/spill hold nothing locally.
    pub async fn acquire(capacity: u64, resident: &[ResidentLane], demand: &LaneDemand) -> Self {
        let acquire = Self::decide(capacity, resident, demand);
        let hold = if matches!(acquire, ExamAcquire::SharedLane { .. }) {
            Some(Self::grow_settle_pin().await)
        } else {
            None
        };
        crate::probe!(
            class = "exam.acquire",
            verdict = acquire.tag(),
            ludicrous = hold.is_some(),
            "proctored exam serving context acquired (strategic admission decision)"
        );
        Self { acquire, _hold: hold }
    }

    /// The live serving inputs weren't resolvable (ungoverned host, model row missing, or the
    /// lane isn't ready yet) — fall back to the share posture (grow → settle → pin) but MARK the
    /// verdict `unresolved` so the gap is visible rather than silently assumed. The exam still
    /// gets the biggest window; only the placement decision is recorded as "couldn't compute".
    pub async fn ludicrous_fallback() -> Self {
        crate::probe!(
            class = "exam.acquire.fallback",
            "exam serving inputs unresolved — growing the live lane LUDICROUS on the share assumption"
        );
        Self {
            acquire: ExamAcquire::SharedLane {
                lane_id: "live(unresolved)".to_string(),
                reclaim: Vec::new(),
            },
            _hold: Some(Self::grow_settle_pin().await),
        }
    }

    /// GROW (declare Ludicrous) → SETTLE (absorb any grow-relaunch) → PIN (steady-hold). The
    /// Ludicrous hold is taken FIRST and kept, so `host_budget` plans at Performance for the
    /// whole exam; the settle happens BEFORE the steady-hold so the pin lands on the grown,
    /// stable lane — never on a lane about to relaunch.
    async fn grow_settle_pin() -> ExamHold {
        let ludicrous = ServingLudicrousHold::acquire();
        let settled = crate::inference::llama_server::wait_for_serving_window_settle(
            LUDICROUS_SETTLE_GRACE,
            LUDICROUS_SETTLE_TIMEOUT,
        )
        .await;
        crate::probe!(
            class = "exam.acquire.ludicrous",
            settled_window = settled.unwrap_or(0),
            settled = settled.is_some(),
            "exam lane grown to Ludicrous (Performance) and settled before pinning"
        );
        let steady = ServingSteadyHold::acquire("eval");
        ExamHold {
            _ludicrous: ludicrous,
            _steady: steady,
        }
    }

    /// The strategic verdict — for the ledger / capture record.
    pub fn verdict(&self) -> &ExamAcquire {
        &self.acquire
    }

    /// True while the live lane is held (a share verdict grew+pinned it). False for spawn/spill.
    pub fn holds(&self) -> bool {
        self._hold.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::serving_daemon::{serving_held_steady, serving_ludicrous_active};
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
            // An exam is a hard, disruption-intolerant task: it demands its OWN lane so it is
            // never a co-tenant slot the live personas starve. Matches the real exam demand.
            isolate: true,
        }
    }

    // what this catches: the living-persona exam (same base already resident) now takes its OWN
    // DEDICATED lane when a fresh copy fits, rather than a co-tenant slot on her live lane that
    // the personas would starve. This is the 2026-07-21 isolation-policy reversal: the exam is a
    // hard task and must not be disrupted — autonomic, no `base_model_id` hand-holding.
    // [[proctored-exam-session-dependable-benchmark]] [[dedicated-eval-lane-must-keep-its-own-window]]
    #[test]
    fn living_persona_exam_takes_a_dedicated_lane_when_it_fits() {
        let resident = [live_lane("devstral-24b")];
        // 40 GiB: live ~17.5 GiB + a fresh exam copy ~16.25 GiB both fit → dedicated Spawn.
        let v = ExamServingContext::decide(40 * GIB, &resident, &exam_demand("devstral-24b"));
        assert!(matches!(v, ExamAcquire::Spawn { .. }), "exam must isolate onto its own lane, got {v:?}");
    }

    // what this catches: the isolation policy degrades GRACEFULLY — when a dedicated second copy
    // won't fit, the exam falls back to a co-tenant SHARE of the live lane rather than spilling.
    #[test]
    fn living_persona_exam_falls_back_to_share_under_pressure() {
        let resident = [live_lane("devstral-24b")];
        // 28 GiB: live ~17.5 GiB leaves ~10.5 free — a fresh ~16.25 copy won't fit, share does.
        let v = ExamServingContext::decide(28 * GIB, &resident, &exam_demand("devstral-24b"));
        assert!(matches!(v, ExamAcquire::SharedLane { ref lane_id, .. } if lane_id == "live"), "got {v:?}");
    }

    // what this catches: a DIFFERENT-base exam does NOT co-tenant her lane — the verdict is
    // Spawn (a fresh copy is needed), the incident's missing explicit decision (no silent
    // second-copy grab).
    #[test]
    fn different_base_exam_decides_spawn_not_a_second_copy() {
        let resident = [live_lane("devstral-24b")];
        let v = ExamServingContext::decide(64 * GIB, &resident, &exam_demand("qwen-coder-32b"));
        assert!(matches!(v, ExamAcquire::Spawn { reclaim } if reclaim.is_empty()));
    }

    // what this catches: when even a share can't be made to fit (a tiny device already full of a
    // pinned live lane), the verdict is an honest CpuSpill — never an OOM of the resident set.
    #[test]
    fn cant_fit_even_a_share_decides_cpu_spill() {
        let resident = [live_lane("devstral-24b")];
        let capacity = resident[0].footprint().saturating_sub(GIB);
        let v = ExamServingContext::decide(capacity, &resident, &exam_demand("devstral-24b"));
        assert!(matches!(v, ExamAcquire::CpuSpill { .. }));
    }

    // what this catches: the async ACQUIRE for a share holds BOTH Ludicrous and steady for the
    // exam's lifetime and releases both on drop (RAII). With no live serving state the settle is
    // a fast no-op, so this exercises the grow→settle→pin holds without a daemon. A spawn verdict
    // holds nothing local. Guards the reverted-thrash regression: the exam pins the lane (steady)
    // AND declares Ludicrous (Performance) — both, together.
    #[tokio::test]
    async fn share_acquire_holds_ludicrous_and_steady_then_releases_on_drop() {
        // Spawn verdict first: holds nothing local, no global gauge touched.
        let resident = [live_lane("devstral-24b")];
        let spawn = ExamServingContext::acquire(64 * GIB, &resident, &exam_demand("qwen-coder-32b")).await;
        assert!(!spawn.holds(), "a spawn verdict holds no local grow/pin");

        // Share verdict (under memory pressure — a dedicated copy won't fit at 28 GiB, so the
        // isolate demand falls back to a co-tenant share): grows Ludicrous + pins steady.
        let ctx = ExamServingContext::acquire(28 * GIB, &resident, &exam_demand("devstral-24b")).await;
        assert!(ctx.holds(), "a shared-lane exam holds the grown, pinned lane");
        assert!(serving_ludicrous_active(), "Ludicrous (Performance) is declared for the exam");
        assert!(serving_held_steady(), "and the lane is pinned steady against further relaunch");
        drop(ctx);
        assert!(!serving_ludicrous_active(), "drop reverts to the pressure-adaptive mode (RAII)");
        assert!(!serving_held_steady(), "drop releases the steady pin (RAII)");
    }
}
