//! Will-driven resolution escalation — the loop that turns a [`Will`] plus an
//! objective **verifier** into an operating point over LIVE capacity, and climbs
//! when the verifier demonstrates the current resolution was insufficient.
//!
//! This is the §2 inversion of `docs/architecture/WILL-DRIVEN-RESOLUTION.md`:
//! **detect, don't predict.** We do not try to guess up front whether a task is
//! "smart work" (any such threshold is either wasteful or dangerous). We start at
//! the resolution the will suggests, run a cheap objective verifier, and escalate on
//! *demonstrated* insufficiency — climbing resolution until verified or until live
//! capacity is genuinely exhausted (then fail loud with the honest reason,
//! [[fallbacks-are-illegal-fail-loud]]).
//!
//! For **code** this is exact and beautiful: the compiler + tests ARE the necessity
//! detector. A cheap model drafts → run tests → PASS ships (a higher-resolution run
//! saved) / FAIL *is* the escalation trigger → bump resolution → re-verify → climb
//! until PASS. "A pass with the higher model for code" falls out automatically, and
//! you **cannot regress the benchmark**, because failure is what summons the smarts.
//!
//! The three seams are traits so the spine is domain- and capacity-agnostic:
//! - [`Drafter`] produces a draft at a requested operating point (the impl maps the
//!   `[0,1]` resolution to a concrete warm model — the camera-SDK inversion).
//! - [`Verifier`] objectively checks sufficiency (code = compiler+tests, outlier A).
//! - [`ResolutionLadder`] reports the operating points live capacity affords RIGHT
//!   NOW, ascending — **no fixed lane/model count appears here** (§6 de-hardcoding
//!   contract); an empty ladder means no capacity and the escalator fails loud.

use super::will::Will;

/// The objective outcome of checking a draft. `passed` is the necessity signal:
/// `true` ships, `false` triggers escalation. `detail` names WHY (the failing test,
/// the compiler error) so a climb carries the reason into the next draft and a final
/// exhaustion can fail loud with a concrete cause rather than a bare "gave up".
#[derive(Debug, Clone)]
pub struct Verdict {
    pub passed: bool,
    pub detail: String,
}

impl Verdict {
    pub fn pass(detail: impl Into<String>) -> Self {
        Self {
            passed: true,
            detail: detail.into(),
        }
    }

    pub fn fail(detail: impl Into<String>) -> Self {
        Self {
            passed: false,
            detail: detail.into(),
        }
    }
}

/// Something went wrong that is NOT a draft failing verification (that is normal and
/// drives escalation) — it is the machinery itself unable to proceed. Surfaced loud;
/// never silently swallowed into a default ([[fallbacks-are-illegal-fail-loud]]).
#[derive(Debug, Clone, thiserror::Error)]
pub enum ResolutionError {
    /// No operating point live capacity offers clears the will's `floor` (or the
    /// ladder is empty). The persona asked for at least `floor` and the machine/grid
    /// cannot serve it right now.
    #[error("no live capacity clears the will floor {floor:.3} (rungs offered: {offered:?})")]
    NoCapacity { floor: f32, offered: Vec<f32> },
    /// A drafter could not produce a draft at all (e.g. the model call errored). The
    /// reason is carried verbatim.
    #[error("drafter failed at resolution {resolution:.3}: {reason}")]
    DraftFailed { resolution: f32, reason: String },
}

/// Produces a draft at a requested operating point. The impl maps `resolution`
/// (`[0,1]`, `0` = cheapest revisable draft, `1` = the most capable resolution live
/// capacity affords) onto a concrete warm model / compute budget. `feedback` carries
/// the previous verdict's `detail` on a re-draft so a climb is INFORMED (the higher
/// model sees why the cheaper one failed), never a blind retry.
pub trait Drafter: Send + Sync {
    type Draft: Send;
    fn draft(
        &self,
        resolution: f32,
        feedback: Option<&str>,
    ) -> impl std::future::Future<Output = Result<Self::Draft, ResolutionError>> + Send;
}

/// Objectively checks whether a draft is SUFFICIENT — the necessity detector. A PASS
/// ships; a FAIL is the escalation trigger. It is also the training label for the
/// effort predictor (WILL-DRIVEN-RESOLUTION.md §5): the gap between where the persona
/// started and what the verifier actually required calibrates the "feel".
pub trait Verifier: Send + Sync {
    type Draft: Send;
    fn verify(
        &self,
        draft: &Self::Draft,
    ) -> impl std::future::Future<Output = Verdict> + Send;
}

/// The operating points live capacity affords right now, as normalized resolutions.
/// The impl reads the governor / warm-model catalog / grid — **it never returns a
/// hardcoded count** (§6). Ascending or not, the escalator sorts; duplicates are
/// fine (two lanes at the same tier).
pub trait ResolutionLadder: Send + Sync {
    fn rungs(&self) -> Vec<f32>;
}

