//! MoE expert layout — locate each layer's stacked expert-set in a GGUF so the
//! Seam-1 splitter can register it as a file-mapped
//! [`ArtifactSource::Mapped`](super::blob::ArtifactSource) artifact on the frozen
//! tier without copying bytes.
//!
//! ## One artifact per layer, `PageOffset::Expert{e}` selects the expert
//!
//! GGUF stores MoE experts **stacked**: each layer has one `blk.{L}.ffn_gate_exps`
//! / `ffn_up_exps` / `ffn_down_exps` tensor of shape `[n_experts, …]` (some
//! exporters fuse gate+up into `ffn_gate_up_exps`). Per the pre-existing
//! `PageKind::MoEExpert` model, ONE artifact is a whole layer's expert set, and
//! `PageOffset::Expert{e}` picks an expert out of it. So this module returns, per
//! MoE layer, the stacked projection tensors' full `(base, total_len)` — enough
//! to build `ArtifactBlob::mapped(id, path, n_experts, projections)`. The
//! per-expert stride-slice (`base + e*(total/n_experts)`) is resolved by
//! [`ArtifactSource::expert_ranges`], which is the single place that math lives.
//!
//! The router weight (`ffn_gate_inp`) is deliberately NOT part of the set — it is
//! one small tensor consulted for EVERY token's routing and stays resident.

use candle_core::quantized::gguf_file::Content;

use crate::inference_capability::gguf_keys;

/// The stacked expert-projection tensor suffixes, in canonical order. A layer
/// carries EITHER the split trio (`gate`/`up`/`down`) OR the fused pair
/// (`gate_up`/`down`); we collect whichever are present, so both export styles
/// work without the caller knowing which it has.
const EXPS_SUFFIXES: &[&str] = &[
    "ffn_gate_exps",
    "ffn_up_exps",
    "ffn_gate_up_exps",
    "ffn_down_exps",
];

/// One MoE layer's expert-set layout: its expert count and the stacked
/// projection tensors' `(absolute_base_offset, total_len)`. Maps directly onto
/// `ArtifactBlob::mapped(id, gguf_path, n_experts, projections)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayerExpertSet {
    pub layer: u32,
    pub n_experts: u32,
    /// Each present stacked projection tensor's `(base, total_len)` for the whole
    /// layer, in `EXPS_SUFFIXES` order. 2–3 entries.
    pub projections: Vec<(u64, u64)>,
}

impl LayerExpertSet {
    /// Total bytes of the whole layer set — the sum of projection lengths. Equals
    /// the resulting `ArtifactSource::size_bytes` for this artifact.
    pub fn total_bytes(&self) -> u64 {
        self.projections.iter().map(|(_, total)| *total).sum()
    }
}

/// Why expert layout could not be resolved from a GGUF.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExpertLayoutError {
    /// No `{arch}.expert_count` key — the model is dense, nothing to page.
    NotMoe,
    /// No `{arch}.block_count` — a broken/incomplete export.
    MissingBlockCount,
    /// `expert_count` was zero — a malformed MoE header.
    ZeroExperts,
    /// A stacked expert tensor's byte size is not divisible by `n_experts`, so
    /// the per-expert stride is undefined — the layout is not what we assume.
    UnevenStack {
        tensor: String,
        tensor_bytes: u64,
        n_experts: u32,
    },
}

