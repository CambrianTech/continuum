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
    fn faculties(
        &self,
        cap: &DeviceCapacity,
        req: &LeaseRequest,
        grant: &Grant,
    ) -> Vec<FacultyScore>;
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
    fn faculties(
        &self,
        cap: &DeviceCapacity,
        req: &LeaseRequest,
        grant: &Grant,
    ) -> Vec<FacultyScore> {
        // A crashed lane serves nobody — the critical faculties die together.
        let alive = !grant_would_oom(cap, req, grant);

        // Served fraction: how much of the room's demand this grant can run concurrently. The
        // surplus queues, so responsiveness tracks it. (When the lane is dead this is moot — the
        // critical gate has already zeroed the experience.)
        let want = req.want_concurrency.max(1) as f32;
        let served = (grant.concurrency as f32 / want).clamp(0.0, 1.0);
        room_faculties(alive, served)
    }
    fn name(&self) -> &'static str {
        "live-room-serving"
    }
}

/// The live-room experience SHAPE, in one place (compression principle): what senses gate the
/// room and how responsiveness weighs against thinking quality. [`LiveRoomServing`] maps a
/// single-device grant onto it; the grid simulator ([`super::grid`]) maps a multi-node placement
/// onto the SAME shape — per-node aliveness and grid-wide served fraction differ, the honest
/// truths about a room do not.
pub fn room_faculties(alive: bool, served_fraction: f32) -> Vec<FacultyScore> {
    let alive = if alive { 1.0 } else { 0.0 };
    vec![
        // The senses that make a persona a live citizen — gating. Present only if the lane
        // is alive; a crash takes all three at once.
        FacultyScore::critical("speak", alive),
        FacultyScore::critical("listen", alive),
        FacultyScore::critical("render", alive),
        // Responsiveness — heavily weighted quality in a room ("far worse to lag the
        // conversation than to slow the thinking"). Degrades with starvation, not a gate.
        FacultyScore::quality("latency", served_fraction, 8.0),
        // The model's thinking quality is unaffected by concurrency — the same brain runs.
        FacultyScore::quality("cognition", 1.0, 3.0),
    ]
}

/// Deep code / project generation — the OUTLIER-B consumer that proves the interface, chosen to
/// be maximally different from [`LiveRoomServing`] (CLAUDE.md outlier-validation discipline).
///
/// A coder in the background has NONE of a room's senses on the critical path: nobody is waiting
/// on a live avatar, so speak/listen/render aren't faculties here at all. The honest truths
/// invert:
/// - **Latency barely matters.** "Far less of an issue for a deep coder to take 2× as long." It's
///   a *quality* faculty with tiny weight — a starved grant that halves throughput is a minor
///   ding, not a degraded experience. Taking longer to think well is fine.
/// - **Correctness IS the experience — gating.** Broken code is a holistic failure no polish
///   rescues, exactly as a mute avatar is in a room. But correctness is a property of the model +
///   the work, not the concurrency grant, so the grant doesn't threaten it here — the crash mode
///   is different (an OOM kills the *job*, not the senses). This is the asymmetry that proves the
///   scorer's gate composes for a totally different faculty set without being forced.
///
/// The grant→experience mapping: a starve dips the (tiny-weight) latency faculty; an OOM kills the
/// job so `working_code` (the critical gate) collapses. Same two mechanisms as the room, wired to
/// a different, inverted faculty set — the interface holds at both extremes.
pub struct CodeGenBatch;

impl QualityModel for CodeGenBatch {
    fn faculties(
        &self,
        cap: &DeviceCapacity,
        req: &LeaseRequest,
        grant: &Grant,
    ) -> Vec<FacultyScore> {
        // An OOM kills the job — no code comes out. Otherwise the code is produced; whether it's
        // *correct* is the model's business, assumed good here (correctness is graded elsewhere by
        // the coder gym, not by the allocator). The gate the allocator can move is job-survival.
        let job_survives = if grant_would_oom(cap, req, grant) {
            0.0
        } else {
            1.0
        };

        let want = req.want_concurrency.max(1) as f32;
        let throughput = (grant.concurrency as f32 / want).clamp(0.0, 1.0);

        vec![
            // The job must produce working code — gating. A crash zeroes it; nothing else can.
            FacultyScore::critical("working_code", job_survives),
            // Solution quality dominates the graded experience: "best of the best."
            FacultyScore::quality("quality", 1.0, 10.0),
            // Throughput/latency is real but nearly weightless here — a slow deep coder is fine.
            FacultyScore::quality("latency", throughput, 1.0),
        ]
    }
    fn name(&self) -> &'static str {
        "code-gen-batch"
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
        LeaseRequest {
            consumer: "serving".into(),
            want_concurrency: 4,
            spike_bytes: 2 * GB,
        }
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
        let full =
            score_experience(&model.faculties(&cap(13), &demand(), &Grant { concurrency: 4 }));

        // Game ate the GPU; fit shrank to 3 lanes → survives, slightly less responsive.
        let shrunk =
            score_experience(&model.faculties(&cap(7), &demand(), &Grant { concurrency: 3 }));

        // Static held 4 lanes into 7GB free → OOM → the room crashes.
        let crashed =
            score_experience(&model.faculties(&cap(7), &demand(), &Grant { concurrency: 4 }));

        assert!(
            full > 0.9,
            "full demand on calm capacity is a great experience, got {full}"
        );
        assert!(
            shrunk > 0.7,
            "a graceful shrink stays a good experience, got {shrunk}"
        );
        assert!(
            crashed < 0.05,
            "an OOM crashes the room — holistic failure, got {crashed}"
        );
        assert!(
            shrunk > crashed * 10.0,
            "shrinking must be VASTLY better than crashing (shrunk={shrunk}, crashed={crashed}) \
             — this is the reward that teaches the policy to shed load instead of OOMing"
        );
    }

    // what this catches: the OUTLIER-B proof — the QualityModel + scorer interface fits a
    // MAXIMALLY DIFFERENT consumer (code-gen: no senses, correctness-gated, latency near-weightless)
    // WITHOUT forcing. The same 50%-throughput starve that noticeably dings a live room barely
    // touches a code-gen job — because the two consumers weight latency oppositely. If the
    // interface only fit the room shape (e.g. if latency weight were baked into the scorer instead
    // of chosen per-consumer), this asymmetry couldn't exist and the abstraction would be a leak.
    #[test]
    fn code_gen_shrugs_off_the_same_starve_that_dings_a_room() {
        let half = Grant { concurrency: 2 }; // half of demand=4 → 50% throughput for both
        let room = score_experience(&LiveRoomServing.faculties(&cap(13), &demand(), &half));
        let code = score_experience(&CodeGenBatch.faculties(&cap(13), &demand(), &half));

        assert!(
            code > room,
            "the same throughput starve must hurt a live room MORE than a deep-coder job \
             (room={room}, code={code}) — latency is heavy in a room, near-weightless in code-gen"
        );
        assert!(
            code > 0.9,
            "a slow-but-working code-gen job is still an excellent outcome, got {code}"
        );

        // And the gate still composes for the inverted faculty set: an OOM kills the job.
        let crashed = score_experience(&CodeGenBatch.faculties(
            &cap(7),
            &demand(),
            &Grant { concurrency: 4 },
        ));
        assert!(
            crashed < 0.05,
            "an OOM kills the code-gen job (working_code gate), got {crashed}"
        );
    }
}
