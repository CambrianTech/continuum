//! Expected-vs-actual decode throughput — "are we hitting the spec?".
//!
//! Joel (2026-06-15): "Low latency high token throughput and we need to
//! compare it to expected, from internet specs, speed. … GPU or bust."
//!
//! The substrate already MEASURES decode tok/s (the llama.cpp scheduler
//! splits every token into GPU-dispatch / GPU-sync / CPU-post-sample
//! phases). What was missing is a baseline to compare against, so a number
//! like "22 tok/s" can be classified as *catastrophically degraded* instead
//! of sitting unjudged in a log line. This module is that classifier: pure,
//! data-driven, no I/O — a `(model, quant, accelerator)` → published/measured
//! expected tok/s, and a verdict comparing the measured value to it.
//!
//! The verdict is what makes a CPU-fallback regression LOUD: a 4B-Q4 model
//! that should do ~180 tok/s on an RTX 5090 but measures 8 tok/s is
//! `Degraded { ratio: 0.05 }` — an unmissable signal something fell off the
//! GPU. Baselines are seed data, refined as we measure real hardware; the
//! classifier mechanism is the load-bearing part.

/// A published / measured single-stream decode-throughput baseline for a
/// `(model, quant, accelerator)` tuple. `expected_tok_s` is steady-state
/// decode (post-warmup, single sequence). `source` records provenance so a
/// stale or guessed number is auditable — never present an unsourced
/// baseline as fact.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThroughputBaseline {
    pub model: &'static str,
    pub quant: &'static str,
    pub accelerator: &'static str,
    pub expected_tok_s: f64,
    pub source: &'static str,
}

/// How measured decode throughput compares to the expected baseline. `ratio`
/// is `measured / expected` in every arm so callers can render a percentage.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ThroughputVerdict {
    /// At or above expected (ratio ≥ 1.0 − a small over-delivery is still
    /// "on par"; only meaningfully-above trips this).
    AbovePar {
        measured_tok_s: f64,
        expected_tok_s: f64,
        ratio: f64,
    },
    /// Within tolerance of expected — healthy.
    OnPar {
        measured_tok_s: f64,
        expected_tok_s: f64,
        ratio: f64,
    },
    /// Below tolerance — investigate (CPU fallback, thermal throttle, a
    /// scheduler stall, the wrong model loaded, …). This is the signal that
    /// must never sit silent in a log.
    Degraded {
        measured_tok_s: f64,
        expected_tok_s: f64,
        ratio: f64,
    },
}

impl ThroughputVerdict {
    pub fn ratio(&self) -> f64 {
        match self {
            ThroughputVerdict::AbovePar { ratio, .. }
            | ThroughputVerdict::OnPar { ratio, .. }
            | ThroughputVerdict::Degraded { ratio, .. } => *ratio,
        }
    }

    pub fn is_degraded(&self) -> bool {
        matches!(self, ThroughputVerdict::Degraded { .. })
    }
}

/// Classify `measured_tok_s` against `expected_tok_s`.
///
/// `on_par_floor` is the `measured/expected` ratio below which we call it
/// `Degraded` (e.g. `0.80` → more than 20% below expected is degraded).
/// `above_par_ceiling` is the ratio at/above which we call it `AbovePar`
/// (e.g. `1.10` → 10%+ over expected). Between the two it's `OnPar`.
///
/// Pure: no I/O, no clock. `expected_tok_s <= 0` is treated as "no usable
/// baseline" → `OnPar` with ratio 0 is wrong, so callers MUST pass a
/// positive expected (look it up via [`baseline_for`] first); we guard by
/// returning `Degraded` only on a positive expected. A non-finite or
/// non-positive `measured` is `Degraded` (something produced no tokens).
pub fn classify_throughput(
    measured_tok_s: f64,
    expected_tok_s: f64,
    on_par_floor: f64,
    above_par_ceiling: f64,
) -> ThroughputVerdict {
    // Guard: without a positive baseline we cannot judge — treat as OnPar
    // ratio 0 would mislead, so clamp expected to a tiny positive and let
    // the ratio reflect reality. A non-positive measured is unambiguously
    // degraded (no real decode happened).
    let expected = if expected_tok_s.is_finite() && expected_tok_s > 0.0 {
        expected_tok_s
    } else {
        // No usable baseline: report OnPar with ratio 1.0 is dishonest;
        // surface it as Degraded ratio 0 so the caller notices the missing
        // baseline rather than silently passing.
        return ThroughputVerdict::Degraded {
            measured_tok_s,
            expected_tok_s,
            ratio: 0.0,
        };
    };
    let measured = if measured_tok_s.is_finite() && measured_tok_s > 0.0 {
        measured_tok_s
    } else {
        return ThroughputVerdict::Degraded {
            measured_tok_s,
            expected_tok_s: expected,
            ratio: 0.0,
        };
    };

    let ratio = measured / expected;
    if ratio >= above_par_ceiling {
        ThroughputVerdict::AbovePar {
            measured_tok_s: measured,
            expected_tok_s: expected,
            ratio,
        }
    } else if ratio >= on_par_floor {
        ThroughputVerdict::OnPar {
            measured_tok_s: measured,
            expected_tok_s: expected,
            ratio,
        }
    } else {
        ThroughputVerdict::Degraded {
            measured_tok_s: measured,
            expected_tok_s: expected,
            ratio,
        }
    }
}

