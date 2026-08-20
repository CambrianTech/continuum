//! MoE gate-magnitude prior — the STATIC cold-start seed for expert residency (Seam-2 signal).
//!
//! A MoE router (`blk.{L}.ffn_gate_inp`, shape `[n_experts, hidden]`) holds one weight row per
//! expert; the L2 norm of expert e's row is the model's BAKED-IN preference for that expert,
//! readable from the GGUF WITHOUT a forward pass. It's the cold-start prior in the demand-
//! aligned residency priority ([[sentinel-ai-anncontroller-is-the-learnable-expert-residency-controller]],
//! `capacity/expert_residency` → folds into `genome/eviction`): before any live activation
//! hits accumulate, a high-magnitude expert should out-rank a low-magnitude one so a fresh lane
//! isn't paging at random. Once live hits arrive they dominate (measured beats predicted — the
//! PGO principle); this only breaks ties and seeds the start.
//!
//! Sits beside [`super::expert_layout`] (which LOCATES the expert-set byte ranges, zero-copy)
//! and [`super::expert_ingest`] (Seam-1). Unlike those, this READS a small tensor (the router
//! is tiny — `n_experts × hidden`, and llama.cpp keeps it high-precision) and computes norms
//! via candle, dequantizing transparently. Reuses `gguf_keys` + the `candle` GGUF reader the
//! rest of the crate already uses — no parallel GGUF parser.

use std::collections::HashMap;
use std::io::{Read, Seek};

use candle_core::quantized::gguf_file::Content;
use candle_core::{Device, Tensor};

use super::expert_layout::{locate_layer_sets, ExpertLayoutError};

/// The GGUF suffix for the MoE router weight, per layer. Deliberately NOT one of the
/// `EXPS_SUFFIXES` in [`super::expert_layout`] — the router picks experts, it is not one.
const GATE_INP_SUFFIX: &str = "ffn_gate_inp.weight";

/// Why gate magnitudes could not be read.
#[derive(Debug)]
pub enum GateMagnitudeError {
    /// The MoE layer layout could not be resolved. Delegated to [`locate_layer_sets`] so the
    /// priors key the SAME layers the ingest (Seam-1) registered — never an independent
    /// MoE-detection that could disagree. `NotMoe` is handled as "no priors", not surfaced here.
    Layout(ExpertLayoutError),
    /// A candle read/dequantize/shape op failed on a router tensor.
    Tensor(candle_core::Error),
}

impl From<candle_core::Error> for GateMagnitudeError {
    fn from(e: candle_core::Error) -> Self {
        GateMagnitudeError::Tensor(e)
    }
}

/// Per-(layer, expert) gate-magnitude prior for every MoE layer the ingest registers. Keyed by
/// `(layer, expert_index)` to match the residency key `(layer, expert)` — a page's
/// `PageRef{artifact: layer_set, offset: Expert{e}}` maps straight onto `(layer, e)`.
///
/// Drives off the SAME [`locate_layer_sets`] enumeration as [`super::expert_ingest`] (Seam-1),
/// NOT an independent MoE-detection: the priors therefore key EXACTLY the `(layer, expert)`
/// pages the splitter made — no format where a router-presence check and an `*_exps`-presence
/// check could disagree (BigMama's co-review note). For each registered layer it reads the
/// router `ffn_gate_inp`, dequantizes to f32, and takes each expert's row L2 norm, clamped to
/// the layer's registered `n_experts`. A dense model yields no priors (not an error). Pure over
/// the GGUF: no forward pass, no serving.
pub fn locate_gate_magnitudes<R: Read + Seek>(
    ct: &Content,
    reader: &mut R,
    arch: &str,
) -> Result<HashMap<(u32, u32), f32>, GateMagnitudeError> {
    let layer_sets = match locate_layer_sets(ct, arch) {
        Ok(sets) => sets,
        Err(ExpertLayoutError::NotMoe) => return Ok(HashMap::new()), // dense — no priors
        Err(e) => return Err(GateMagnitudeError::Layout(e)),
    };

    let mut out = HashMap::new();
    for set in layer_sets {
        let name = format!("blk.{}.{GATE_INP_SUFFIX}", set.layer);
        // A registered expert layer with no router is malformed but non-fatal: that layer's
        // experts simply fall back to the live-hits signal with no cold-start prior.
        if !ct.tensor_infos.contains_key(&name) {
            continue;
        }
        let router = ct
            .tensor(reader, &name, &Device::Cpu)?
            .dequantize(&Device::Cpu)?; // [n_experts, hidden]
                                        // Clamp to the layer's registered expert count so a prior can never key an expert the
                                        // splitter didn't page.
        for (e, mag) in per_expert_row_norms(&router)?
            .into_iter()
            .take(set.n_experts as usize)
            .enumerate()
        {
            out.insert((set.layer, e as u32), mag);
        }
    }
    Ok(out)
}

