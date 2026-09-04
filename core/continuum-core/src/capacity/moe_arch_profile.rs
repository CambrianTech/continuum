//! MoE arch profile — every expert-serving fact READ from the artifact,
//! never typed (#231, Joel's law: "if some other random Mistral MoE comes
//! out, or DeepSeek, I'd expect most of the code to be ready for it").
//!
//! This is the adapter between a GGUF's self-description and everything
//! downstream that reasons about experts: the container manifest, the
//! cache-budget cliff, the pager's working set, grid shard placement. The
//! rule it enforces by construction: **no consumer may hand-type an arch
//! number.** `activated_per_token` — the cliff input the whole retention
//! story hangs on — is DERIVED here (`top_k × n_moe_layers`), so the
//! 488-vs-8 trap cannot be re-typed into existence.
//!
//! Model-family knowledge lives NOWHERE in this file. The keys are the
//! GGUF convention's arch-templated names (`{arch}.expert_count`, …) read
//! through [`crate::inference_capability::gguf_keys`] — one home per key.
//! A new MoE family that follows the convention (they all do; llama.cpp's
//! converters emit these keys) profiles with ZERO code changes. A family
//! with a genuinely new wrinkle earns a new KEY READER, not a name match.
//!
//! Outlier validation (the doctrine's two-extremes proof): Qwen3-MoE-style
//! (all layers routed, no shared experts, both optional keys absent) and
//! DeepSeek-style (dense leading blocks + always-active shared experts).
//! Both profile through the same code; the tests pin both.

use candle_core::quantized::gguf_file::Content;

use crate::inference_capability::gguf_keys;

use super::expert_container::ContainerManifest;

/// Why a GGUF could not be profiled as an MoE. Loud and specific: the
/// caller either serves the model dense (NotMoe) or refuses the artifact
/// (MissingKey = broken export) — there is no guessed default.
#[derive(Debug, PartialEq, Eq)]
pub enum MoeProfileError {
    /// `general.architecture` absent — a broken export, nothing is safe.
    NoArchitecture,
    /// No `{arch}.expert_count`: this artifact declares itself dense.
    /// Not an error to serve — an error to expert-page.
    NotMoe { arch: String },
    /// An MoE artifact missing a REQUIRED key (has expert_count but not the
    /// rest) — refuse rather than guess.
    MissingKey { arch: String, key: String },
    /// Self-description is internally inconsistent (e.g. dense lead ≥ total
    /// blocks). Refuse: arithmetic on it would be silently wrong.
    Inconsistent { arch: String, detail: String },
}

impl std::fmt::Display for MoeProfileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoArchitecture => {
                write!(f, "GGUF has no general.architecture — broken export")
            }
            Self::NotMoe { arch } => {
                write!(
                    f,
                    "{arch} declares no experts — dense model, nothing to page"
                )
            }
            Self::MissingKey { arch, key } => {
                write!(
                    f,
                    "{arch} is MoE but missing required key {key} — refusing to guess"
                )
            }
            Self::Inconsistent { arch, detail } => {
                write!(f, "{arch} self-description inconsistent: {detail}")
            }
        }
    }
}

/// The expert-serving facts of one MoE artifact, read from its own
/// metadata. Everything downstream (manifest, cliff, pager, shards)
/// consumes THIS — never raw numbers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoeArchProfile {
    /// The artifact's own architecture string (identity, never matched on).
    pub arch: String,
    /// Total transformer blocks.
    pub n_layers: u32,
    /// Blocks that actually route experts (= n_layers − leading dense).
    /// This is the container's bank count and the cliff multiplier.
    pub n_moe_layers: u32,
    /// Routed experts per MoE layer.
    pub experts_per_layer: u32,
    /// Router top-k per layer.
    pub top_k: u32,
    /// Always-active shared experts per layer — resident trunk weights,
    /// excluded from routed-cache arithmetic by definition.
    pub shared_experts: u32,
    /// Partial LAYER offload is FORBIDDEN for this artifact: its own
    /// per-layer `attention.head_count_kv` array contains zeros — recurrent
    /// (GDN/SSM) layers whose fused ops cannot span CPU/GPU buffers (the
    /// `node->buffer->buft` assertion crash, BigMama's registered 5090
    /// issue 3 / #238). Serve all-resident-on-GPU (experts paged INTO VRAM
    /// is fine — that's the expert axis, not the layer axis) or route to a
    /// grid peer; a launcher must refuse to emit a partial `-ngl` for this
    /// model. DATA from the artifact's self-description — never an
    /// arch-name match (#70).
    pub uniform_offload_required: bool,
}

