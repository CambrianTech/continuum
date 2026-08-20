//! What ONE benchmark attempt's settle MEANS — the pure classifier that stands
//! between "the substrate failed her" and "she produced a real result".
//!
//! # Why this file exists (glass-boxed 2026-08-16, run `claim-109172fa-…`)
//!
//! `agent/solve` used to fold two genuinely different endings into one arm:
//!
//! ```ignore
//! if r.infra_error.is_some() || (r.acts == 0 && r.patch.is_empty()) { … }
//! ```
//!
//! …and then emitted ONE probe whose prose was hardcoded to the SECOND disjunct:
//! *"attempt produced ZERO work with no error — infra void (serving transition)"*.
//!
//! On that run the FIRST disjunct fired. The ledger
//! (`~/.continuum/progress/agent-solve-claim-109172fa-….json`) recorded
//! `acts: 19`, `files_changed: ["astropy/modeling/separable.py"]`, a 12,937-byte
//! patch, and an `infra_error` naming a served-model swap. The wire said she did
//! nothing. A reader (human or citizen) who trusts the probe stream — which is the
//! whole point of the probe stream — concludes a citizen who worked 19 acts and
//! wrote a real patch "produced ZERO work". That is a lying receipt, the class this
//! repo hunts (#151/#357), and it cost a debugging session.
//!
//! The label was also unearned in the other direction: "serving transition" was
//! asserted for EVERY zero-work ending, including ones where serving never moved.
//! An attempt that is not attributable to infrastructure must not be attributed to
//! infrastructure — otherwise the harness launders a capability result into an
//! infra excuse, retries it for 90s × N, and the round burns hours producing no
//! measurement at all.
//!
//! # The contract
//!
//! One pure function over EVIDENCE — the settle's own numbers plus the serving
//! snapshot observed at the attempt's start and end. No I/O, no clock, so the
//! table test below pins every arm without a live lane.

/// Which infrastructure failed her — named, never guessed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InfraKind {
    /// The served model moved underneath the attempt (a re-home / model swap), or
    /// the gateway refused because the pinned model is no longer resident. THIS is
    /// the ending that earns the words "serving transition".
    ServingTransition,
    /// The deliberation call failed for some other named reason (timeout, 5xx,
    /// stream read error) with no evidence that serving itself moved.
    InferenceFault,
}

impl InfraKind {
    /// The wire word — stable, greppable, one truth for probe + ledger prose.
    pub fn as_str(self) -> &'static str {
        match self {
            InfraKind::ServingTransition => "serving_transition",
            InfraKind::InferenceFault => "inference_fault",
        }
    }
}

/// What the attempt loop must DO with this settle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttemptDisposition {
    /// A real ending on a working lane — grade it. Includes an honest empty-diff
    /// settle: producing nothing while the substrate worked IS a result.
    Grade,
    /// A NAMED infrastructure fault. Her chances must not be burned; the attempt is
    /// retried in the same workspace (her partial work survives there).
    InfraFault { kind: InfraKind, cause: String },
    /// Zero acts, empty patch, NO named error, and serving never moved. Nothing here
    /// is attributable to infrastructure — so nothing here may CLAIM infrastructure.
    /// Loud and distinct: it grades as the zero it is, and the probe says exactly
    /// that, so a silent-settle regression surfaces as itself instead of hiding for
    /// hours behind an infra retry loop.
    SilentVoid,
}

/// The gateway's refusal marker for "the model you are pinned to is not the one
/// being served". Matching on this substring is matching on OUR OWN contract string
/// (`ai::openai_adapter::unguaranteed_model_refusal`), not on a foreign format.
pub const SERVED_MODEL_REFUSAL_MARKER: &str = "is not the active served model";
/// The gateway's refusal marker for "nothing is resident right now" — the other
/// half of the same serving transition (published on every teardown/re-home).
pub const NO_MODEL_RESIDENT_MARKER: &str = "no model is resident right now";