/// Pure: the per-row (per-expert) L2 norm of a `[n_experts, hidden]` tensor. `sqrt(Σ x²)` over
/// the hidden dim, one scalar per expert row. The magnitude signal itself — separated so the
/// math is unit-testable without a GGUF.
fn per_expert_row_norms(router: &Tensor) -> candle_core::Result<Vec<f32>> {
    router.sqr()?.sum(1)?.sqrt()?.to_vec1::<f32>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference_capability::gguf_keys;

    // what this catches: the magnitude math — each expert's prior is the L2 norm of its router
    // row. A 3-4-5 / 6-8-10 row gives clean norms (5, 10), so a transposed reduction (summing
    // the wrong axis) or a missing sqrt would fail loudly. This is THE signal; if it's wrong the
    // cold-start prior ranks experts backwards.
    #[test]
    fn per_expert_norm_is_the_l2_of_each_router_row() {
        // 2 experts × 2 hidden: rows [3,4] and [6,8] → norms 5 and 10.
        let t = Tensor::from_vec(vec![3.0f32, 4.0, 6.0, 8.0], (2, 2), &Device::Cpu).unwrap();
        let norms = per_expert_row_norms(&t).unwrap();
        assert_eq!(norms.len(), 2);
        assert!(
            (norms[0] - 5.0).abs() < 1e-5,
            "row [3,4] L2 = 5, got {}",
            norms[0]
        );
        assert!(
            (norms[1] - 10.0).abs() < 1e-5,
            "row [6,8] L2 = 10, got {}",
            norms[1]
        );
    }

    // what this catches: a higher-weight expert gets a strictly higher prior — the ORDERING
    // the cold-start residency relies on. If two experts tie on hits, the one the model prefers
    // (bigger router row) must win, or a fresh lane pins the wrong experts.
    #[test]
    fn bigger_router_row_yields_a_higher_prior() {
        let t = Tensor::from_vec(vec![1.0f32, 0.0, 0.0, 5.0], (2, 2), &Device::Cpu).unwrap();
        let norms = per_expert_row_norms(&t).unwrap();
        assert!(
            norms[1] > norms[0],
            "the 5-weight expert must out-rank the 1-weight one"
        );
    }

    // Real-GGUF validation — reads the on-disk Qwen3-Coder-30B-A3B router tensors and checks the
    // prior is well-formed (one entry per (layer, expert), all finite, not all identical).
    // #[ignore]d so CI stays fast + doesn't need the 18GB file; run locally:
    //   cargo test -p continuum-core --lib gate_magnitude::tests::real_qwen3_coder -- --ignored --nocapture
    #[test]
    #[ignore = "reads the on-disk ~18GB Qwen3-Coder GGUF; run locally with --ignored"]
    fn real_qwen3_coder_router_priors_are_well_formed() {
        let path = format!(
            "{}/.continuum/genome/models/qwen3-coder-30b-a3b-gguf/Qwen3-Coder-30B-A3B-Instruct-Q4_K_M.gguf",
            std::env::var("HOME").unwrap()
        );
        let mut file = std::fs::File::open(&path).expect("open qwen3-coder gguf");
        let ct = Content::read(&mut file).expect("read gguf");
        let arch = gguf_keys::architecture(&ct).expect("arch");
        let n_experts = gguf_keys::expert_count(&ct, &arch).expect("expert_count");
        let mut reader = std::io::BufReader::new(std::fs::File::open(&path).unwrap());

        let mags = locate_gate_magnitudes(&ct, &mut reader, &arch).expect("gate magnitudes");
        assert!(!mags.is_empty(), "MoE model must yield priors");
        assert!(
            mags.values().all(|m| m.is_finite() && *m >= 0.0),
            "all norms finite ≥ 0"
        );
        // Layer 0 has one prior per expert.
        let layer0: Vec<_> = (0..n_experts)
            .filter(|e| mags.contains_key(&(0, *e)))
            .collect();
        assert_eq!(
            layer0.len() as u32,
            n_experts,
            "one prior per expert in layer 0"
        );
        // The router is not degenerate — experts differ in baked-in preference.
        let first = mags[&(0, 0)];
        assert!(
            mags.iter().any(|((_, _), m)| (*m - first).abs() > 1e-6),
            "experts differ"
        );
        eprintln!(
            "qwen3-coder: {} (layer,expert) priors across MoE layers",
            mags.len()
        );
    }
}