impl MoeArchProfile {
    /// Profile a GGUF from its own metadata. Fail-loud per the error enum;
    /// the ONLY tolerated absences are the two optional keys whose absence
    /// is itself the arch's declaration (no dense lead / no shared experts).
    pub fn from_gguf(ct: &Content) -> Result<Self, MoeProfileError> {
        let arch = gguf_keys::architecture(ct).ok_or(MoeProfileError::NoArchitecture)?;
        let experts_per_layer = gguf_keys::expert_count(ct, &arch)
            .ok_or_else(|| MoeProfileError::NotMoe { arch: arch.clone() })?;
        let n_layers =
            gguf_keys::block_count(ct, &arch).ok_or_else(|| MoeProfileError::MissingKey {
                arch: arch.clone(),
                key: format!("{arch}.block_count"),
            })?;
        let top_k =
            gguf_keys::expert_used_count(ct, &arch).ok_or_else(|| MoeProfileError::MissingKey {
                arch: arch.clone(),
                key: format!("{arch}.expert_used_count"),
            })?;
        let leading_dense = gguf_keys::leading_dense_block_count(ct, &arch).unwrap_or(0);
        let shared_experts = gguf_keys::expert_shared_count(ct, &arch).unwrap_or(0);

        if leading_dense >= n_layers {
            return Err(MoeProfileError::Inconsistent {
                arch,
                detail: format!(
                    "leading_dense_block_count {leading_dense} >= block_count {n_layers}"
                ),
            });
        }
        if top_k == 0 || top_k > experts_per_layer {
            return Err(MoeProfileError::Inconsistent {
                arch,
                detail: format!("expert_used_count {top_k} vs expert_count {experts_per_layer}"),
            });
        }
        let uniform_offload_required = gguf_keys::attention_head_count_kv_per_layer(ct, &arch)
            .is_some_and(|per_layer| per_layer.contains(&0));

        Ok(Self {
            n_moe_layers: n_layers - leading_dense,
            arch,
            n_layers,
            experts_per_layer,
            top_k,
            shared_experts,
            uniform_offload_required,
        })
    }

    /// THE cliff input: total routed expert records touched per decoded
    /// token, across all MoE layers. Derived — the manifest field of the
    /// same name is a projection of this, never an independent number.
    pub fn activated_per_token(&self) -> u32 {
        self.top_k * self.n_moe_layers
    }

