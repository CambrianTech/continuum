//! `BaseModelPolicy` — the ONE answer to "what base model does this citizen think on?"
//!
//! ## Why this type exists
//!
//! `resolve_model_for_persona` grew FOUR ways to answer that question, each silently
//! handing off to the next:
//!
//! | precedence | source | what it meant |
//! |---|---|---|
//! | 0 | `override_model` | runtime per-persona assignment |
//! | 1 | `model_preferences` | tiered ladder, "best that fits" |
//! | 2 | `model_id` | explicit per-persona model, labelled *"Legacy"* |
//! | 3 | `default_local_model` | system-wide default |
//!
//! Four expressions of one decision is the compression violation the whole codebase is
//! built against, and it had two concrete costs. First, nobody could answer "where is the
//! base model governed?" — the honest answer was "in four places." Second, and worse, the
//! hand-offs are **silent downgrades**: rows 1→2→3 walk *down* without anyone deciding to,
//! which is the 2026-08-15 incident shape (#438) — one bogus `usable_gb=0` sample walked
//! the ladder to the bottom and served citizens a 0.5B that emitted template-token garbage
//! into the room. The sibling module already forbids exactly this in prose
//! (`inference_profile.rs`: *"substrate HARD ERRORS with diagnosis instead of silently
//! degrading"*) — the prose was right and the code did not implement it.
//!
//! ## The genome is what makes a base REACHABLE
//!
//! A LoRA adapter is a **per-base derivative**. The durable asset is the CORPUS
//! (transcripts, tool traces, solved instances), which is base-independent; an adapter is
//! forged from that corpus *onto a specific base* and is invalid on any other (#369).
//!
//! So a citizen is not stuck on one base — she can scale up to a bigger one or down to a
//! smaller one **provided her genome has been forged for the destination**. That is the
//! actual constraint, and it is why this module takes a [`GenomeCoverage`] rather than
//! treating the ladder as freely walkable: a rung she has no adapter for is a rung where
//! she thinks with base capability only. Reachable, sometimes correct, but *a different
//! citizen* than the one the previous round measured.
//!
//! The paired obligation lives in the forge, not here: **one corpus fans out to N
//! adapters, one per targeted base, each trained independently against that base.** A
//! ladder is only as walkable as the forge made it. A policy declaring four rungs while
//! the forge only ever targeted one is a ladder with three rungs missing, and
//! [`RungPolicy::RequireGenome`] is what turns that from a silent capability cliff into a
//! visible one.
//!
//! Two consequences, and they are why benchmarks need this type:
//!
//! 1. **Genome lift is only measurable against a fixed base.** Round 1 on a known base →
//!    corpus accrues → forge an adapter onto that same base → round 2, same base + adapter
//!    → the delta is attributable to the genome. Float the base between rounds and the
//!    number means nothing AND the round-1 adapters are garbage on the new base.
//! 2. **A silent re-base corrupts the experience, not just the score.** Turns taken on an
//!    unintended base still land in the corpus. A round that quietly slid to a 0.5B
//!    doesn't merely score badly — it poisons the training data the next adapter is forged
//!    from.
//!
//! So refusing to serve is the *cheap* failure. Serving on an unintended base is the
//! expensive one, and it is the one that used to happen by default.
//!
//! ## What this module does NOT decide
//!
//! Whether the weights are on disk, whether a lane can be spawned, whether the host is
//! under pressure right now. This is a pure policy → `(model, vram_budget)` decision over
//! a declared host VRAM figure, so it is unit-testable without a GPU, a registry, or a
//! running lane. Availability and lane admission stay where they already live
//! (`model_registry`, `inference::llama_server`), and a caller that resolves a model it
//! then cannot load must fail loudly there — not by coming back here for a second guess.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// One rung of an [`BaseModelPolicy::Adaptive`] ladder: a model that applies when the host
/// has at least `min_vram_gb`.
///
/// Structurally identical to the existing catalog `ModelPreference` (this is deliberately
/// the same shape, so the catalog's ladders port over unchanged rather than growing a
/// parallel encoding of "which model at which size").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/BaseModelRung.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct BaseModelRung {
    /// Minimum total host VRAM (GB) for this rung to apply.
    pub min_vram_gb: f64,
    /// The model id this rung selects.
    pub model: String,
    /// VRAM this persona needs when thinking on this model.
    pub vram_budget_gb: f64,
}