/// The result of resolving a will against a verifier over live capacity.
#[derive(Debug, Clone)]
pub enum Resolved<D> {
    /// The verifier passed. `draft` is sufficient; `resolution` is the operating
    /// point that achieved it; `escalations` is how many times we had to climb
    /// (0 = the first cheap draft passed — the efficient case we want to be common).
    Passed {
        draft: D,
        resolution: f32,
        escalations: u32,
        verdict: Verdict,
    },
    /// We climbed every rung live capacity offered and none passed. `last` is the
    /// best (highest-resolution) draft we produced; `verdict` carries the concrete
    /// failing reason. This is NOT a silent fallback — the caller decides policy
    /// (ship-with-warning, defer, escalate to grid) with the honest reason in hand.
    Exhausted {
        last: Option<D>,
        resolution: f32,
        escalations: u32,
        verdict: Verdict,
    },
}

/// Resolve `will` by drafting at the lowest live rung its confidence permits,
/// verifying, and climbing on failure until PASS or capacity is exhausted.
///
/// Ordering:
/// 1. Take the ladder's live rungs; keep only those clearing `will.floor()`.
/// 2. Sort ascending. Empty → [`ResolutionError::NoCapacity`] (fail loud).
/// 3. Start at the lowest rung `>= will.start_point()` (an uncertain will starts
///    cheaper; a confident one starts at `target`; a will above all rungs starts at
///    the top — necessity outranks the guess).
/// 4. Draft → verify. PASS → return. FAIL → carry the reason, climb to the next
///    higher rung, re-draft. Exhaust the ladder → [`Resolved::Exhausted`].
pub async fn resolve<Dr, V, L>(
    will: Will,
    drafter: &Dr,
    verifier: &V,
    ladder: &L,
) -> Result<Resolved<Dr::Draft>, ResolutionError>
where
    Dr: Drafter,
    V: Verifier<Draft = Dr::Draft>,
    L: ResolutionLadder,
{
    let offered = ladder.rungs();
    let mut rungs: Vec<f32> = offered.iter().copied().filter(|r| will.accepts(*r)).collect();
    rungs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    if rungs.is_empty() {
        return Err(ResolutionError::NoCapacity {
            floor: will.floor(),
            offered,
        });
    }

    // Lowest rung at/above the will's draft point; if the will asks above everything
    // available, start at the top rung (the most capable resolution we can serve).
    let start = will.start_point();
    let start_idx = rungs
        .iter()
        .position(|r| *r + f32::EPSILON >= start)
        .unwrap_or(rungs.len() - 1);

    let mut attempts: u32 = 0;
    let mut feedback: Option<String> = None;
    let mut last_draft: Option<Dr::Draft> = None;
    let mut last_verdict = Verdict::fail("no draft attempted");

    for &res in &rungs[start_idx..] {
        attempts += 1;
        let draft = drafter.draft(res, feedback.as_deref()).await?;
        let verdict = verifier.verify(&draft).await;
        if verdict.passed {
            return Ok(Resolved::Passed {
                draft,
                resolution: res,
                escalations: attempts - 1,
                verdict,
            });
        }
        feedback = Some(verdict.detail.clone());
        last_verdict = verdict;
        last_draft = Some(draft);
    }

    Ok(Resolved::Exhausted {
        last: last_draft,
        resolution: *rungs.last().expect("rungs non-empty checked above"),
        escalations: attempts.saturating_sub(1),
        verdict: last_verdict,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A ladder of fixed rungs — stands in for the live warm-model catalog in tests.
    struct FixedLadder(Vec<f32>);
    impl ResolutionLadder for FixedLadder {
        fn rungs(&self) -> Vec<f32> {
            self.0.clone()
        }
    }

    /// A drafter that records every resolution it was asked to draft at, so tests can
    /// assert the escalation PATH (which rungs were tried, in order).
    struct RecordingDrafter {
        tried: std::sync::Mutex<Vec<f32>>,
    }
    impl RecordingDrafter {
        fn new() -> Self {
            Self {
                tried: std::sync::Mutex::new(Vec::new()),
            }
        }
    }
    impl Drafter for RecordingDrafter {
        type Draft = f32; // the draft IS the resolution it was drafted at
        async fn draft(
            &self,
            resolution: f32,
            _feedback: Option<&str>,
        ) -> Result<f32, ResolutionError> {
            self.tried.lock().unwrap().push(resolution);
            Ok(resolution)
        }
    }

    /// Verifier that passes once the draft's resolution reaches a threshold — the
    /// "how much resolution this task actually required" oracle.
    struct ThresholdVerifier {
        required: f32,
        calls: AtomicU32,
    }
    impl Verifier for ThresholdVerifier {
        type Draft = f32;
        async fn verify(&self, draft: &f32) -> Verdict {
            self.calls.fetch_add(1, Ordering::AcqRel);
            if *draft + f32::EPSILON >= self.required {
                Verdict::pass(format!("met requirement {:.2}", self.required))
            } else {
                Verdict::fail(format!("needs >= {:.2}, got {:.2}", self.required, draft))
            }
        }
    }

    // what this catches: an EASY task passes at the first (cheapest) drafted rung —
    // zero escalations, one verify — which is the compute-saving case the whole design
    // exists for. The cheap model must not be skipped when it suffices.
    #[tokio::test]
    async fn easy_task_passes_cheap_no_escalation() {
        let ladder = FixedLadder(vec![0.2, 0.5, 0.9]);
        let drafter = RecordingDrafter::new();
        let verifier = ThresholdVerifier {
            required: 0.2,
            calls: AtomicU32::new(0),
        };
        // Confident low will → starts at the cheapest rung.
        let will = Will::new(0.2, 0.1, 0.0);
        let out = resolve(will, &drafter, &verifier, &ladder).await.unwrap();
        match out {
            Resolved::Passed {
                resolution,
                escalations,
                ..
            } => {
                assert!((resolution - 0.2).abs() < 1e-6);
                assert_eq!(escalations, 0, "cheap draft sufficed");
            }
            other => panic!("expected Passed, got {other:?}"),
        }
        assert_eq!(*drafter.tried.lock().unwrap(), vec![0.2], "only the cheap rung drafted");
    }

    // what this catches: a HARD task that the cheap rungs cannot satisfy CLIMBS the
    // ladder and still passes at the rung that meets the requirement — the "failure
    // summons the smarts, benchmark not regressed" guarantee. The path proves each
    // lower rung was tried and rejected by the objective verifier before escalating.
    #[tokio::test]
    async fn hard_task_escalates_until_verified() {
        let ladder = FixedLadder(vec![0.2, 0.5, 0.9]);
        let drafter = RecordingDrafter::new();
        let verifier = ThresholdVerifier {
            required: 0.85, // only the top rung passes
            calls: AtomicU32::new(0),
        };
        // Uncertain will → starts cheap and leans on escalation.
        let will = Will::new(0.5, 0.1, 1.0);
        let out = resolve(will, &drafter, &verifier, &ladder).await.unwrap();
        match out {
            Resolved::Passed {
                resolution,
                escalations,
                ..
            } => {
                assert!((resolution - 0.9).abs() < 1e-6, "passed at top rung");
                assert!(escalations >= 1, "had to climb");
            }
            other => panic!("expected Passed after climb, got {other:?}"),
        }
        let tried = drafter.tried.lock().unwrap().clone();
        assert_eq!(tried, vec![0.2, 0.5, 0.9], "climbed every rung in order");
    }

    // what this catches: when NO live rung clears the will's floor, the escalator
    // fails LOUD with the floor and what was offered — never silently drafts below
    // stakes. This is the anti-fallback guarantee at the capacity boundary.
    #[tokio::test]
    async fn floor_above_all_capacity_fails_loud() {
        let ladder = FixedLadder(vec![0.2, 0.4]);
        let drafter = RecordingDrafter::new();
        let verifier = ThresholdVerifier {
            required: 0.0,
            calls: AtomicU32::new(0),
        };
        let will = Will::new(0.9, 0.8, 0.1); // floor 0.8 > every rung
        let err = resolve(will, &drafter, &verifier, &ladder).await.unwrap_err();
        match err {
            ResolutionError::NoCapacity { floor, offered } => {
                assert!((floor - 0.8).abs() < 1e-6);
                assert_eq!(offered, vec![0.2, 0.4]);
            }
            other => panic!("expected NoCapacity, got {other:?}"),
        }
        assert!(drafter.tried.lock().unwrap().is_empty(), "never drafted below floor");
    }

    // what this catches: a task nothing available can satisfy climbs to the top and
    // returns Exhausted (with the concrete failing reason), NOT a fabricated pass.
    // The caller owns the ship-anyway/defer/grid decision with an honest verdict.
    #[tokio::test]
    async fn unsatisfiable_task_exhausts_with_reason() {
        let ladder = FixedLadder(vec![0.3, 0.6]);
        let drafter = RecordingDrafter::new();
        let verifier = ThresholdVerifier {
            required: 0.99, // no rung passes
            calls: AtomicU32::new(0),
        };
        let will = Will::new(0.3, 0.1, 1.0);
        let out = resolve(will, &drafter, &verifier, &ladder).await.unwrap();
        match out {
            Resolved::Exhausted {
                resolution,
                verdict,
                ..
            } => {
                assert!((resolution - 0.6).abs() < 1e-6, "reports the top rung reached");
                assert!(!verdict.passed && verdict.detail.contains("0.99"));
            }
            other => panic!("expected Exhausted, got {other:?}"),
        }
        assert_eq!(*drafter.tried.lock().unwrap(), vec![0.3, 0.6], "tried all rungs");
    }
}