    /// Project this profile into a container manifest. The foundry supplies
    /// only what the QUANTIZATION decided (`fmt`, `record_bytes`) and the
    /// human-facing `model` name; every arch fact flows from the profile,
    /// which flowed from the artifact. Hand-typing 488 is impossible here.
    ///
    /// Container layer indices are MoE-layer ordinals: dense leading blocks
    /// have no banks, so `experts-L0.bin` is the FIRST ROUTED layer.
    pub fn manifest(
        &self,
        model: impl Into<String>,
        fmt: impl Into<String>,
        record_bytes: u64,
    ) -> ContainerManifest {
        ContainerManifest {
            version: 1,
            model: model.into(),
            fmt: fmt.into(),
            record_bytes,
            n_layers: self.n_moe_layers as u16,
            experts_per_layer: self.experts_per_layer as u16,
            activated_per_token: self.activated_per_token(),
            top_k_per_layer: Some(self.top_k),
            // Single-tier v1 projection: the tier TABLE is the quantizer's
            // decision (which tiers to emit), not an arch fact — the foundry
            // grows this into a v2 manifest when it packs multiple tiers.
            tiers: vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use candle_core::quantized::gguf_file::{self, Value};
    use std::io::Cursor;

    fn content_with(md: Vec<(&str, Value)>) -> Content {
        let refs: Vec<(&str, &Value)> = md.iter().map(|(k, v)| (*k, v)).collect();
        let mut buf = Cursor::new(Vec::new());
        gguf_file::write(&mut buf, &refs, &[]).unwrap();
        buf.set_position(0);
        Content::read(&mut buf).unwrap()
    }

    // what this catches (#231 outlier A): a Qwen3-MoE-shaped artifact — every
    // layer routed, no shared experts, both optional keys ABSENT — profiles
    // with the absence read as the arch's own declaration, and the cliff
    // input derives as top_k × ALL layers. No qwen-specific code anywhere.
    #[test]
    fn qwen3moe_shape_profiles_with_all_layers_routed() {
        let ct = content_with(vec![
            ("general.architecture", Value::String("qwen3moe".into())),
            ("qwen3moe.block_count", Value::U32(48)),
            ("qwen3moe.expert_count", Value::U32(128)),
            ("qwen3moe.expert_used_count", Value::U32(8)),
        ]);
        let p = MoeArchProfile::from_gguf(&ct).expect("profiles");
        assert_eq!(p.n_moe_layers, 48, "no dense lead declared → all routed");
        assert_eq!(p.shared_experts, 0);
        assert_eq!(p.activated_per_token(), 8 * 48);

        let m = p.manifest("qwen3-moe-30b", "VQ3R", 4096);
        assert_eq!(m.activated_per_token, 384, "manifest field is DERIVED");
        assert_eq!(m.top_k_per_layer, Some(8));
        assert_eq!(m.n_layers, 48);
    }

    // what this catches: BigMama's registered 5090 issue 3 (#238) — a GDN/SSM
    // hybrid must declare uniform_offload_required from its OWN metadata: the
    // per-layer attention.head_count_kv ARRAY containing zeros (recurrent
    // layers whose fused ops cannot span CPU/GPU buffers — the
    // node->buffer->buft assertion on partial -ngl). Detection is artifact
    // DATA, never an arch-name match (#70): the same key as a SCALAR (a
    // uniform GQA MoE) must NOT set the flag.
    #[test]
    fn hybrid_recurrent_arch_requires_uniform_offload() {
        let hybrid = content_with(vec![
            ("general.architecture", Value::String("kimi-k3".into())),
            ("kimi-k3.block_count", Value::U32(8)),
            ("kimi-k3.expert_count", Value::U32(896)),
            ("kimi-k3.expert_used_count", Value::U32(16)),
            (
                "kimi-k3.attention.head_count_kv",
                Value::Array(vec![
                    Value::U32(0),
                    Value::U32(0),
                    Value::U32(0),
                    Value::U32(1),
                    Value::U32(0),
                    Value::U32(0),
                    Value::U32(0),
                    Value::U32(1),
                ]),
            ),
        ]);
        assert!(
            MoeArchProfile::from_gguf(&hybrid)
                .unwrap()
                .uniform_offload_required,
            "zeros in the per-layer KV-head array declare recurrent layers → \
             partial layer offload must be refused"
        );

        let uniform = content_with(vec![
            ("general.architecture", Value::String("qwen3moe".into())),
            ("qwen3moe.block_count", Value::U32(4)),
            ("qwen3moe.expert_count", Value::U32(128)),
            ("qwen3moe.expert_used_count", Value::U32(8)),
            ("qwen3moe.attention.head_count_kv", Value::U32(8)),
        ]);
        assert!(
            !MoeArchProfile::from_gguf(&uniform)
                .unwrap()
                .uniform_offload_required,
            "a scalar head_count_kv (uniform GQA) must not forbid partial offload"
        );
    }

    // what this catches (#231 outlier B): a DeepSeek-shaped artifact — dense
    // leading blocks + always-active shared experts — subtracts the dense
    // lead from the bank/cliff arithmetic and surfaces shared experts as a
    // RESIDENT fact, all from the artifact's own keys. Same code as outlier A.
    #[test]
    fn deepseek_shape_subtracts_dense_lead_and_carries_shared_experts() {
        let ct = content_with(vec![
            ("general.architecture", Value::String("deepseek2".into())),
            ("deepseek2.block_count", Value::U32(61)),
            ("deepseek2.leading_dense_block_count", Value::U32(3)),
            ("deepseek2.expert_count", Value::U32(256)),
            ("deepseek2.expert_used_count", Value::U32(8)),
            ("deepseek2.expert_shared_count", Value::U32(1)),
        ]);
        let p = MoeArchProfile::from_gguf(&ct).expect("profiles");
        assert_eq!(p.n_moe_layers, 58, "61 blocks − 3 dense leads");
        assert_eq!(p.shared_experts, 1, "resident, not cached");
        assert_eq!(p.activated_per_token(), 8 * 58);
        assert_eq!(
            p.manifest("deepseek-v3", "VQ3R", 4096).activated_per_token,
            464
        );
    }

    // what this catches: the refusal ladder — dense models are NotMoe (serve
    // dense, don't page), an MoE missing a required key refuses loudly, and
    // an internally inconsistent self-description refuses rather than doing
    // silently-wrong arithmetic. No guessed defaults anywhere on this path.
    #[test]
    fn refusals_are_specific_and_never_guess() {
        let dense = content_with(vec![
            ("general.architecture", Value::String("llama".into())),
            ("llama.block_count", Value::U32(32)),
        ]);
        assert_eq!(
            MoeArchProfile::from_gguf(&dense),
            Err(MoeProfileError::NotMoe {
                arch: "llama".into()
            })
        );

        let missing_topk = content_with(vec![
            ("general.architecture", Value::String("somenewmoe".into())),
            ("somenewmoe.block_count", Value::U32(40)),
            ("somenewmoe.expert_count", Value::U32(64)),
        ]);
        assert!(matches!(
            MoeArchProfile::from_gguf(&missing_topk),
            Err(MoeProfileError::MissingKey { .. })
        ));

        let inconsistent = content_with(vec![
            ("general.architecture", Value::String("brokenmoe".into())),
            ("brokenmoe.block_count", Value::U32(10)),
            ("brokenmoe.leading_dense_block_count", Value::U32(10)),
            ("brokenmoe.expert_count", Value::U32(64)),
            ("brokenmoe.expert_used_count", Value::U32(8)),
        ]);
        assert!(matches!(
            MoeArchProfile::from_gguf(&inconsistent),
            Err(MoeProfileError::Inconsistent { .. })
        ));
    }
}