/// WHY a citizen is pegged — carried so a refusal can explain itself, and so the
/// measurement peg can be told apart from a durable one when a lease expires.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/persona/PegReason.ts")]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum PegReason {
    /// A measurement is in flight and the number must be attributable to a known base.
    /// Held for the round's duration and released with it — the round-lifecycle owner
    /// (#371) sets and clears this, never the solver.
    Measurement { run_id: String },
    /// A FORGE RUN is in flight and is targeting this base.
    ///
    /// This is the peg with the most utility, because continuous learning never stops: the
    /// flywheel runs turns → corpus → forge → adapter → page-in continuously, and the
    /// adapter coming out the far end is a derivative of whatever base it was trained
    /// against. If her base floats between the corpus accruing and the forge finishing, the
    /// adapter lands for a base she has already left — forged for nobody, and page-in must
    /// then refuse it (#369).
    ///
    /// So a training run holds this for its duration, exactly as a measurement round holds
    /// [`Self::Measurement`], and for the same underlying reason: a process is in flight
    /// whose output is only valid against the base it started on.
    Training { job_id: String },
    /// Her genome's adapters are forged against this base and are invalid on any other
    /// (#369). Derived from the adapters she actually holds, not hand-declared — the
    /// standing consequence of past [`Self::Training`] runs.
    GenomeBound,
    /// Operator intent — "this citizen runs on this model."
    Operator,
}

/// What base model a citizen thinks on. ONE decision, one place.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/BaseModelPolicy.ts"
)]
#[serde(rename_all = "camelCase", tag = "mode")]
pub enum BaseModelPolicy {
    /// Pinned: this base or nothing. Subsumes the old `override_model` (as a
    /// [`PegReason::Measurement`] peg with a lease) and the old `model_id` (as
    /// [`PegReason::Operator`]).
    Pegged {
        model: String,
        vram_budget_gb: f64,
        reason: PegReason,
    },
    /// Governed within bounds: walk the ladder for the best rung the host can carry AND
    /// her genome can reach, and REFUSE rather than descend past `floor`.
    ///
    /// `floor` names a model that must appear in `ladder`. It is a named member rather
    /// than an index so a ladder can be reordered or extended without silently moving the
    /// floor — the failure mode of every "last entry wins" rule.
    Adaptive {
        ladder: Vec<BaseModelRung>,
        floor: String,
        /// What to do with a rung her genome was never forged for.
        #[serde(default)]
        rungs: RungPolicy,
    },
}

/// What a rung with no adapter means for this citizen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/persona/RungPolicy.ts")]
#[serde(rename_all = "camelCase")]
pub enum RungPolicy {
    /// Skip rungs her genome has not been forged for. The default, because a citizen who
    /// silently loses her genome mid-ladder is the same class of surprise as one who
    /// silently changes base — she is measurably a different worker and nothing said so.
    ///
    /// This makes an unforged ladder VISIBLE: declare four rungs, forge one, and she
    /// resolves to the one that exists instead of appearing to have a four-rung range.
    #[default]
    RequireGenome,
    /// Take the best-fitting rung regardless, thinking with base capability alone where
    /// no adapter exists. Legitimate — a bare frontier base may well beat a small
    /// genome-backed one — but the result is flagged
    /// [`ResolvedBase::genome_backed`]` == false` so a measurement records WHICH citizen
    /// it scored, and a lift comparison can refuse to compare across that line.
    AllowBare,
}

/// Which bases this citizen's genome has actually been forged for.
///
/// Derived from the adapters on disk, never declared — a config field claiming coverage
/// the forge never produced is exactly the lying-receipt shape this module exists to end.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GenomeCoverage {
    forged_for: std::collections::BTreeSet<String>,
}

