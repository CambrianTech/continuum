//! Consumer quality models — the bridge from an allocation GRANT to a lived EXPERIENCE.
//!
//! The allocator ([`super::AllocationPolicy`]) emits a [`Grant`]; the scorer ([`super::score`])
//! judges an *experience* from faculty values. Neither knows the other. A [`QualityModel`] is
//! the missing middle: given the grant a consumer actually received against this instant's
//! capacity + demand, it produces the [`FacultyScore`]s describing the resulting experience.
//! This is what makes the simulator a **perception-reward gym** instead of a mere OOM detector —
//! the learned policy (later, #126) optimizes the experience score these models produce, so it
//! chases *human-perceived quality* ("did the room stay responsive?"), never raw ops.
//!
//! Each model is one consumer's honest theory of how compute maps to experience, and is exactly
//! the thing the calibration loop (design §9) sharpens against the real benchmark ledger: today
//! the curves are principled guesses; each real serving run makes them truer.

use super::score::FacultyScore;
use super::{grant_would_oom, DeviceCapacity, Grant, LeaseRequest};

/// A consumer's theory of experience: grant + live world → faculty scores. Deterministic and
/// pure, so it runs identically in the sim and (later) as the prod reward estimator.
pub trait QualityModel: Send + Sync {
    /// The faculties that make up THIS consumer's experience under the grant it received.
    fn faculties(&self, cap: &DeviceCapacity, req: &LeaseRequest, grant: &Grant) -> Vec<FacultyScore>;
    fn name(&self) -> &'static str;
}

/// Live-room persona serving: N personas want to speak; the grant says how many may run at once.
///
/// The honest links the design demands:
/// - **Starvation → latency, gracefully.** If the grant serves fewer lanes than demanded, the
///   surplus personas queue and answer late. Latency degrades with the served fraction (C/N) —
///   a *quality* faculty, so a shrink from 4→3 lanes dips responsiveness but the room survives.
/// - **OOM → death, holistically.** A grant that overflows live GPU crashes the lane: the
///   personas cannot speak, hear, or render at all. Those are *critical* faculties, so the
///   experience collapses to ~0 — the same fact `oom_count` records, now expressed as the lived
///   catastrophe it is. This is why the fit policy (shrinks, never OOMs) must beat the static one
///   (holds, then crashes) on *experience*, not just on the OOM tally.
pub struct LiveRoomServing;

impl QualityModel for LiveRoomServing {
    fn faculties(&self, cap: &DeviceCapacity, req: &LeaseRequest, grant: &Grant) -> Vec<FacultyScore> {
        // A crashed lane serves nobody — the critical faculties die together.
        let alive = if grant_would_oom(cap, req, grant) { 0.0 } else { 1.0 };

        // Served fraction: how much of the room's demand this grant can run concurrently. The
        // surplus queues, so responsiveness tracks it. (When the lane is dead this is moot — the
        // critical gate has already zeroed the experience.)
        let want = req.want_concurrency.max(1) as f32;
        let served = (grant.concurrency as f32 / want).clamp(0.0, 1.0);

        vec![
            // The senses that make a persona a live citizen — gating. Present only if the lane
            // is alive; a crash takes all three at once.
            FacultyScore::critical("speak", alive),
            FacultyScore::critical("listen", alive),
            FacultyScore::critical("render", alive),
            // Responsiveness — heavily weighted quality in a room ("far worse to lag the
            // conversation than to slow the thinking"). Degrades with starvation, not a gate.
            FacultyScore::quality("latency", served, 8.0),
            // The model's thinking quality is unaffected by concurrency — the same brain runs.
            FacultyScore::quality("cognition", 1.0, 3.0),
        ]
    }
    fn name(&self) -> &'static str {
        "live-room-serving"
    }
}

#[cfg(test)]
mod tests {
    use super::super::score::score_experience;
    use super::*;

    const GB: u64 = 1024 * 1024 * 1024;

    fn cap(free_gb: u64) -> DeviceCapacity {
        DeviceCapacity {
            gpu_total_bytes: 55 * GB,
            gpu_free_bytes_live: free_gb * GB,
            system_ram_free_bytes: 40 * GB,
        }
    }
    fn demand() -> LeaseRequest {
        LeaseRequest { consumer: "serving".into(), want_concurrency: 4, spike_bytes: 2 * GB }
    }

    // what this catches: the honest grant→experience mapping the whole gym optimizes. A grant
    // that OOMs (4 lanes into 7GB free) crashes the room — speak/listen/render die → experience
    // collapses; a grant that fits full demand is a great experience; a grant that shrinks under
    // pressure (3 of 4 lanes) is still a GOOD experience, just slightly less responsive. If this
    // ordering ever inverts (a crash scoring near a shrink), the reward signal is lying and the
    // learned policy would learn to crash rooms.
    #[test]
    fn oom_crashes_the_room_but_a_graceful_shrink_stays_good() {
        let model = LiveRoomServing;

        // Full demand fits calm capacity → excellent.
        let full = score_experience(&model.faculties(&cap(13), &demand(), &Grant { concurrency: 4 }));

        // Game ate the GPU; fit shrank to 3 lanes → survives, slightly less responsive.
        let shrunk = score_experience(&model.faculties(&cap(7), &demand(), &Grant { concurrency: 3 }));

        // Static held 4 lanes into 7GB free → OOM → the room crashes.
        let crashed = score_experience(&model.faculties(&cap(7), &demand(), &Grant { concurrency: 4 }));

        assert!(full > 0.9, "full demand on calm capacity is a great experience, got {full}");
        assert!(shrunk > 0.7, "a graceful shrink stays a good experience, got {shrunk}");
        assert!(crashed < 0.05, "an OOM crashes the room — holistic failure, got {crashed}");
        assert!(
            shrunk > crashed * 10.0,
            "shrinking must be VASTLY better than crashing (shrunk={shrunk}, crashed={crashed}) \
             — this is the reward that teaches the policy to shed load instead of OOMing"
        );
    }
}