/// Seed baseline table — single-stream decode tok/s. Each entry records its
/// `source`. MEASURED entries come from continuum's own benches; ESTIMATE
/// entries are bandwidth-bound back-of-envelope figures to be REPLACED with
/// measured numbers (the whole point of wiring this into the bench). An
/// estimate that's off by 2× still catches a CPU-fallback regression (which
/// is off by 10–30×), which is the load-bearing job until measurement lands.
pub const SEED_BASELINES: &[ThroughputBaseline] = &[
    ThroughputBaseline {
        model: "qwen3.5-4b",
        quant: "Q4_K_M",
        accelerator: "apple-m5",
        expected_tok_s: 67.8,
        source: "MEASURED: continuum tests/llamacpp_metal_throughput.rs single-seq, 2026-06",
    },
    ThroughputBaseline {
        model: "qwen3-8b",
        quant: "Q4_K_M",
        accelerator: "rtx-5090",
        // Real hardware number: qwen3-8B resident entirely in 5090 VRAM,
        // DMR llama.cpp-cuda backend (WSL2 GPU-PV), single-stream decode.
        // prompt-eval was ~960 tok/s. This anchors the 4B estimate below.
        expected_tok_s: 221.7,
        source: "MEASURED: DMR llama.cpp-cuda slot timing, RTX 5090 32GB, 2026-06-15",
    },
    ThroughputBaseline {
        model: "qwen3.5-4b",
        quant: "Q4_K_M",
        accelerator: "rtx-5090",
        // Still an estimate (no 4B run yet) but now SANITY-CHECKED: the 8B
        // above measured 221.7 tok/s on this exact GPU, and a 4B is ~half
        // the weights, so it should comfortably EXCEED the 8B — 180 is a
        // conservative floor, not a ceiling. Sub-30 tok/s on a 4B here = a
        // fallen-off-GPU regression worth screaming about.
        expected_tok_s: 180.0,
        source: "ESTIMATE: conservative floor (8B measured 221.7 same GPU); REFINE with a 4B run",
    },
];