impl GenomeCoverage {
    /// Build from the base ids her adapters declare (#369 — every adapter carries the
    /// `base_model_id` it was forged against).
    pub fn from_adapter_bases<I, S>(bases: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            forged_for: bases.into_iter().map(Into::into).collect(),
        }
    }

    /// Has her corpus been forged onto this base?
    pub fn covers(&self, model: &str) -> bool {
        self.forged_for.contains(model)
    }

    /// Every base she can think on WITH her genome — the honest answer to "how far can
    /// she scale?", and the work-list for the forge fan-out when the answer is "not far".
    pub fn bases(&self) -> impl Iterator<Item = &str> {
        self.forged_for.iter().map(String::as_str)
    }
}

/// The resolved answer: which model, the VRAM she needs on it, and whether her genome
/// actually reaches it.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedBase {
    pub model: String,
    pub vram_budget_gb: f64,
    /// Is her corpus forged onto THIS base?
    ///
    /// `false` means she is thinking with base capability alone. Load-bearing for
    /// measurement: a score from a bare rung and a score from a genome-backed rung are
    /// numbers about two different workers, and a lift comparison that mixes them is
    /// measuring the base swap, not the genome.
    pub genome_backed: bool,
}

/// Why a policy could not be satisfied. Every variant is a REFUSAL that names its own
/// remedy — there is no "and so we used something smaller" arm, by construction.
#[derive(Debug, Clone, PartialEq)]
pub enum BaseModelError {
    /// A peg that does not fit this host. She is honestly absent rather than silently
    /// re-based: a citizen serving on a base her adapters were not forged against
    /// produces turns that corrupt the corpus AND a score attributable to nothing.
    PegDoesNotFit {
        model: String,
        reason: PegReason,
        needs_gb: f64,
        host_gb: f64,
    },
    /// Even the declared floor does not fit. The host is too small for this citizen as
    /// configured; lowering the floor is a DECISION, not something resolution may take.
    FloorDoesNotFit {
        floor: String,
        needs_gb: f64,
        host_gb: f64,
    },
    /// The floor names a model absent from the ladder — a malformed policy, caught at
    /// resolution rather than silently ignored (an unenforceable floor is worse than no
    /// floor, because it reads as protection).
    FloorNotOnLadder { floor: String },
    /// An adaptive policy with no rungs. There is no system-wide default to fall back to:
    /// that fallback WAS the bug.
    EmptyLadder,
    /// Rungs FIT this host, but her genome was never forged onto any of them. Not a
    /// hardware problem — a **forge** problem, and the remedy is a training run, so the
    /// refusal carries the exact work-list: which bases to target, and which she already
    /// has. This is the state a declared-but-unforged ladder is actually in, made visible
    /// instead of silently handing back a citizen without her genome.
    NoForgedRungFits {
        fits_but_unforged: Vec<String>,
        forged_for: Vec<String>,
    },
}

impl std::fmt::Display for BaseModelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PegDoesNotFit {
                model,
                reason,
                needs_gb,
                host_gb,
            } => write!(
                f,
                "pegged base {model} needs {needs_gb:.1} GB but this host has {host_gb:.1} GB \
                 (peg held because: {reason:?}). Refusing to re-base her: her genome's \
                 adapters are forged against {model} and her turns would corrupt the corpus. \
                 Free VRAM, move her to a host that fits, or change the peg deliberately."
            ),
            Self::FloorDoesNotFit {
                floor,
                needs_gb,
                host_gb,
            } => write!(
                f,
                "adaptive floor {floor} needs {needs_gb:.1} GB but this host has \
                 {host_gb:.1} GB. Descending past the floor is a decision, not a fallback \
                 — lower the floor explicitly if a smaller base is acceptable for her."
            ),
            Self::FloorNotOnLadder { floor } => write!(
                f,
                "floor {floor} does not appear on this citizen's ladder — the floor is \
                 unenforceable as written, which reads as protection and is not"
            ),
            Self::EmptyLadder => write!(
                f,
                "adaptive policy with no rungs, and there is no system-wide default to \
                 fall back to (that fallback was the #438 downgrade)"
            ),
            Self::NoForgedRungFits {
                fits_but_unforged,
                forged_for,
            } => write!(
                f,
                "base(s) [{}] fit this host but her genome was never forged onto any of \
                 them; she is forged for [{}]. This is a FORGE gap, not a hardware one — \
                 the ladder is declared wider than the corpus has been trained out to. \
                 Fan the forge out to the missing target(s), or set rungs=allowBare to \
                 run her here without her genome (the result is then a different worker \
                 and is reported as genome_backed=false).",
                fits_but_unforged.join(", "),
                if forged_for.is_empty() {
                    "nothing yet".to_string()
                } else {
                    forged_for.join(", ")
                }
            ),
        }
    }
}

