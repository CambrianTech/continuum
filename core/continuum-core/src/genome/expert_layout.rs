//! MoE expert layout — locate each `(layer, expert)`'s physical byte ranges
//! in a GGUF so the Seam-1 splitter can register them as file-mapped
//! [`ArtifactSource::Mapped`](super::blob::ArtifactSource) artifacts on the
//! frozen tier without ever copying the bytes.
//!
//! ## Why ranges, not tensors
//!
//! GGUF stores MoE experts **stacked**: each layer has one `blk.{L}.ffn_gate_exps`
//! / `ffn_up_exps` / `ffn_down_exps` tensor of shape `[n_experts, …]` (some
//! exporters fuse gate+up into `ffn_gate_up_exps`). So "expert `e`" is not a named
//! tensor — it is the `[e]` **slice** of each stacked projection tensor, and those
//! projections live at *different* file offsets. One expert therefore spans N
//! disjoint `(offset, len)` ranges (one per projection). The stack is along dim 0,
//! so each expert's slice is an equal contiguous chunk: `stride = tensor_bytes /
//! n_experts`, and expert `e`'s slice is `base + e * stride` for `stride` bytes.
//!
//! The router weight (`ffn_gate_inp`) is deliberately NOT paged per-expert — it is
//! one small tensor consulted for EVERY token's routing and stays resident.

use candle_core::quantized::gguf_file::Content;

use crate::inference_capability::gguf_keys;

/// The stacked expert-projection tensor suffixes, in canonical range order. A
/// layer carries EITHER the split trio (`gate`/`up`/`down`) OR the fused pair
/// (`gate_up`/`down`); we emit ranges for whichever are present, so both export
/// styles work without the caller knowing which it has.
const EXPS_SUFFIXES: &[&str] = &[
    "ffn_gate_exps",
    "ffn_up_exps",
    "ffn_gate_up_exps",
    "ffn_down_exps",
];

/// One MoE expert's physical location: the `(absolute_offset, len)` byte ranges
/// of its slices across the stacked projection tensors in one layer. Feeds
/// `ArtifactBlob::mapped(id, gguf_path, ranges)` — one whole expert per artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpertLocation {
    pub layer: u32,
    pub expert: u32,
    /// Absolute-in-file `(offset, len)` ranges, one per present projection.
    pub ranges: Vec<(u64, u64)>,
}