/// Everything the classifier is allowed to look at. Borrowed, so callers pass their
/// live values with no allocation.
#[derive(Debug, Clone, Copy)]
pub struct AttemptEvidence<'a> {
    /// Acts the settle actually executed (the drive sums re-drives into this).
    pub acts: u32,
    /// Bytes of the workspace diff the grader would read.
    pub patch_bytes: usize,
    /// `Some(cause)` when the deliberation path failed rather than settling.
    pub infra_error: Option<&'a str>,
    /// The served model id observed when this attempt STARTED.
    pub served_model_at_start: Option<&'a str>,
    /// The served model id observed when this attempt ENDED. A difference is
    /// MEASURED evidence that the lane moved under her — not an assumption.
    pub served_model_at_end: Option<&'a str>,
}

impl AttemptEvidence<'_> {
    /// Did serving demonstrably move during the attempt?
    pub fn serving_moved(&self) -> bool {
        self.served_model_at_start != self.served_model_at_end
    }

    /// Does the failure cause itself name a serving transition?
    fn cause_names_serving(&self) -> bool {
        self.infra_error.is_some_and(|c| {
            c.contains(SERVED_MODEL_REFUSAL_MARKER) || c.contains(NO_MODEL_RESIDENT_MARKER)
        })
    }

    /// True when the attempt produced something a grader can read.
    pub fn produced_work(&self) -> bool {
        self.acts > 0 || self.patch_bytes > 0
    }
}

