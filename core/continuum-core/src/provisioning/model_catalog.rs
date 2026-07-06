//! Owning the model catalog — since unsloth is gone, WE decide which GGUF a repo offers
//! and which quantization fits THIS machine. This module is the pure selection heart:
//! given the files a repo publishes + a VRAM budget, pick the highest-fidelity quant
//! that fits, or fail loud that the machine can't host the model. The HF API query that
//! lists a repo's files feeds these candidates (next slice); keeping the *policy* pure
//! makes the hardware-fit decision testable without a network.
//!
//! This is the convergence point with the lane governor: "which quant to fetch" IS
//! "which base model to serve" ([[model-fit-is-the-priority-single-machine-first]],
//! [[governor-settles-on-measured-throughput]]) — one decision, made from measured VRAM,
//! never a hardcoded `Q4_K_M`.

/// One GGUF file a repo publishes — the filename (which encodes the quant) and the size
/// that determines whether it fits in VRAM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GgufCandidate {
    pub filename: String,
    pub size_bytes: u64,
}

impl GgufCandidate {
    pub fn new(filename: impl Into<String>, size_bytes: u64) -> Self {
        Self { filename: filename.into(), size_bytes }
    }

    /// The quant label parsed from the filename, e.g. "Q4_K_M" (None if unparseable).
    pub fn quant(&self) -> Option<String> {
        parse_quant(&self.filename)
    }

    /// Auxiliary GGUFs that are NOT the main weights: the vision projector (`mmproj-…`)
    /// and multi-part splits (`…-00002-of-00003.gguf`) — the main-weight selection must
    /// skip these, they're fetched alongside the chosen quant, not instead of it.
    pub fn is_auxiliary(&self) -> bool {
        let lower = self.filename.to_lowercase();
        lower.contains("mmproj") || lower.contains("-of-")
    }
}

/// Parse the quant label out of a GGUF filename: the last hyphen/dot-delimited token
/// that looks like a quant tag (`Q4_K_M`, `IQ3_XXS`, `F16`, `BF16`, `Q8_0`). Returns the
/// tag verbatim (upper-cased), or None if the name carries no recognizable quant.
pub fn parse_quant(filename: &str) -> Option<String> {
    let stem = filename.strip_suffix(".gguf").unwrap_or(filename);
    // Tokens are separated by '-' or '.'; scan right-to-left so `…-Instruct-Q4_K_M`
    // returns the quant, not an earlier token.
    for token in stem.rsplit(['-', '.']) {
        if looks_like_quant(token) {
            return Some(token.to_uppercase());
        }
    }
    None
}

fn looks_like_quant(token: &str) -> bool {
    let up = token.to_uppercase();
    // Float weights.
    if matches!(up.as_str(), "F16" | "F32" | "BF16") {
        return true;
    }
    // Integer quants: Q… or IQ… followed by a digit (Q4_K_M, IQ3_XXS, Q8_0).
    let rest = up.strip_prefix("IQ").or_else(|| up.strip_prefix('Q'));
    matches!(rest, Some(r) if r.chars().next().is_some_and(|c| c.is_ascii_digit()))
}

/// Pick the highest-fidelity main-weight GGUF that FITS `vram_budget_bytes`: among the
/// non-auxiliary candidates whose size ≤ budget, the LARGEST (bigger quant ≈ bigger file
/// ≈ higher fidelity). Returns None when NONE fit — a hard truth about this machine, to
/// be surfaced (fail loud), never silently downgraded past what exists or oversized past
/// what fits.
pub fn select_best_fit(candidates: &[GgufCandidate], vram_budget_bytes: u64) -> Option<&GgufCandidate> {
    candidates
        .iter()
        .filter(|c| !c.is_auxiliary() && c.size_bytes <= vram_budget_bytes)
        // Largest that fits; deterministic tie-break by name so the choice is stable.
        .max_by(|a, b| {
            a.size_bytes
                .cmp(&b.size_bytes)
                .then_with(|| b.filename.cmp(&a.filename))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: quant parsing pulls the tag from real GGUF names (right-most
    // token wins over model-name noise) and rejects names with no quant.
    #[test]
    fn parse_quant_reads_the_tag() {
        assert_eq!(
            parse_quant("Qwen2.5-Coder-14B-Instruct-Q4_K_M.gguf").as_deref(),
            Some("Q4_K_M")
        );
        assert_eq!(parse_quant("model-IQ3_XXS.gguf").as_deref(), Some("IQ3_XXS"));
        assert_eq!(parse_quant("weights.F16.gguf").as_deref(), Some("F16"));
        assert_eq!(parse_quant("some-random-model.gguf"), None);
    }

    // what this catches: THE model-fit decision — pick the largest quant that fits VRAM,
    // return None (fail loud) when the smallest still overflows, and never pick an
    // auxiliary (mmproj / split part) as the main weights.
    #[test]
    fn select_best_fit_is_largest_that_fits_or_none() {
        let files = vec![
            GgufCandidate::new("m-Q3_K_M.gguf", 6_000),
            GgufCandidate::new("m-Q4_K_M.gguf", 8_000),
            GgufCandidate::new("m-Q8_0.gguf", 15_000),
            GgufCandidate::new("mmproj-f16.gguf", 1_000), // auxiliary — never the main pick
        ];
        // 10k budget → Q4 (8k) is the largest main weight that fits.
        assert_eq!(select_best_fit(&files, 10_000).unwrap().filename, "m-Q4_K_M.gguf");
        // 20k budget → Q8 (the biggest).
        assert_eq!(select_best_fit(&files, 20_000).unwrap().filename, "m-Q8_0.gguf");
        // 5k budget → nothing fits (Q3 is 6k). Fail loud, don't grab the 1k mmproj.
        assert!(select_best_fit(&files, 5_000).is_none());
    }
}