impl ExpertLocation {
    /// Total bytes this expert occupies — the sum of its range lengths. Matches
    /// what `ArtifactSource::Mapped::size_bytes` will bill the tier store.
    pub fn total_bytes(&self) -> u64 {
        self.ranges.iter().map(|(_, len)| *len).sum()
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

/// Enumerate every `(layer, expert)` in a MoE GGUF and its physical byte ranges.
/// Returns one [`ExpertLocation`] per `(layer, expert)`, skipping dense layers
/// (layers with no `*_exps` tensors). `Err(NotMoe)` for a dense model.
pub fn locate_experts(ct: &Content, arch: &str) -> Result<Vec<ExpertLocation>, ExpertLayoutError> {
    let n_experts = gguf_keys::expert_count(ct, arch).ok_or(ExpertLayoutError::NotMoe)?;
    if n_experts == 0 {
        return Err(ExpertLayoutError::ZeroExperts);
    }
    let n_layers = gguf_keys::block_count(ct, arch).ok_or(ExpertLayoutError::MissingBlockCount)?;

    let mut out = Vec::new();
    for layer in 0..n_layers {
        // Per-projection (base_offset, per_expert_stride) for every stacked
        // expert tensor present in this layer, in canonical suffix order.
        let mut slices: Vec<(u64, u64)> = Vec::new();
        for suffix in EXPS_SUFFIXES {
            let name = format!("blk.{layer}.{suffix}.weight");
            let Some(info) = ct.tensor_infos.get(&name) else {
                continue;
            };
            let elems = info.shape.elem_count();
            let block_size = info.ggml_dtype.block_size();
            let type_size = info.ggml_dtype.type_size();
            // elems is always divisible by block_size for a valid GGUF tensor;
            // guard anyway so a corrupt header fails loud, not with wrong math.
            let tensor_bytes = (elems / block_size * type_size) as u64;
            if tensor_bytes % n_experts as u64 != 0 {
                return Err(ExpertLayoutError::UnevenStack {
                    tensor: name,
                    tensor_bytes,
                    n_experts,
                });
            }
            let stride = tensor_bytes / n_experts as u64;
            let base = ct.tensor_data_offset + info.offset;
            slices.push((base, stride));
        }
        if slices.is_empty() {
            continue; // dense layer — no experts to page
        }
        for expert in 0..n_experts {
            let ranges = slices
                .iter()
                .map(|(base, stride)| (base + expert as u64 * stride, *stride))
                .collect();
            out.push(ExpertLocation {
                layer,
                expert,
                ranges,
            });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use candle_core::quantized::gguf_file::{Content, TensorInfo, Value, VersionedMagic};
    use candle_core::quantized::GgmlDType;
    use candle_core::Shape;
    use std::collections::HashMap;

    // F32 keeps the byte math trivial (block_size 1, type_size 4): a
    // [4, 2, 8] tensor = 64 elems = 256 bytes, per-expert stride = 64.
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

    // what this catches: the core stacked-slice math — expert e's ranges are
    // `base + e*stride` across each present projection, layers with no *_exps
    // tensors are skipped (dense), and every expert bills the summed stride.
    #[test]
    fn locates_stacked_experts_and_skips_dense_layers() {
        // n_experts=4, 2 layers. Layer 0 dense (no exps). Layer 1 MoE with the
        // split trio at distinct offsets. tensor_data_offset=1000.
        let md = vec![
            ("qwen3moe.expert_count", Value::U32(4)),
            ("qwen3moe.block_count", Value::U32(2)),
        ];
        let tensors = vec![
            // layer 0 is dense — an attention tensor, no experts
            ("blk.0.attn_q.weight", f32_info(&[2, 8], 0)),
            // layer 1: gate/up/down stacked over 4 experts, 256 bytes each
            ("blk.1.ffn_gate_exps.weight", f32_info(&[4, 2, 8], 0)),
            ("blk.1.ffn_up_exps.weight", f32_info(&[4, 2, 8], 256)),
            ("blk.1.ffn_down_exps.weight", f32_info(&[4, 2, 8], 512)),
        ];
        let ct = content(md, tensors, 1000);

        let experts = locate_experts(&ct, "qwen3moe").unwrap();
        // 4 experts in layer 1 only (layer 0 dense → skipped).
        assert_eq!(experts.len(), 4);
        assert!(experts.iter().all(|e| e.layer == 1));

        // Expert 0: first slice of each of the 3 projections. stride = 256/4 = 64.
        let e0 = &experts[0];
        assert_eq!(e0.expert, 0);
        assert_eq!(
            e0.ranges,
            vec![(1000, 64), (1256, 64), (1512, 64)],
            "gate@1000 up@1256 down@1512, expert-0 slice at offset 0"
        );
        assert_eq!(e0.total_bytes(), 192);

        // Expert 3: last slice — base + 3*stride in each projection.
        let e3 = &experts[3];
        assert_eq!(e3.expert, 3);
        assert_eq!(
            e3.ranges,
            vec![(1000 + 192, 64), (1256 + 192, 64), (1512 + 192, 64)]
        );
    }

    // what this catches: a dense model (no expert_count key) is NotMoe, not a
    // panic or an empty-but-Ok — the splitter must distinguish "nothing to page"
    // as an explicit signal.
    #[test]
    fn dense_model_is_not_moe() {
        let ct = content(vec![("llama.block_count", Value::U32(4))], vec![], 100);
        assert_eq!(locate_experts(&ct, "llama"), Err(ExpertLayoutError::NotMoe));
    }

    // what this catches: the fused gate_up export style (gate+up in one tensor)
    // still resolves — an expert = the fused slice + the down slice (2 ranges),
    // proving EXPS_SUFFIXES handles both layouts without caller knowledge.
    #[test]
    fn fused_gate_up_export_resolves_two_ranges() {
        let md = vec![
            ("glm4moe.expert_count", Value::U32(2)),
            ("glm4moe.block_count", Value::U32(1)),
        ];
        let tensors = vec![
            ("blk.0.ffn_gate_up_exps.weight", f32_info(&[2, 4, 8], 0)),
            ("blk.0.ffn_down_exps.weight", f32_info(&[2, 2, 8], 512)),
        ];
        let ct = content(md, tensors, 0);
        let experts = locate_experts(&ct, "glm4moe").unwrap();
        assert_eq!(experts.len(), 2);
        // gate_up: [2,4,8]=64 elems=256B → stride 128; down: [2,2,8]=32 elems=128B → stride 64.
        assert_eq!(experts[0].ranges, vec![(0, 128), (512, 64)]);
        assert_eq!(experts[1].ranges, vec![(128, 128), (576, 64)]);
    }
}
