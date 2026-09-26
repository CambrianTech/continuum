//! The numerics a LoRA is FIT AGAINST versus the numerics it is SERVED against.
//!
//! Joel, 2026-09-25: *"Should/does it qlora learn to match the quant level?"* It
//! should, and until this module nothing in the tree could even ask: training records
//! `"quantization": "nf4-double"` in its provenance, serving loads a GGUF whose quant
//! lives only in a filename, and no code joined the two. Two subsystems, no join key.
//!
//! **Why it is a correctness question and not a preference.** A QLoRA adapter is fit
//! on a quantised base, so part of what it learns compensates THAT base's
//! quantisation error. Serve the same adapter over different numerics and some of the
//! correction it learned answers error that is not there, while the error that is
//! there goes uncorrected. The low-rank delta itself transfers — it is stored in
//! bf16 — but its *fit* was conditioned on the base it saw.
//!
//! **What this node does today** (measured 2026-09-25): trains NF4 + double-quant
//! (≈4.13 bpw) and serves `Qwen3.8-27B-Q4_K_M.gguf` (≈4.83 bpw). Those are the same
//! order of base error, which is why nothing has visibly broken. It is luck, not
//! construction: a `Qwen3.8-27B-Q8_0.gguf` (≈8.50 bpw) sits in the same cold-quants
//! directory, and the day a pin or a plan serves it, every NF4-fit adapter meets
//! numerics twice as precise with no receipt and no refusal. The adoption gate would
//! read the result as "training did not help" — the mismatch hides as absent lift,
//! which is the one failure shape this substrate keeps paying for.
//!
//! The comparison is PURE and the bits are the decision, never the names: `nf4-double`
//! and `Q4_K_M` share no substring and belong together; `Q4_K_M` and `Q8_0` share a
//! prefix and do not.

use std::path::Path;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A quantisation, as its producer names it plus the only property the comparison
/// needs: how many bits each stored weight actually costs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/WeightNumerics.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct WeightNumerics {
    /// The name its producer uses — `Q4_K_M` off a GGUF filename, `nf4-double` from
    /// the CUDA trainer's provenance. Carried verbatim so a receipt names the real
    /// artifact rather than our interpretation of it.
    pub label: String,
    /// Effective bits per stored weight, scales and zero-points included. Nominal
    /// figures from llama.cpp's own quantisation table (and bitsandbytes' NF4
    /// arithmetic); they are accurate to ~0.1 bpw, which is far finer than the
    /// distances this type is asked to judge.
    #[ts(type = "number")]
    pub bits_per_weight: f32,
}

impl WeightNumerics {
    /// The quant a GGUF filename declares: `…-Q4_K_M.gguf` → 4.83 bpw. `None` when the
    /// name carries no tag we know — a typed absence, never a default bpw, because a
    /// guessed bit width would silently become a verdict
    /// ([[absence-rendered-as-positive-fact]]).
    pub fn from_gguf_path(path: &Path) -> Option<Self> {
        let stem = path.file_stem()?.to_str()?.to_ascii_uppercase();
        // Longest tag first: Q4_K_M must not be read as Q4_K, nor Q4_0 as Q4.
        let mut tags: Vec<&str> = GGUF_BITS.iter().map(|(t, _)| *t).collect();
        tags.sort_by_key(|t| std::cmp::Reverse(t.len()));
        let tag = tags.into_iter().find(|t| stem.contains(*t))?;
        let bits = GGUF_BITS.iter().find(|(t, _)| *t == tag).map(|(_, b)| *b)?;
        Some(Self { label: tag.to_string(), bits_per_weight: bits })
    }

    /// The quant a trainer declares in its provenance (`quantization` field).
    pub fn from_trainer_label(label: &str) -> Option<Self> {
        let bits = match label.trim().to_ascii_lowercase().as_str() {
            // 4 bits + an fp8 absmax per 64 weights + fp32 scales per 256 blocks:
            // 4 + 8/64 + 32/(64*256) ≈ 4.127. This is what `prepare_model_for_kbit_training`
            // gives with `bnb_4bit_use_double_quant`.
            "nf4-double" => 4.127,
            "nf4" => 4.25,
            "int8" => 8.0,
            "bf16" | "fp16" => 16.0,
            _ => return None,
        };
        Some(Self { label: label.trim().to_string(), bits_per_weight: bits })
    }
}