impl BaseModelPolicy {
    /// Resolve to a concrete base, or refuse with a reason.
    ///
    /// Pure over `(host_vram_gb, coverage)` so every mode and every refusal is unit-pinned
    /// without a GPU or a forge. Ladder order is authoritative: rungs are tried top-down
    /// and the FIRST that both fits AND is reachable wins, so a caller controls preference
    /// by ordering, never by a tie-break here.
    ///
    /// `coverage` is what makes scaling real rather than nominal: a citizen may move to
    /// any base her genome was forged for, and [`RungPolicy::RequireGenome`] skips the
    /// rest rather than silently handing back a differently-skilled worker.
    pub fn resolve(
        &self,
        host_vram_gb: f64,
        coverage: &GenomeCoverage,
    ) -> Result<ResolvedBase, BaseModelError> {
        match self {
            Self::Pegged {
                model,
                vram_budget_gb,
                reason,
            } => {
                if host_vram_gb >= *vram_budget_gb {
                    Ok(ResolvedBase {
                        model: model.clone(),
                        vram_budget_gb: *vram_budget_gb,
                        genome_backed: coverage.covers(model),
                    })
                } else {
                    // The whole point of the type: a peg that does not fit REFUSES.
                    Err(BaseModelError::PegDoesNotFit {
                        model: model.clone(),
                        reason: reason.clone(),
                        needs_gb: *vram_budget_gb,
                        host_gb: host_vram_gb,
                    })
                }
            }
            Self::Adaptive {
                ladder,
                floor,
                rungs,
            } => {
                if ladder.is_empty() {
                    return Err(BaseModelError::EmptyLadder);
                }
                let floor_idx = ladder
                    .iter()
                    .position(|r| &r.model == floor)
                    .ok_or_else(|| BaseModelError::FloorNotOnLadder {
                        floor: floor.clone(),
                    })?;
                let mut fits_but_unforged: Vec<String> = Vec::new();
                // Walk DOWN to the floor inclusive — never past it.
                for rung in &ladder[..=floor_idx] {
                    if host_vram_gb < rung.min_vram_gb {
                        continue;
                    }
                    let forged = coverage.covers(&rung.model);
                    if !forged && *rungs == RungPolicy::RequireGenome {
                        // She could physically run here, but her corpus was never forged
                        // onto this base — taking it would silently swap in a differently
                        // skilled worker. Remember it so the refusal can name the forge
                        // work that would open the rung.
                        fits_but_unforged.push(rung.model.clone());
                        continue;
                    }
                    return Ok(ResolvedBase {
                        model: rung.model.clone(),
                        vram_budget_gb: rung.vram_budget_gb,
                        genome_backed: forged,
                    });
                }
                if !fits_but_unforged.is_empty() {
                    return Err(BaseModelError::NoForgedRungFits {
                        fits_but_unforged,
                        forged_for: coverage.bases().map(str::to_string).collect(),
                    });
                }
                let floor_rung = &ladder[floor_idx];
                Err(BaseModelError::FloorDoesNotFit {
                    floor: floor.clone(),
                    needs_gb: floor_rung.min_vram_gb,
                    host_gb: host_vram_gb,
                })
            }
        }
    }