/// Enumerate every MoE layer's expert set in a GGUF. Returns one
/// [`LayerExpertSet`] per layer that has stacked `*_exps` tensors, skipping dense
/// layers. `Err(NotMoe)` for a dense model.
pub fn locate_layer_sets(
    ct: &Content,
    arch: &str,
) -> Result<Vec<LayerExpertSet>, ExpertLayoutError> {
    let n_experts = gguf_keys::expert_count(ct, arch).ok_or(ExpertLayoutError::NotMoe)?;
    if n_experts == 0 {
        return Err(ExpertLayoutError::ZeroExperts);
    }
    let n_layers = gguf_keys::block_count(ct, arch).ok_or(ExpertLayoutError::MissingBlockCount)?;

    let mut out = Vec::new();
    for layer in 0..n_layers {
        let mut projections: Vec<(u64, u64)> = Vec::new();
        for suffix in EXPS_SUFFIXES {
            let name = format!("blk.{layer}.{suffix}.weight");
            let Some(info) = ct.tensor_infos.get(&name) else {
                continue;
            };
            let elems = info.shape.elem_count();
            let block_size = info.ggml_dtype.block_size();
            let type_size = info.ggml_dtype.type_size();
            let tensor_bytes = (elems / block_size * type_size) as u64;
            // Stacking is along dim 0 (experts); the whole tensor must divide
            // evenly by n_experts or the per-expert stride is undefined.
            if tensor_bytes % n_experts as u64 != 0 {
                return Err(ExpertLayoutError::UnevenStack {
                    tensor: name,
                    tensor_bytes,
                    n_experts,
                });
            }
            let base = ct.tensor_data_offset + info.offset;
            projections.push((base, tensor_bytes));
        }
        if projections.is_empty() {
            continue; // dense layer — no experts to page
        }
        out.push(LayerExpertSet {
            layer,
            n_experts,
            projections,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::blob::ArtifactSource;
    use candle_core::quantized::gguf_file::{Content, TensorInfo, Value, VersionedMagic};
    use candle_core::quantized::GgmlDType;
    use candle_core::Shape;
    use std::path::PathBuf;

    // F32 keeps the byte math trivial (block_size 1, type_size 4).
    fn f32_info(shape: &[usize], offset: u64) -> TensorInfo {
        TensorInfo {
            ggml_dtype: GgmlDType::F32,
            shape: Shape::from(shape.to_vec()),
            offset,
        }
    }

    fn content(md: Vec<(&str, Value)>, tensors: Vec<(&str, TensorInfo)>, data_off: u64) -> Content {
        Content {
            magic: VersionedMagic::GgufV3,
            metadata: md.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
            tensor_infos: tensors
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
            tensor_data_offset: data_off,
        }
    }

    // what this catches: per-layer projection extraction — the (base, total_len)
    // of each stacked *_exps tensor, dense layers skipped, and the crucial
    // cross-check that feeding the result into ArtifactSource::Mapped and calling
    // expert_ranges reproduces the exact stride-slices. This ties the locator
    // (write side) to the resolver (read side) so they can't drift apart.
    #[test]
    fn locates_layer_sets_and_resolver_agrees() {
        // n_experts=4, 2 layers. Layer 0 dense; layer 1 MoE (gate/up/down).
        // tensor_data_offset=1000. Each [4,2,8] tensor = 64 elems = 256 bytes.
        let md = vec![
            ("qwen3moe.expert_count", Value::U32(4)),
            ("qwen3moe.block_count", Value::U32(2)),
        ];
        let tensors = vec![
            ("blk.0.attn_q.weight", f32_info(&[2, 8], 0)),
            ("blk.1.ffn_gate_exps.weight", f32_info(&[4, 2, 8], 0)),
            ("blk.1.ffn_up_exps.weight", f32_info(&[4, 2, 8], 256)),
            ("blk.1.ffn_down_exps.weight", f32_info(&[4, 2, 8], 512)),
        ];
        let ct = content(md, tensors, 1000);

        let sets = locate_layer_sets(&ct, "qwen3moe").unwrap();
        assert_eq!(sets.len(), 1, "only layer 1 is MoE");
        let s = &sets[0];
        assert_eq!(s.layer, 1);
        assert_eq!(s.n_experts, 4);
        // projections = (base = data_offset + tensor.offset, total_len = 256).
        assert_eq!(s.projections, vec![(1000, 256), (1256, 256), (1512, 256)]);
        assert_eq!(s.total_bytes(), 768);

        // Cross-check: build the artifact source and resolve expert 3 — it must
        // match base + 3*(256/4)=+192, stride 64 in each projection.
        let src = ArtifactSource::Mapped {
            path: PathBuf::from("/frozen/k3.gguf"),
            n_experts: s.n_experts,
            projections: s.projections.clone(),
        };
        assert_eq!(
            src.expert_ranges(3),
            Some(vec![(1000 + 192, 64), (1256 + 192, 64), (1512 + 192, 64)])
        );
        assert_eq!(src.expert_size_bytes(0), Some(64 * 3));
    }

    // what this catches: a dense model (no expert_count) is NotMoe — an explicit
    // "nothing to page" signal, not a panic or empty-Ok.
    #[test]
    fn dense_model_is_not_moe() {
        let ct = content(vec![("llama.block_count", Value::U32(4))], vec![], 100);
        assert_eq!(
            locate_layer_sets(&ct, "llama"),
            Err(ExpertLayoutError::NotMoe)
        );
    }

    // what this catches: the fused gate_up export (gate+up in one tensor) still
    // resolves — a layer set with 2 projections (fused + down), proving
    // EXPS_SUFFIXES handles both layouts.
    #[test]
    fn fused_gate_up_export_resolves_two_projections() {
        let md = vec![
            ("glm4moe.expert_count", Value::U32(2)),
            ("glm4moe.block_count", Value::U32(1)),
        ];
        let tensors = vec![
            ("blk.0.ffn_gate_up_exps.weight", f32_info(&[2, 4, 8], 0)),
            ("blk.0.ffn_down_exps.weight", f32_info(&[2, 2, 8], 512)),
        ];
        let ct = content(md, tensors, 0);
        let sets = locate_layer_sets(&ct, "glm4moe").unwrap();
        assert_eq!(sets.len(), 1);
        // gate_up: [2,4,8]=64 elems=256B; down: [2,2,8]=32 elems=128B.
        assert_eq!(sets[0].projections, vec![(0, 256), (512, 128)]);
    }
}
