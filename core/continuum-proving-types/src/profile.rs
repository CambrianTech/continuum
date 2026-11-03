//! What is measured on a node, each value with its source, time and error (THE-PROVING-GROUND 8a,
//! P2). Produced from the node's own `~/.continuum/state/{decode-knee,prefill-rate,serving-rates}.json`,
//! its engine log and a bench run; a machine we do not have is a declared shape, every domain
//! `Unmeasured` until a real one supplies it.

use serde::{Deserialize, Serialize};

use crate::closed::{ModelId, Slug};
use crate::provenance::Measured;

/// A memory tier a byte can live on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    /// A discrete GPU's memory.
    Device,
    /// Memory the GPU and CPU share (Apple silicon).
    Unified,
    Ram,
    Disk,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TierCapacity {
    pub tier: Tier,
    pub bytes: Measured<u64>,
}

/// The engine's slot layout, so a slot layout is data (4a; `/props` `context_geometry`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SlotGeometry {
    pub sequences: u32,
    /// The KV pool's cells: the one shared pool under `--kv-unified`, the slots' sum without it.
    pub allocated_tokens: u32,
    pub max_tokens_per_sequence: u32,
    pub kv_unified: bool,
}

/// A rate as a function of how many jobs share the engine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "shape", rename_all = "snake_case", deny_unknown_fields)]
pub enum RateCurve {
    /// Measured at each concurrency (the decode knee's points); the harness interpolates.
    Points { points: Vec<RatePoint> },
    /// A fitted `scale x n^exponent` tokens per second, as the flow sim's M5 constants are
    /// (decode `30.5 x n^0.865`, prefill `304 x n^-0.5`, 2026-10-28). The fit's error is the
    /// scale's spread; its points are not kept.
    PowerLaw { scale: Measured<f64>, exponent: f64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RatePoint {
    pub concurrency: u32,
    pub tokens_per_s: Measured<f64>,
}

/// What a move between tiers costs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PageCost {
    pub from: Tier,
    pub to: Tier,
    pub bytes_per_s: Measured<f64>,
}

/// A fixed holder of engine memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Holder {
    Weights,
    /// The compute graph: one for the shared pool under `--kv-unified`.
    ComputeGraph,
    /// The speculative draft model's whole context (its weights, KV and compute).
    Draft,
    /// The multimodal projector's weights and reserved compute.
    Projector,
    /// The host prompt cache's byte limit.
    PromptCache,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HolderBytes {
    pub holder: Holder,
    pub tier: Tier,
    pub bytes: Measured<u64>,
}

/// One model as served on this node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelProfile {
    pub model: ModelId,
    /// Everything the engine holds whatever the minds do, each where it lives (the fork's
    /// `memory_breakdown` and `resident_state`, llama.cpp#91). Before #91 the draft and the
    /// projector were invisible, and the plan inferred them from the process footprint.
    pub holders: Vec<HolderBytes>,
    /// Attention KV per token of a sequence's context.
    pub kv_bytes_per_token: Measured<u64>,
    /// A hybrid model's recurrent state, fixed per sequence whatever its length. Folding it into the
    /// per-token rate overstates long sequences and short-changes many short ones (card c8b2e1df).
    /// `Unmeasured` until the fork reports it apart from the KV.
    pub recurrent_bytes_per_sequence: Measured<u64>,
    pub prefill: RateCurve,
    pub decode: RateCurve,
    pub page: Vec<PageCost>,
    /// One context checkpoint's bytes (`/slots` `checkpoints.max_bytes`, fork #91).
    pub checkpoint_bytes: Measured<u64>,
    pub slots: Measured<SlotGeometry>,
}

/// A node's measured world (THE-PROVING-GROUND 2, 8a).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeProfile {
    /// The node's name on the grid (`m5`, `intelmac`, `bigmama-5090`), or a declared class
    /// (`class/mac-unified-16g`) for a machine we do not have.
    pub node: Slug,
    pub memory: Vec<TierCapacity>,
    pub cpu_cores: Measured<u32>,
    pub models: Vec<ModelProfile>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a declared shape for a machine we do not have reading as measured, so a
    // sweep over the hardware classes would report another node's numbers as that class's.
    #[test]
    fn a_declared_class_is_unmeasured_on_the_wire() {
        let declared = NodeProfile {
            node: Slug::try_from("class/mac-unified-16g".to_string()).expect("slug"),
            memory: vec![TierCapacity { tier: Tier::Unified, bytes: Measured::Unmeasured {} }],
            cpu_cores: Measured::Unmeasured {},
            models: vec![],
        };
        let wire = serde_json::to_value(&declared).expect("ser");
        assert_eq!(wire["memory"][0]["bytes"], serde_json::json!({ "state": "unmeasured" }));
        assert_eq!(serde_json::from_value::<NodeProfile>(wire).expect("de"), declared);
    }

    // what this catches: a model profile that cannot carry what the fork reports, which is how the
    // plan went wrong on the M5 (it saw weights and KV, not the draft, the projector or the graph).
    #[test]
    fn a_model_profile_carries_every_fixed_holder_and_the_recurrent_split() {
        use crate::provenance::{Source, Spread};
        let slug = |s: &str| Slug::try_from(s.to_string()).expect("slug");
        let measured = |value: u64| Measured::Measured {
            value,
            at_ms: 1_793_710_260_000,
            source: Source { kind: slug("engine/resident-state"), node: slug("m5") },
            samples: 1,
            error: Spread::NotYetMeasured,
        };
        let holder = |holder, bytes| HolderBytes { holder, tier: Tier::Unified, bytes: measured(bytes) };
        let model = ModelProfile {
            model: crate::closed::ModelId::try_from("continuum-ai/qwen3.6-35b-a3b-GGUF".to_string()).expect("model id"),
            holders: vec![
                holder(Holder::Weights, 20_690_000_000),
                holder(Holder::ComputeGraph, 1_870_000_000),
                holder(Holder::Draft, 3_150_000_000),
                holder(Holder::Projector, 900_000_000),
            ],
            kv_bytes_per_token: measured(8_317),
            recurrent_bytes_per_sequence: Measured::Unmeasured {},
            prefill: RateCurve::Points { points: vec![] },
            decode: RateCurve::Points { points: vec![] },
            page: vec![],
            checkpoint_bytes: measured(94_600_000),
            slots: Measured::Unmeasured {},
        };
        let wire = serde_json::to_value(&model).expect("ser");
        assert_eq!(wire["recurrent_bytes_per_sequence"], serde_json::json!({ "state": "unmeasured" }));
        assert_eq!(serde_json::from_value::<ModelProfile>(wire).expect("de"), model);
    }
}
