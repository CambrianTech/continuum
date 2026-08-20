//! Expert ingest (Seam-1 splitter) — register a MoE GGUF's per-layer expert
//! sets as file-mapped artifacts on a tier, without copying a byte.
//!
//! One artifact per MoE layer (the expert set), keyed by a deterministic
//! `ArtifactId` derived from `(gguf_id, layer)`. The bytes stay in the GGUF on
//! the frozen tier drive; the artifact just references their ranges
//! ([`ArtifactSource::Mapped`]). At serve time the residency layer pages
//! individual experts via `PageRef { offset: PageOffset::Expert{e} }`, resolved
//! by [`ArtifactSource::expert_ranges`]. This is the write half; the read/page
//! half is Seam-2.

use std::path::Path;

use uuid::Uuid;

use super::blob::{ArtifactBlob, Provenance};
use super::expert_layout::{locate_layer_sets, ExpertLayoutError};
use super::store::TierStore;
use super::tier::TierError;
use super::working_set::{ArtifactId, PageKind, PageOffset, PageRef};
use candle_core::quantized::gguf_file::Content;

/// Stable namespace for `(gguf_id, layer)` → expert-set `ArtifactId` derivation.
/// A fixed constant so the same GGUF layer always yields the same artifact id on
/// any machine (deterministic, content-addressed at model+layer granularity).
const EXPERT_SET_NAMESPACE: Uuid = Uuid::from_u128(0x6b33_e17e_5ea1_4c0d_9a11_e59e0f7ac001);

/// The deterministic artifact id for one layer's expert set of a given GGUF.
/// `gguf_id` is the model's own content identity (registry id / blob hash) —
/// same model + same layer → same id, so re-ingest is idempotent and two nodes
/// agree on the id without coordination.
pub fn expert_set_artifact_id(gguf_id: &str, layer: u32) -> ArtifactId {
    let key = format!("{gguf_id}:{layer}");
    ArtifactId::new(Uuid::new_v5(&EXPERT_SET_NAMESPACE, key.as_bytes()))
}

/// What an ingest run registered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestOutcome {
    /// One `PageRef` per registered layer-set artifact (offset `Whole` — the
    /// stored page IS the whole set; per-expert reads use `Expert{e}`).
    pub artifacts: Vec<PageRef>,
    /// Total experts registered across all MoE layers (`n_experts × moe_layers`).
    pub total_experts: u64,
    /// Total bytes referenced across all layer sets (not copied — referenced).
    pub total_bytes: u64,
}

/// Why an ingest run failed.
#[derive(Debug)]
pub enum IngestError {
    /// The GGUF layout could not be resolved (dense model, malformed header).
    Layout(ExpertLayoutError),
    /// A tier write failed (backing-store I/O, no eviction candidate, …).
    Tier(TierError),
}

impl From<ExpertLayoutError> for IngestError {
    fn from(e: ExpertLayoutError) -> Self {
        IngestError::Layout(e)
    }
}
impl From<TierError> for IngestError {
    fn from(e: TierError) -> Self {
        IngestError::Tier(e)
    }
}