/// llama.cpp's nominal bits-per-weight per quant tag.
const GGUF_BITS: &[(&str, f32)] = &[
    ("Q2_K", 2.63),
    ("Q3_K_S", 3.44),
    ("Q3_K_M", 3.66),
    ("Q3_K_L", 3.96),
    ("Q4_0", 4.34),
    ("Q4_1", 4.78),
    ("Q4_K_S", 4.58),
    ("Q4_K_M", 4.83),
    ("Q5_0", 5.21),
    ("Q5_1", 5.65),
    ("Q5_K_S", 5.52),
    ("Q5_K_M", 5.67),
    ("Q6_K", 6.56),
    ("Q8_0", 8.50),
    ("BF16", 16.0),
    ("F16", 16.0),
    ("F32", 32.0),
];

/// How far apart two quantisations may sit and still be the same order of base error.
///
/// One bit. NF4 (4.127) against `Q4_K_M` (4.83) is 0.70 apart — the pair this fleet
/// actually runs, and it must pass quietly or the guard is noise. `Q4_K_M` against
/// `Q8_0` is 3.67 apart and must not.
pub const MATCHED_BITS: f32 = 1.0;

/// Beyond this the job is refused before it spends an hour of card: the adapter would
/// be fit to compensate base error of a different order than it will ever meet. Two
/// bits is a whole quantisation tier (Q4 → Q6, Q6 → Q8).
pub const REFUSAL_BITS: f32 = 2.0;

/// What a fit-versus-served comparison concluded.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/NumericsMatch.ts"
)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum NumericsMatch {
    /// Within [`MATCHED_BITS`]: the adapter will meet base error of the order it was
    /// fit against.
    Matched {
        #[ts(type = "number")]
        bits_apart: f32,
    },
    /// Past [`MATCHED_BITS`] but under [`REFUSAL_BITS`]: worth recording on the
    /// receipt, not worth refusing the run over.
    Drifted {
        #[ts(type = "number")]
        bits_apart: f32,
    },
    /// Past [`REFUSAL_BITS`]: fit against numerics it will not be served against.
    Incompatible {
        #[ts(type = "number")]
        bits_apart: f32,
    },
    /// One side could not be read. NOT a pass: it says which side is unknown, so a
    /// receipt cannot claim a match nobody measured.
    Unmeasured { missing: String },
}