/// Look up a seed baseline for `(model, quant, accelerator)`. Case-sensitive
/// exact match on all three. `None` when we have no published number — a
/// caller MUST NOT claim "degraded" without a baseline (that's why
/// [`classify_throughput`] takes an explicit expected rather than guessing).
pub fn baseline_for(
    model: &str,
    quant: &str,
    accelerator: &str,
) -> Option<&'static ThroughputBaseline> {
    SEED_BASELINES
        .iter()
        .find(|b| b.model == model && b.quant == quant && b.accelerator == accelerator)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Conventional tolerances used by the tests: >20% below = degraded,
    // ≥10% above = above-par.
    const FLOOR: f64 = 0.80;
    const CEIL: f64 = 1.10;

    /// what this catches: a measured value comfortably inside the band is
    /// OnPar — the healthy case must not trip a false Degraded alarm.
    #[test]
    fn within_tolerance_is_on_par() {
        let v = classify_throughput(95.0, 100.0, FLOOR, CEIL);
        assert!(matches!(v, ThroughputVerdict::OnPar { .. }), "{v:?}");
        assert!(!v.is_degraded());
    }

    /// what this catches: the load-bearing case — a value far below expected
    /// (the CPU-fallback / fell-off-GPU regression) is Degraded, not silently
    /// passed. 8 tok/s vs an expected 180 (5090) is ratio ~0.044.
    #[test]
    fn far_below_expected_is_degraded() {
        let v = classify_throughput(8.0, 180.0, FLOOR, CEIL);
        assert!(v.is_degraded(), "{v:?}");
        assert!(v.ratio() < 0.05);
    }

    /// what this catches: the boundary — exactly at the floor is NOT degraded
    /// (>= floor is OnPar); a hair below IS. Off-by-one on the comparison
    /// would mis-call borderline hardware.
    #[test]
    fn floor_boundary_is_inclusive_on_par() {
        assert!(matches!(
            classify_throughput(80.0, 100.0, FLOOR, CEIL),
            ThroughputVerdict::OnPar { .. }
        ));
        assert!(classify_throughput(79.0, 100.0, FLOOR, CEIL).is_degraded());
    }

    /// what this catches: meaningfully exceeding expected is AbovePar (e.g.
    /// after a fork optimization), so a win is visible, not flattened to OnPar.
    #[test]
    fn above_ceiling_is_above_par() {
        let v = classify_throughput(150.0, 100.0, FLOOR, CEIL);
        assert!(matches!(v, ThroughputVerdict::AbovePar { .. }), "{v:?}");
        assert!(v.ratio() >= 1.10);
    }

    /// what this catches: a non-positive / non-finite measured (no tokens
    /// produced, NaN timing) is Degraded ratio 0 — never silently OnPar.
    #[test]
    fn zero_or_nonfinite_measured_is_degraded() {
        assert!(classify_throughput(0.0, 100.0, FLOOR, CEIL).is_degraded());
        assert!(classify_throughput(f64::NAN, 100.0, FLOOR, CEIL).is_degraded());
    }

    /// what this catches: a missing/garbage baseline must NOT pass as OnPar —
    /// you can't claim "on spec" without a spec. Non-positive expected →
    /// Degraded ratio 0 so the caller fixes the missing baseline.
    #[test]
    fn nonpositive_expected_is_degraded_not_silently_ok() {
        assert!(classify_throughput(100.0, 0.0, FLOOR, CEIL).is_degraded());
        assert!(classify_throughput(100.0, -5.0, FLOOR, CEIL).is_degraded());
    }

    /// what this catches: the real-world contract this module exists for —
    /// the MEASURED 5090 number (qwen3-8B, 221.7 t/s, 2026-06-15) is healthy
    /// against its own baseline, while a CPU-fallback number (~12 t/s, the
    /// shitty-CPU regression) on the same expectation trips Degraded. GPU =
    /// OnPar, fallen-off-GPU = caught loud.
    #[test]
    fn measured_5090_is_healthy_and_cpu_fallback_is_caught() {
        let b = baseline_for("qwen3-8b", "Q4_K_M", "rtx-5090").expect("measured 5090 baseline");
        assert!(b.source.contains("MEASURED"));
        assert!((b.expected_tok_s - 221.7).abs() < 0.01);
        assert!(!classify_throughput(221.7, b.expected_tok_s, FLOOR, CEIL).is_degraded());
        assert!(classify_throughput(12.0, b.expected_tok_s, FLOOR, CEIL).is_degraded());
    }

    /// what this catches: the seed table is wired correctly — the measured M5
    /// number is present + sourced, and an unknown tuple returns None (so the
    /// caller can't fabricate a degraded verdict without a real baseline).
    #[test]
    fn baseline_lookup_hits_known_and_misses_unknown() {
        let m5 = baseline_for("qwen3.5-4b", "Q4_K_M", "apple-m5").expect("seeded");
        assert!(m5.expected_tok_s > 0.0);
        assert!(m5.source.contains("MEASURED"));
        assert!(baseline_for("qwen3.5-4b", "Q4_K_M", "rtx-5090").is_some());
        assert!(baseline_for("nonexistent", "Q4_K_M", "apple-m5").is_none());
    }
}