/// Split a MoE GGUF into per-layer expert-set artifacts and register each on
/// `tier` (the frozen/cold tier). Zero-copy: builds `Mapped` references, never
/// reads the expert bytes. `now_ms` stamps provenance (caller supplies it so the
/// function stays deterministic + testable).
pub async fn ingest_expert_sets(
    gguf_path: &Path,
    gguf_id: &str,
    ct: &Content,
    arch: &str,
    now_ms: u64,
    tier: &dyn TierStore,
) -> Result<IngestOutcome, IngestError> {
    let sets = locate_layer_sets(ct, arch)?;
    let mut artifacts = Vec::with_capacity(sets.len());
    let mut total_experts: u64 = 0;
    let mut total_bytes: u64 = 0;

    for set in &sets {
        let id = expert_set_artifact_id(gguf_id, set.layer);
        let blob = ArtifactBlob::mapped(
            id,
            gguf_path.to_path_buf(),
            set.n_experts,
            set.projections.clone(),
        );
        total_bytes += blob.size_bytes();
        total_experts += set.n_experts as u64;

        let page = PageRef {
            kind: PageKind::MoEExpert,
            artifact: id,
            offset: PageOffset::Whole,
        };
        tier.write(page, blob, Provenance::minimal(id, now_ms))
            .await?;
        artifacts.push(page);
    }

    Ok(IngestOutcome {
        artifacts,
        total_experts,
        total_bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::blob::ArtifactSource;
    use crate::genome::tier::{EvictionRecord, TierRole};
    use crate::genome::working_set::PageHandle;
    use async_trait::async_trait;
    use candle_core::quantized::gguf_file::{Content, TensorInfo, Value, VersionedMagic};
    use candle_core::quantized::GgmlDType;
    use candle_core::Shape;
    use std::path::PathBuf;
    use std::sync::Mutex;

    // Records every write so the test can assert what the splitter produced
    // WITHOUT a real disk tier. A test double for the write side only — read()
    // is unused here.
    #[derive(Default)]
    struct RecordingTier {
        writes: Mutex<Vec<(PageRef, ArtifactBlob, Provenance)>>,
    }

    #[async_trait]
    impl TierStore for RecordingTier {
        fn role(&self) -> TierRole {
            TierRole::Frozen
        }
        async fn read(&self, page: PageRef) -> Result<PageHandle, TierError> {
            Err(TierError::PageNotFound { page })
        }
        async fn write(
            &self,
            page: PageRef,
            blob: ArtifactBlob,
            provenance: Provenance,
        ) -> Result<(), TierError> {
            self.writes.lock().unwrap().push((page, blob, provenance));
            Ok(())
        }
        async fn evict(&self, _target_free_bytes: usize) -> Vec<EvictionRecord> {
            Vec::new()
        }
        fn capacity(&self) -> crate::genome::tier::TierCapacity {
            // Unlimited test tier — the write-side double never triggers eviction.
            crate::genome::tier::TierCapacity {
                current_used: 0,
                configured_limit: u64::MAX,
            }
        }
        fn observe_access(&self, _page: PageRef) {}
    }

    fn f32_info(shape: &[usize], offset: u64) -> TensorInfo {
        TensorInfo {
            ggml_dtype: GgmlDType::F32,
            shape: Shape::from(shape.to_vec()),
            offset,
        }
    }

    fn moe_gguf() -> Content {
        // 2 layers, both MoE, 4 experts. Each [4,2,8] tensor = 256 bytes.
        let md = vec![
            ("qwen3moe.expert_count", Value::U32(4)),
            ("qwen3moe.block_count", Value::U32(2)),
        ];
        let tensors = vec![
            ("blk.0.ffn_gate_exps.weight", f32_info(&[4, 2, 8], 0)),
            ("blk.0.ffn_up_exps.weight", f32_info(&[4, 2, 8], 256)),
            ("blk.0.ffn_down_exps.weight", f32_info(&[4, 2, 8], 512)),
            ("blk.1.ffn_gate_exps.weight", f32_info(&[4, 2, 8], 768)),
            ("blk.1.ffn_up_exps.weight", f32_info(&[4, 2, 8], 1024)),
            ("blk.1.ffn_down_exps.weight", f32_info(&[4, 2, 8], 1280)),
        ];
        Content {
            magic: VersionedMagic::GgufV3,
            metadata: md.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
            tensor_infos: tensors
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
            tensor_data_offset: 0,
        }
    }

    // what this catches: the splitter registers ONE artifact per MoE layer, with
    // a deterministic (gguf_id, layer) id, a Mapped source referencing the layer's
    // projections, PageOffset::Whole, and never reads bytes (the path is bogus).
    // Also that a resolved expert's ranges are correct end-to-end.
    #[tokio::test]
    async fn ingest_registers_one_artifact_per_layer() {
        let ct = moe_gguf();
        let tier = RecordingTier::default();
        let outcome = ingest_expert_sets(
            &PathBuf::from("/frozen/does-not-exist.gguf"),
            "qwen3moe-test@v1",
            &ct,
            "qwen3moe",
            1_700_000_000_000,
            &tier,
        )
        .await
        .unwrap();

        // 2 MoE layers → 2 artifacts, 4 experts each → 8 total.
        assert_eq!(outcome.artifacts.len(), 2);
        assert_eq!(outcome.total_experts, 8);
        // 2 layers × 3 projections × 256 bytes = 1536.
        assert_eq!(outcome.total_bytes, 1536);

        let writes = tier.writes.lock().unwrap();
        assert_eq!(writes.len(), 2);

        // Deterministic id for (gguf_id, layer) — stable across runs/machines.
        let (page0, blob0, prov0) = &writes[0];
        assert_eq!(page0.kind, PageKind::MoEExpert);
        assert_eq!(page0.offset, PageOffset::Whole);
        assert_eq!(
            page0.artifact,
            expert_set_artifact_id("qwen3moe-test@v1", 0)
        );
        assert_eq!(prov0.artifact_id, page0.artifact);

        // The blob is Mapped over layer 0's 3 projections; expert 3 resolves to
        // base + 3*(256/4)=+192, stride 64 in each projection.
        match &blob0.source {
            ArtifactSource::Mapped {
                n_experts,
                projections,
                ..
            } => {
                assert_eq!(*n_experts, 4);
                assert_eq!(projections, &vec![(0, 256), (256, 256), (512, 256)]);
            }
            _ => panic!("expected Mapped"),
        }
        assert_eq!(
            blob0.source.expert_ranges(3),
            Some(vec![(192, 64), (448, 64), (704, 64)])
        );
    }

    // what this catches: a dense GGUF is a clean IngestError::Layout(NotMoe), not
    // a partial write — the splitter refuses rather than registering nonsense.
    #[tokio::test]
    async fn dense_gguf_refuses() {
        let ct = Content {
            magic: VersionedMagic::GgufV3,
            metadata: [("llama.block_count".to_string(), Value::U32(4))]
                .into_iter()
                .collect(),
            tensor_infos: Default::default(),
            tensor_data_offset: 0,
        };
        let tier = RecordingTier::default();
        let err = ingest_expert_sets(
            &PathBuf::from("/x.gguf"),
            "llama-test",
            &ct,
            "llama",
            1,
            &tier,
        )
        .await
        .unwrap_err();
        assert!(matches!(
            err,
            IngestError::Layout(ExpertLayoutError::NotMoe)
        ));
        assert!(tier.writes.lock().unwrap().is_empty());
    }
}