    /// The model this policy names when it is a peg — the identity a genome page-in checks
    /// its adapters against (#369), and what a roster row reports as her pinned base.
    pub fn pegged_model(&self) -> Option<&str> {
        match self {
            Self::Pegged { model, .. } => Some(model.as_str()),
            Self::Adaptive { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rung(min: f64, model: &str, budget: f64) -> BaseModelRung {
        BaseModelRung {
            min_vram_gb: min,
            model: model.to_string(),
            vram_budget_gb: budget,
        }
    }

    /// Coverage for a citizen whose corpus has been forged onto EVERY rung — the
    /// "fully fanned-out forge" case, which is what the pre-coverage tests assumed.
    fn all_forged() -> GenomeCoverage {
        GenomeCoverage::from_adapter_bases(["big-27b", "mid-7b", "small-1b", "tiny-0.5b"])
    }

    fn ladder() -> Vec<BaseModelRung> {
        vec![
            rung(40.0, "big-27b", 32.0),
            rung(16.0, "mid-7b", 12.0),
            rung(4.0, "small-1b", 3.0),
            rung(1.0, "tiny-0.5b", 1.0),
        ]
    }

    // what this catches: THE #438 INCIDENT. A host reading low must NOT slide to the
    // bottom of the ladder. With a floor at mid-7b and a host that cannot carry it,
    // resolution REFUSES — it does not quietly serve small-1b or tiny-0.5b. Before this
    // type, `resolve_model_for_persona` fell through to "use last entry (lowest tier)"
    // and then to a system default, which is how citizens got served a 0.5B that emitted
    // template-token garbage into the room.
    #[test]
    fn adaptive_refuses_rather_than_sliding_below_its_floor() {
        let p = BaseModelPolicy::Adaptive {
            ladder: ladder(),
            floor: "mid-7b".into(),
            rungs: RungPolicy::RequireGenome,
        };
        let err = p.resolve(8.0, &all_forged()).unwrap_err();
        assert_eq!(
            err,
            BaseModelError::FloorDoesNotFit {
                floor: "mid-7b".into(),
                needs_gb: 16.0,
                host_gb: 8.0
            }
        );
        let msg = err.to_string();
        assert!(
            msg.contains("Descending past the floor is a decision"),
            "the refusal must name the remedy, not just fail: {msg}"
        );
    }

    // what this catches: the floor is INCLUSIVE — a host that exactly fits the floor gets
    // the floor, not a refusal. An off-by-one here would make the floor unreachable and
    // every floor-sized host non-resident.
    #[test]
    fn adaptive_serves_the_floor_when_the_host_exactly_fits_it() {
        let p = BaseModelPolicy::Adaptive {
            ladder: ladder(),
            floor: "mid-7b".into(),
            rungs: RungPolicy::RequireGenome,
        };
        let got = p.resolve(16.0, &all_forged()).unwrap();
        assert_eq!(got.model, "mid-7b");
        assert_eq!(got.vram_budget_gb, 12.0);
    }

    // what this catches: scaling UP still works — a big host takes the top rung. The floor
    // bounds the DOWN direction only; adding it must not pin everyone to the floor.
    #[test]
    fn adaptive_still_scales_up_to_the_best_rung_that_fits() {
        let p = BaseModelPolicy::Adaptive {
            ladder: ladder(),
            floor: "small-1b".into(),
            rungs: RungPolicy::RequireGenome,
        };
        assert_eq!(p.resolve(64.0, &all_forged()).unwrap().model, "big-27b");
        assert_eq!(p.resolve(20.0, &all_forged()).unwrap().model, "mid-7b");
        assert_eq!(p.resolve(5.0, &all_forged()).unwrap().model, "small-1b");
    }

    // what this catches: a PEG never degrades. This is the property that makes a benchmark
    // number attributable — if the pegged base cannot be served, the honest outcome is
    // that she does not serve, NOT that she serves on something else and we score it as
    // though it were the peg. It also protects the corpus: turns taken on an unintended
    // base become training data for an adapter forged against the pegged one.
    #[test]
    fn a_peg_refuses_instead_of_re_basing() {
        let p = BaseModelPolicy::Pegged {
            model: "big-27b".into(),
            vram_budget_gb: 32.0,
            reason: PegReason::Measurement {
                run_id: "swe-lite-round-1".into(),
            },
        };
        assert_eq!(p.resolve(64.0, &all_forged()).unwrap().model, "big-27b");

        let err = p.resolve(16.0, &all_forged()).unwrap_err();
        match &err {
            BaseModelError::PegDoesNotFit { model, reason, .. } => {
                assert_eq!(model, "big-27b");
                assert!(matches!(reason, PegReason::Measurement { .. }));
            }
            other => panic!("a peg that does not fit must refuse, got {other:?}"),
        }
        let msg = err.to_string();
        assert!(
            msg.contains("corrupt the corpus"),
            "the refusal must say WHY re-basing is worse than absence: {msg}"
        );
    }

    // what this catches: a floor naming a model that isn't on the ladder is a MALFORMED
    // policy, not a no-op. Silently ignoring it would leave an unenforceable floor that
    // reads as protection — the failure shape where a guard exists on paper and the slide
    // happens anyway.
    #[test]
    fn a_floor_absent_from_the_ladder_is_a_loud_policy_error() {
        let p = BaseModelPolicy::Adaptive {
            ladder: ladder(),
            floor: "not-a-rung".into(),
            rungs: RungPolicy::RequireGenome,
        };
        assert_eq!(
            p.resolve(64.0, &all_forged()).unwrap_err(),
            BaseModelError::FloorNotOnLadder {
                floor: "not-a-rung".into()
            }
        );
    }

    // what this catches: there is NO system-wide default arm. An empty ladder refuses.
    // The old resolver's last act was `default_local_model` — a fallback that turned a
    // configuration gap into a silently wrong base.
    #[test]
    fn an_empty_ladder_has_nothing_to_fall_back_to_and_says_so() {
        let p = BaseModelPolicy::Adaptive {
            ladder: vec![],
            floor: "anything".into(),
            rungs: RungPolicy::RequireGenome,
        };
        assert_eq!(p.resolve(64.0, &all_forged()).unwrap_err(), BaseModelError::EmptyLadder);
    }

    // what this catches: THE RULE — she may scale to any base her genome was forged for,
    // and no further. Forged only for mid-7b, sitting on a host that could carry big-27b:
    // she resolves to mid-7b, NOT big-27b. Taking the bigger rung would hand back a
    // citizen without her genome while the caller believed it was scaling her UP.
    #[test]
    fn scaling_up_stops_at_the_highest_base_her_genome_was_forged_for() {
        let p = BaseModelPolicy::Adaptive {
            ladder: ladder(),
            floor: "small-1b".into(),
            rungs: RungPolicy::RequireGenome,
        };
        let only_mid = GenomeCoverage::from_adapter_bases(["mid-7b"]);
        let got = p.resolve(64.0, &only_mid).unwrap();
        assert_eq!(
            got.model, "mid-7b",
            "the host could carry big-27b, but her corpus was never forged onto it"
        );
        assert!(got.genome_backed);
    }

    // what this catches: the same rule going DOWN. Forged only for big-27b, on a host that
    // can only carry mid-7b — she does not quietly drop to a base she has no adapter for.
    // The refusal names the FORGE work, because that (not hardware) is the actual remedy:
    // one corpus, fanned out to the missing target.
    #[test]
    fn scaling_down_to_an_unforged_base_is_a_forge_gap_and_names_the_work() {
        let p = BaseModelPolicy::Adaptive {
            ladder: ladder(),
            floor: "tiny-0.5b".into(),
            rungs: RungPolicy::RequireGenome,
        };
        let only_big = GenomeCoverage::from_adapter_bases(["big-27b"]);
        let err = p.resolve(20.0, &only_big).unwrap_err();
        match &err {
            BaseModelError::NoForgedRungFits {
                fits_but_unforged, ..
            } => {
                assert!(
                    fits_but_unforged.contains(&"mid-7b".to_string()),
                    "must name the rung she could have taken had it been forged: {fits_but_unforged:?}"
                );
            }
            other => panic!("an unforged-but-fitting rung must refuse, got {other:?}"),
        }
        let msg = err.to_string();
        assert!(
            msg.contains("FORGE gap, not a hardware one") && msg.contains("Fan the forge out"),
            "the refusal must point at training, not at buying a bigger box: {msg}"
        );
    }

    // what this catches: AllowBare is a real, permitted mode — a bare frontier base may
    // well beat a small genome-backed one — but the result must be LABELLED. A score from
    // genome_backed=false and one from genome_backed=true are numbers about two different
    // workers; mixing them measures the base swap, not the genome.
    #[test]
    fn allow_bare_takes_the_rung_but_reports_it_as_not_genome_backed() {
        let p = BaseModelPolicy::Adaptive {
            ladder: ladder(),
            floor: "small-1b".into(),
            rungs: RungPolicy::AllowBare,
        };
        let only_mid = GenomeCoverage::from_adapter_bases(["mid-7b"]);
        let got = p.resolve(64.0, &only_mid).unwrap();
        assert_eq!(got.model, "big-27b", "bare scaling up IS allowed when asked for");
        assert!(
            !got.genome_backed,
            "…but the caller must be able to see she is running without her genome"
        );
    }

    // what this catches: a PEG onto a base her genome doesn't cover still resolves (the peg
    // is the operator's/measurement's call) but reports genome_backed=false. Silently
    // claiming genome backing on a base with no adapter would make a lift number that
    // compares a bare run against a forged one and calls the difference "learning".
    #[test]
    fn a_peg_onto_an_unforged_base_serves_but_admits_it_is_bare() {
        let p = BaseModelPolicy::Pegged {
            model: "big-27b".into(),
            vram_budget_gb: 32.0,
            reason: PegReason::Measurement {
                run_id: "baseline-round-0".into(),
            },
        };
        let got = p
            .resolve(64.0, &GenomeCoverage::from_adapter_bases(["mid-7b"]))
            .unwrap();
        assert_eq!(got.model, "big-27b");
        assert!(
            !got.genome_backed,
            "a baseline round on an unforged base is exactly the round you WANT — it just \
             has to be labelled so round 2's lift is attributable"
        );
    }

    // what this catches: a TRAINING peg refuses exactly like a measurement one. Continuous
    // learning runs forever, so this is the peg that is held most of the time — and the
    // failure it prevents is the worst of the three: a forge that starts on one base and
    // finishes after she has drifted to another produces an adapter for nobody, which
    // page-in must then refuse (#369). The corpus survives; the compute does not.
    #[test]
    fn a_training_peg_holds_the_base_for_the_duration_of_the_forge() {
        let p = BaseModelPolicy::Pegged {
            model: "big-27b".into(),
            vram_budget_gb: 32.0,
            reason: PegReason::Training {
                job_id: "forge-coder-act-v3".into(),
            },
        };
        assert_eq!(p.resolve(64.0, &all_forged()).unwrap().model, "big-27b");

        let err = p.resolve(16.0, &all_forged()).unwrap_err();
        match &err {
            BaseModelError::PegDoesNotFit { reason, .. } => {
                assert!(
                    matches!(reason, PegReason::Training { .. }),
                    "the refusal must carry WHY, so an operator can tell a live forge from \
                     an operator pin: {reason:?}"
                );
            }
            other => panic!("a training peg that does not fit must refuse, got {other:?}"),
        }
    }

    // what this catches: `pegged_model` is what a genome page-in checks adapters against
    // (#369) and what the roster reports as her pinned base. An adaptive citizen has no
    // pinned base, and reporting one would be the same lie in a different field.
    #[test]
    fn only_a_pegged_citizen_reports_a_pinned_base() {
        let pegged = BaseModelPolicy::Pegged {
            model: "big-27b".into(),
            vram_budget_gb: 32.0,
            reason: PegReason::GenomeBound,
        };
        assert_eq!(pegged.pegged_model(), Some("big-27b"));

        let adaptive = BaseModelPolicy::Adaptive {
            ladder: ladder(),
            floor: "small-1b".into(),
            rungs: RungPolicy::RequireGenome,
        };
        assert_eq!(adaptive.pegged_model(), None);
    }
}