/// Classify one attempt's ending from its evidence. Pure.
///
/// Precedence is deliberate: a NAMED infra fault outranks the work counters (#386 —
/// an attempt whose settle carries an inference error died to infrastructure by
/// definition, however much she had done before it), and "serving transition" is
/// only ever claimed when serving evidence supports it.
pub fn classify_attempt(ev: AttemptEvidence<'_>) -> AttemptDisposition {
    if let Some(cause) = ev.infra_error {
        let kind = if ev.serving_moved() || ev.cause_names_serving() {
            InfraKind::ServingTransition
        } else {
            InfraKind::InferenceFault
        };
        return AttemptDisposition::InfraFault {
            kind,
            cause: cause.to_string(),
        };
    }
    if ev.produced_work() {
        return AttemptDisposition::Grade;
    }
    // Zero work with no named error. The ONLY thing that can make this infra is
    // measured serving movement (#384's F1 signature: null-decision ticks while the
    // lane was being swapped). Without it, the harness has no standing to claim a
    // fault it cannot name.
    if ev.serving_moved() {
        return AttemptDisposition::InfraFault {
            kind: InfraKind::ServingTransition,
            cause: format!(
                "attempt produced zero acts and an empty patch while the served model \
                 moved from {} to {} — the lane was swapped under the drive (#384)",
                ev.served_model_at_start.unwrap_or("<none>"),
                ev.served_model_at_end.unwrap_or("<none>"),
            ),
        };
    }
    AttemptDisposition::SilentVoid
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev<'a>(
        acts: u32,
        patch_bytes: usize,
        infra_error: Option<&'a str>,
        start: Option<&'a str>,
        end: Option<&'a str>,
    ) -> AttemptEvidence<'a> {
        AttemptEvidence {
            acts,
            patch_bytes,
            infra_error,
            served_model_at_start: start,
            served_model_at_end: end,
        }
    }

    const DEVSTRAL: &str = "unsloth/Devstral-Small-2507-GGUF";
    const QWEN: &str = "ggml-org/Qwen3.8-27B-GGUF";
    // The verbatim cause from run claim-109172fa-b5eb-4259-9d40-39bd0a4dab00.
    const LIVE_SWAP_CAUSE: &str = "llama-server (local OpenAI-compatible gateway): model \
                                   'unsloth/Devstral-Small-2507-GGUF' is not the active served \
                                   model (serving: ggml-org/Qwen3.8-27B-GGUF, ready: true); the \
                                   serving daemon owns which single model is resident";

    // what this catches: the lying receipt of run claim-109172fa — 19 acts and a
    // 12,937-byte patch classified by a branch whose probe prose says "produced ZERO
    // work with no error". The disposition must be an infra fault that CARRIES her
    // work forward as evidence, and the caller must never be able to print
    // "zero work" for it (the evidence says otherwise).
    #[test]
    fn a_swapped_lane_after_real_work_is_a_serving_transition_not_a_zero() {
        let e = ev(19, 12_937, Some(LIVE_SWAP_CAUSE), Some(DEVSTRAL), Some(QWEN));
        assert!(e.produced_work(), "19 acts + a 12kB patch IS work");
        assert_eq!(
            classify_attempt(e),
            AttemptDisposition::InfraFault {
                kind: InfraKind::ServingTransition,
                cause: LIVE_SWAP_CAUSE.to_string(),
            }
        );
    }

    // what this catches: "serving transition" asserted with no serving evidence. A
    // stream read error on a lane that never moved is an inference fault — still
    // infra (unburned retry), but it must not name a transition that did not happen.
    #[test]
    fn an_inference_fault_on_a_steady_lane_is_not_called_a_serving_transition() {
        let e = ev(
            3,
            0,
            Some("llama-server: stream read error: error decoding response body"),
            Some(DEVSTRAL),
            Some(DEVSTRAL),
        );
        assert_eq!(
            classify_attempt(e),
            AttemptDisposition::InfraFault {
                kind: InfraKind::InferenceFault,
                cause: "llama-server: stream read error: error decoding response body".to_string(),
            }
        );
    }

    // what this catches: the #384 protection surviving the rewrite BY EVIDENCE. Zero
    // acts, empty patch, no error, but the served model moved mid-attempt — that IS
    // the F1 signature and must still retry unburned rather than grade as capability.
    #[test]
    fn zero_work_during_a_measured_model_swap_stays_infra() {
        let e = ev(0, 0, None, Some(DEVSTRAL), Some(QWEN));
        match classify_attempt(e) {
            AttemptDisposition::InfraFault { kind, cause } => {
                assert_eq!(kind, InfraKind::ServingTransition);
                assert!(cause.contains(DEVSTRAL) && cause.contains(QWEN), "{cause}");
            }
            other => panic!("a measured swap must stay infra, got {other:?}"),
        }
    }

    // what this catches: the retry-loop hole. Zero work, no error, serving steady —
    // the harness has NO evidence of a fault, so it must not claim one (and must not
    // burn 90s × N retries producing no measurement). This grades as the zero it is.
    #[test]
    fn zero_work_on_a_steady_lane_is_a_silent_void_never_an_infra_claim() {
        assert_eq!(
            classify_attempt(ev(0, 0, None, Some(DEVSTRAL), Some(DEVSTRAL))),
            AttemptDisposition::SilentVoid
        );
        // …and with serving never observed at all (no snapshot either side), the
        // absence of evidence is still not evidence of a transition.
        assert_eq!(
            classify_attempt(ev(0, 0, None, None, None)),
            AttemptDisposition::SilentVoid
        );
    }

    // what this catches: an honest settle being stolen from the grader. Acts with an
    // empty diff, or a patch with no error, are RESULTS — the empty-diff re-drive and
    // the verifier already had their say by the time we get here.
    #[test]
    fn work_on_a_working_lane_grades() {
        assert_eq!(
            classify_attempt(ev(12, 0, None, Some(DEVSTRAL), Some(DEVSTRAL))),
            AttemptDisposition::Grade
        );
        assert_eq!(
            classify_attempt(ev(0, 400, None, Some(DEVSTRAL), Some(DEVSTRAL))),
            AttemptDisposition::Grade
        );
    }

    // what this catches: the cause-marker path, for the case where the snapshot
    // reads identical at both ends because the daemon swapped BACK before the
    // attempt returned. The refusal text itself is our own contract string and is
    // sufficient serving evidence on its own.
    #[test]
    fn a_served_model_refusal_names_a_transition_even_when_the_snapshot_settled_back() {
        let e = ev(5, 0, Some(LIVE_SWAP_CAUSE), Some(DEVSTRAL), Some(DEVSTRAL));
        assert!(matches!(
            classify_attempt(e),
            AttemptDisposition::InfraFault {
                kind: InfraKind::ServingTransition,
                ..
            }
        ));
        let transition = "llama-server: no model is resident right now (the serving daemon is \
                          between lanes)";
        let e2 = ev(0, 0, Some(transition), Some(DEVSTRAL), Some(DEVSTRAL));
        assert!(matches!(
            classify_attempt(e2),
            AttemptDisposition::InfraFault {
                kind: InfraKind::ServingTransition,
                ..
            }
        ));
    }
}
