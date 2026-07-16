//! The scoring tree — compositional, GATED, context-weighted, non-linear.
//!
//! The objective the allocator optimizes is not a flat weighted sum; its *structure* encodes
//! honest truths about what ruins an experience (design §9). This module makes the load-bearing
//! one — **gating** — executable and tested: a live-room persona with excellent cognition but no
//! ability to *speak* is not "0.9 minus a bit," it's a **holistic failure**. A `min`/product gate
//! on critical faculties makes the score refuse to call a broken whole good.
//!
//! The mode (live-room vs code-gen vs solo) chooses each faculty's [`Role`] — which are gating
//! (an experience can't survive without them) and which are weighted quality with how much
//! weight. Same faculties, different composition per situation: a live room can't survive a mute
//! avatar; a code-gen job can't survive broken code but shrugs off latency.

/// A faculty's role in ONE experience mode.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Role {
    /// The experience cannot survive without it — GATING (multiplicative). A low value drives
    /// the whole score toward zero regardless of everything else. (Speak/listen/render in a
    /// live room; working code in a code-gen job.)
    Critical,
    /// Contributes proportionally to its weight — weighted quality, not a gate. (Latency in a
    /// code-gen job; avatar polish in a room.)
    Quality { weight: f32 },
}

/// One faculty's normalized outcome (0.0 worst .. 1.0 best) + its role THIS mode.
#[derive(Debug, Clone, Copy)]
pub struct FacultyScore {
    pub name: &'static str,
    pub value: f32,
    pub role: Role,
}

impl FacultyScore {
    pub fn critical(name: &'static str, value: f32) -> Self {
        Self { name, value: value.clamp(0.0, 1.0), role: Role::Critical }
    }
    pub fn quality(name: &'static str, value: f32, weight: f32) -> Self {
        Self { name, value: value.clamp(0.0, 1.0), role: Role::Quality { weight: weight.max(0.0) } }
    }
}

/// Compose faculty scores into ONE honest experience scalar in 0.0..=1.0.
///
/// `overall = product(critical values) × weighted_mean(quality values)`.
///
/// - The **product over critical faculties** is the gate: any near-zero critical faculty
///   (a mute avatar, broken code) drags the whole score to near-zero — a holistic failure that
///   no amount of quality polish rescues. This is the "honest truth" the design demands.
/// - The **weighted mean over quality faculties** is the graded part; the mode's weights make
///   it context-aware (latency heavy in a live room, near-zero in code-gen).
/// - No critical faculties → the gate is 1.0 (a solo coder needs no speak/listen). No quality
///   faculties → the score is the gate alone.
pub fn score_experience(faculties: &[FacultyScore]) -> f32 {
    let mut gate = 1.0_f32;
    let mut q_weighted = 0.0_f32;
    let mut q_weight = 0.0_f32;
    let mut any_quality = false;
    for f in faculties {
        match f.role {
            Role::Critical => gate *= f.value,
            Role::Quality { weight } => {
                any_quality = true;
                q_weighted += weight * f.value;
                q_weight += weight;
            }
        }
    }
    let quality = if any_quality && q_weight > 0.0 {
        q_weighted / q_weight
    } else {
        1.0 // no graded quality axis → the gate is the whole story
    };
    (gate * quality).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: THE honest-truth lever. A live-room persona with excellent cognition,
    // vision, hearing, and render — but no ability to SPEAK — is a holistic FAILURE, not a
    // near-perfect score. If a future refactor made critical faculties additive (weighted-sum),
    // a mute avatar would score ~0.8 and the allocator would happily keep shipping a broken
    // experience. The gate forbids that: pull one critical faculty → the whole score collapses.
    #[test]
    fn pulling_one_critical_faculty_collapses_the_whole_experience() {
        let excellent = [
            FacultyScore::critical("see", 1.0),
            FacultyScore::critical("listen", 1.0),
            FacultyScore::critical("speak", 1.0),
            FacultyScore::critical("render", 1.0),
            FacultyScore::quality("cognition", 1.0, 3.0),
            FacultyScore::quality("latency", 0.9, 2.0),
        ];
        let good = score_experience(&excellent);
        assert!(good > 0.9, "all faculties present + strong → a great experience, got {good}");

        // Pull ONLY speak; everything else stays perfect.
        let mut muted = excellent;
        muted[2] = FacultyScore::critical("speak", 0.0);
        let broken = score_experience(&muted);
        assert!(
            broken < 0.05,
            "a mute avatar in a live room is a HOLISTIC FAILURE even with perfect cognition, \
             vision, hearing, and render — the gate must collapse the whole score, got {broken}"
        );
    }

    // what this catches: context-weighting asymmetry — the SAME poor latency (0.2) is a minor
    // ding in code/project generation (quality is what matters; latency barely weighted) but a
    // severe hit in a live room (responsiveness is critical). "Far worse to lag the conversation
    // than to slow the thinking." If the weights ever became mode-independent, a slow-but-brilliant
    // code-gen would be wrongly punished and a laggy live room wrongly forgiven.
    #[test]
    fn latency_is_penalized_by_context_not_uniformly() {
        // Code-gen: quality dominates, latency barely weighted. Slow but excellent → still high.
        let codegen_slow = [
            FacultyScore::critical("working_code", 1.0),
            FacultyScore::quality("quality", 1.0, 10.0),
            FacultyScore::quality("latency", 0.2, 1.0),
        ];
        // Live-room: latency is heavily weighted. Same 0.2 latency, otherwise fine → hit harder.
        let liveroom_laggy = [
            FacultyScore::critical("speak", 1.0),
            FacultyScore::quality("relevance", 1.0, 3.0),
            FacultyScore::quality("latency", 0.2, 8.0),
        ];
        let codegen = score_experience(&codegen_slow);
        let liveroom = score_experience(&liveroom_laggy);
        assert!(
            codegen > liveroom,
            "the same poor latency must hurt a live room more than a code-gen job \
             (codegen={codegen}, liveroom={liveroom})"
        );
        assert!(codegen > 0.85, "slow-but-excellent code-gen stays strong, got {codegen}");
    }
}