/// Compare the numerics a LoRA is fit against with the numerics it will be served
/// against. Pure — the whole decision is a subtraction, so the fleet's real pairs are
/// a test rather than a story.
pub fn compare(fit: Option<&WeightNumerics>, served: Option<&WeightNumerics>) -> NumericsMatch {
    let (fit, served) = match (fit, served) {
        (Some(f), Some(s)) => (f, s),
        (None, Some(_)) => return NumericsMatch::Unmeasured { missing: "fit".into() },
        (Some(_), None) => return NumericsMatch::Unmeasured { missing: "served".into() },
        (None, None) => return NumericsMatch::Unmeasured { missing: "fit+served".into() },
    };
    let bits_apart = (fit.bits_per_weight - served.bits_per_weight).abs();
    if bits_apart >= REFUSAL_BITS {
        NumericsMatch::Incompatible { bits_apart }
    } else if bits_apart > MATCHED_BITS {
        NumericsMatch::Drifted { bits_apart }
    } else {
        NumericsMatch::Matched { bits_apart }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the pair this fleet actually runs must pass QUIETLY, and the
    // pair sitting one directory away must not. NF4-double (4.127) vs Q4_K_M (4.83)
    // is the 5090's real configuration, 0.70 bpw apart; the Q8_0 cold-quant in the
    // same snapshots dir is 4.37 apart and is the case that would have shipped an
    // adapter fit to error twice as coarse as it meets, reported as "no lift".
    #[test]
    fn the_fleets_real_pair_matches_and_the_cold_quant_beside_it_does_not() {
        let fit = WeightNumerics::from_trainer_label("nf4-double").expect("trainer label");
        let served = WeightNumerics::from_gguf_path(Path::new(
            r"D:\continuum-cold\huggingface\hub\models--ggml-org--Qwen3.8-27B-GGUF\snapshots\0669b986\Qwen3.8-27B-Q4_K_M.gguf",
        ))
        .expect("Q4_K_M");
        assert_eq!(served.label, "Q4_K_M");
        match compare(Some(&fit), Some(&served)) {
            NumericsMatch::Matched { bits_apart } => {
                assert!((bits_apart - 0.703).abs() < 0.01, "{bits_apart}")
            }
            other => panic!("the running pair must pass quietly, got {other:?}"),
        }

        let cold = WeightNumerics::from_gguf_path(Path::new("cold-quants/Qwen3.8-27B-Q8_0.gguf"))
            .expect("Q8_0");
        assert_eq!(cold.label, "Q8_0");
        assert!(
            matches!(compare(Some(&fit), Some(&cold)), NumericsMatch::Incompatible { .. }),
            "NF4-fit served at Q8_0 is a tier apart and must be refused, not reported as no-lift"
        );
    }

    // what this catches: the tag parse erring in either direction — a longer tag must
    // win over its own prefix (Q4_K_M is not Q4_K, Q4_0 is not Q4), a projector or
    // draft file names its own quant, and an unknown name is ABSENT rather than a
    // guessed bit width that would silently become a verdict.
    #[test]
    fn the_longest_tag_wins_and_an_unknown_name_is_absent() {
        let q = |p: &str| WeightNumerics::from_gguf_path(Path::new(p)).map(|n| n.label);
        assert_eq!(q("Qwen3.8-27B-Q4_K_M.gguf").as_deref(), Some("Q4_K_M"));
        assert_eq!(q("Qwen3.8-27B-Q4_K_S.gguf").as_deref(), Some("Q4_K_S"));
        assert_eq!(q("mtp-Qwen3.8-27B-Q4_0.gguf").as_deref(), Some("Q4_0"), "the draft model");
        assert_eq!(q("mmproj-Qwen3.8-27B-BF16.gguf").as_deref(), Some("BF16"), "the projector");
        assert_eq!(q("some-model-imatrix.gguf"), None, "no tag we know = absent, never a default");
        assert_eq!(WeightNumerics::from_trainer_label("mystery-4bit"), None);
    }

    // what this catches: an unreadable side can never be laundered into a pass. A
    // receipt that says Matched when nothing was measured is the success-signal-that-
    // cannot-report-failure shape, and it names WHICH side is missing so the next
    // reader knows where to look.
    #[test]
    fn an_unmeasured_side_is_never_a_match() {
        let known = WeightNumerics::from_trainer_label("nf4-double").expect("label");
        assert_eq!(
            compare(Some(&known), None),
            NumericsMatch::Unmeasured { missing: "served".into() }
        );
        assert_eq!(
            compare(None, Some(&known)),
            NumericsMatch::Unmeasured { missing: "fit".into() }
        );
        assert_eq!(
            compare(None, None),
            NumericsMatch::Unmeasured { missing: "fit+served".into() }
        );
    }

    // what this catches: the thresholds keep their stated meaning — one bit is the
    // same order, two bits is a tier — so a later edit cannot quietly widen the guard
    // until it passes everything.
    #[test]
    fn the_thresholds_mean_what_the_docs_say() {
        let at = |bits: f32| WeightNumerics { label: format!("{bits}"), bits_per_weight: bits };
        let base = at(4.0);
        assert!(matches!(compare(Some(&base), Some(&at(5.0))), NumericsMatch::Matched { .. }));
        assert!(matches!(compare(Some(&base), Some(&at(5.5))), NumericsMatch::Drifted { .. }));
        assert!(matches!(compare(Some(&base), Some(&at(6.0))), NumericsMatch::Incompatible { .. }));
        assert!(MATCHED_BITS < REFUSAL_BITS, "a drifted band must exist between them");
    }
}
