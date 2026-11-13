//! The typed vocabulary of a LoRA's curriculum: the examples a persona learns, where they came
//! from, and the shape and schedule she learns them at.
//!
//! No `serde_json::Value` pass-through. Every field that crosses an
//! adapter boundary is typed at compile time. Per the
//! [[noteworthy-flag-feeds-memory-AND-curriculum]] doctrine the
//! dataset's origin (teacher-synthesized vs operator-curated vs raw)
//! is load-bearing — it's a typed enum, not a magic string.

use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

// ─── Request ─────────────────────────────────────────────────────────

/// What a gene was learned from: written once as its provenance (`request.json` in its job
/// directory, `in_place::snapshot`), and read back by the examiner.
///
/// The `persona_id` is load-bearing — the resulting LoRA artifact is
/// genome-paged into THAT persona's working set, and the
/// [[matrix-dojo-layer-loading-as-substrate-primitive]] mesh routes
/// it by persona identity when other personas request the layer.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/fine_tuning/TrainingJobRequest.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct TrainingJobRequest {
    /// Owning persona — the layer is paged into this persona's working
    /// set when the matching skill activates.
    #[ts(type = "string")]
    pub persona_id: Uuid,
    /// Human-readable persona name for telemetry + alloy
    /// attribution. Substrate-derived from the persona seed;
    /// caller-passed for adapters that need it in the job metadata.
    pub persona_name: String,
    /// Base model the LoRA layer attaches to. Must match an entry in
    /// the substrate's model registry; the adapter validates that
    /// its provider hosts the base.
    pub base_model: String,
    /// What this layer is meant to encode — used as the skill key in
    /// genome paging. e.g. "typescript-expertise", "kc-tech-history",
    /// or a persona-named trait like "maya-voice".
    pub trait_kind: String,
    /// The curated dataset, always populated.
    pub dataset: TrainingDataset,
    /// A JSONL eval-set path (the `cognition/eval` `eval_set`) for a MANUAL
    /// spot-check of this trait. It decides nothing: the automatic adoption
    /// path no longer runs
    /// an eval copy of her mind (Joel, 2026-09-27: integrated, not parallel).
    /// It registers the gene dormant and opens an in-room
    /// [`GeneTrial`](crate::genome::gene_trial::GeneTrial), whose card
    /// outcomes promote or retire it, whether or not this is set.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub eval_set: Option<String>,
    /// LoRA-specific hyperparams (rank, alpha, dropout, target
    /// modules). `None` lets the adapter pick its provider defaults —
    /// `Some` overrides them.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub lora: Option<LoRAHyperparams>,
    /// Training schedule (epochs, batch size, learning rate, sequence
    /// length). `None` lets the adapter pick provider defaults.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub schedule: Option<ScheduleParams>,
    /// Where the resulting artifact should land on local disk.
    /// `None` lets the adapter pick — usually
    /// `~/.continuum/genome/<persona>/<trait_kind>/<job_uuid>.safetensors`.
    /// Provider artifacts (OpenAI's `ft:gpt-4o-mini:...`) don't have a
    /// local path; the field is `None` in their [`TrainingArtifact`].
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub local_artifact_dir: Option<PathBuf>,
    /// A checkpoint directory (a PEFT adapter plus its `state.json`) this job continues
    /// from — set by the resume of a reboot-killed job (card 244757bc) so it loses
    /// minutes, not the run. Absent on a fresh job.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub resume_from: Option<PathBuf>,
    /// The gene this job forks from (`Decision::Fork`): the child's lineage, stamped
    /// into its signature at adoption. A trainer that can warm-start from it does; one
    /// that cannot trains from the base and says so on its probe. Absent on a mint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub parent: Option<crate::genome::competence::GeneRef>,
}

// ─── Dataset ─────────────────────────────────────────────────────────

/// A curated set of examples + the audit trail of where they came
/// from. The audit trail is load-bearing for the
/// [[teacher-synthesizes-in-academy-like-dreaming]] doctrine —
/// teacher-synthesized datasets carry a different reputation signal
/// than raw-experience datasets.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/fine_tuning/TrainingDataset.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct TrainingDataset {
    pub examples: Vec<TrainingExample>,
    pub source: TrainingSource,
    /// Fraction of `examples` reserved for validation
    /// (default `0.1`, range `[0.0, 0.5]`). Adapters that don't
    /// support a validation split clamp to `0.0`.
    pub validation_split: f32,
}

impl TrainingDataset {
    /// Load a chat-format dataset (the `dataset/*` commands' `{messages:[...]}`
    /// JSONL) into training pairs: every message before the final assistant turn
    /// folds into `prompt` in render order (system first — the same text the live
    /// turn saw), the final assistant turn is `completion`. This is the
    /// dataset-by-NAME seam: `dataset/from-captures` writes the corpus and
    /// `genome/teach` reads it by name — the recipe stays data on disk, never a
    /// multi-megabyte example blob hand-carried through argv.
    pub fn from_chat_jsonl(path: &std::path::Path, source: TrainingSource) -> Result<Self, String> {
        let raw = std::fs::read_to_string(path)
            .map_err(|e| format!("read dataset {}: {e}", path.display()))?;
        let mut examples = Vec::new();
        for (i, line) in raw.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let row: serde_json::Value = serde_json::from_str(line)
                .map_err(|e| format!("{}:{}: bad JSON: {e}", path.display(), i + 1))?;
            let msgs = row
                .get("messages")
                .and_then(|m| m.as_array())
                .ok_or_else(|| format!("{}:{}: row has no messages[]", path.display(), i + 1))?;
            let (last, context) = msgs
                .split_last()
                .ok_or_else(|| format!("{}:{}: empty messages[]", path.display(), i + 1))?;
            if last.get("role").and_then(|r| r.as_str()) != Some("assistant") {
                return Err(format!(
                    "{}:{}: final message is not the assistant turn",
                    path.display(),
                    i + 1
                ));
            }
            let completion = last
                .get("content")
                .and_then(|c| c.as_str())
                .unwrap_or_default()
                .to_string();
            let prompt = context
                .iter()
                .filter_map(|m| m.get("content").and_then(|c| c.as_str()))
                .collect::<Vec<_>>()
                .join("\n");
            if prompt.is_empty() || completion.is_empty() {
                return Err(format!(
                    "{}:{}: empty prompt or completion after fold",
                    path.display(),
                    i + 1
                ));
            }
            // Audit provenance belongs to the example, not just its JSONL wrapper.
            // Retain the existing top-level skillAxis mapping when both are present.
            let mut metadata = match row.get("metadata") {
                None | Some(serde_json::Value::Null) => None,
                Some(serde_json::Value::Object(object)) => Some(object.clone()),
                Some(_) => {
                    return Err(format!(
                        "{}:{}: metadata must be an object or null",
                        path.display(),
                        i + 1,
                    ))
                }
            };
            if let Some(axis) = row.get("skillAxis") {
                metadata
                    .get_or_insert_with(serde_json::Map::new)
                    .insert("skillAxis".into(), axis.clone());
            }
            examples.push(TrainingExample {
                prompt,
                completion,
                metadata: metadata.map(serde_json::Value::Object),
                lived: None,
            });
        }
        if examples.is_empty() {
            return Err(format!("{}: no training examples", path.display()));
        }
        Ok(Self {
            examples,
            source,
            validation_split: 0.05,
        })
    }
}

/// One training pair — the unit of evidence.
///
/// `metadata` is `serde_json::Value` because it's audit-trail
/// pass-through (which engram this came from, what session, what
/// noteworthy score). The substrate doesn't dispatch on it; it
/// flows into telemetry + alloy provenance.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/fine_tuning/TrainingExample.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct TrainingExample {
    pub prompt: String,
    pub completion: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(type = "Record<string, unknown> | undefined")]
    pub metadata: Option<serde_json::Value>,
    /// The call she actually lived, when this example came from one (card ad107e18):
    /// the exact request the engine was served and her exact response, reasoning and
    /// tool calls included. A trainer that renders it trains on what she saw and did;
    /// `prompt`/`completion` stay as its flat projection for trainers that take text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[schemars(with = "Option<serde_json::Value>")]
    pub lived: Option<LivedCall>,
}

/// One served call exactly as it happened, read back from the prompt capture.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/fine_tuning/LivedCall.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct LivedCall {
    /// The capture cursor it was read from: the link back to the trace.
    pub capture: String,
    /// The request as submitted to the engine: room, grounding, tools, stimulus.
    pub request: crate::ai::types::TextGenerationRequest,
    /// Her response: text, reasoning and tool calls.
    pub response: crate::ai::types::TextGenerationResponse,
}

/// Where this dataset came from. The substrate's reputation signal
/// for layers trained from teacher-synthesized data is distinct from
/// layers trained from raw operator-supplied corpora.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/fine_tuning/TrainingSource.ts"
)]
#[serde(rename_all = "snake_case")]
pub enum TrainingSource {
    /// Teacher persona in the academy synthesized this from noteworthy
    /// engrams. Substrate's dream phase
    /// ([[teacher-synthesizes-in-academy-like-dreaming]]).
    TeacherSynthesized,
    /// Operator-supplied corpus (a .jsonl file, a curated dataset
    /// repo). Lower reputation signal because no substrate teacher
    /// vouched for it.
    OperatorCurated,
    /// Raw conversation logs, unfiltered. Lowest reputation signal;
    /// useful for "this is what literally happened" experiments but
    /// not the doctrine's preferred input.
    Raw,
    /// Another persona's curriculum shared via the
    /// [[mesh-of-lessons-cross-persona-curricula]] mesh.
    MeshInherited,
}

// ─── Hyperparams ─────────────────────────────────────────────────────

/// LoRA-specific knobs. Same field set every cloud provider exposes
/// (rank + alpha + dropout + target_modules); local Candle adds none
/// beyond these.
///
/// `PartialEq` (not `Eq`) because of the `dropout: f32` field. The
/// training-trigger bucket-coherence check compares two
/// `LoRAHyperparams` via `!=`, which requires the trait.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/fine_tuning/LoRAHyperparams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct LoRAHyperparams {
    /// LoRA rank — the bottleneck dimension. Common: 4, 8, 16, 32, 64.
    /// Higher rank = more capacity but more weights to train + store.
    pub rank: u32,
    /// LoRA alpha — the scaling factor applied during merge
    /// (`W' = W + (alpha/rank) * B @ A`). Common heuristic: `alpha =
    /// rank * 2`.
    pub alpha: u32,
    /// Dropout on the LoRA branch during training. Common: 0.0, 0.05,
    /// 0.1. Defaults to 0.0 for most providers.
    pub dropout: f32,
    /// Which transformer projection layers to inject LoRA into.
    /// Empty `Vec` lets the adapter pick provider defaults
    /// (usually `q_proj` + `v_proj`).
    pub target_modules: Vec<String>,
    /// Adapt only the last `top_layers` transformer blocks; `None` adapts every block. The
    /// backward pass stops at the lowest adapted block, so the training graph's memory falls
    /// roughly linearly with this (fork #27: Qwen2.5-1.5B at window 512, 2576 MiB for all 28
    /// blocks, 1316 MiB for the last 7). A full-depth 27B run did not fit beside its serving
    /// context (41.5 GB at window 1536), so a resident dream chooses a depth. Only the
    /// in-engine trainer reads it; other adapters ignore it and train every block.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub top_layers: Option<u32>,
}

/// Training schedule knobs.
///
/// `PartialEq` (not `Eq`) because of `learning_rate: f64`. Same
/// motivation as `LoRAHyperparams` — the trigger module's bucket
/// coherence check needs to compare two `ScheduleParams` values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/fine_tuning/ScheduleParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleParams {
    pub epochs: u32,
    pub batch_size: u32,
    pub sequence_length: u32,
    pub learning_rate: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the dataset-by-NAME seam's fold contract — a chat
    // {messages} JSONL row (what dataset/from-captures writes) becomes ONE
    // TrainingExample whose prompt is every pre-assistant message joined in render
    // order and whose completion is the assistant turn. A row whose final message
    // is NOT the assistant fails LOUD (training on a context-as-completion row
    // would teach the model to parrot its own prompt).
    #[test]
    fn from_chat_jsonl_folds_context_and_rejects_non_assistant_tail() {
        let dir = std::env::temp_dir().join(format!("ds-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let good = dir.join("train.jsonl");
        std::fs::write(
            &good,
            concat!(
                r#"{"messages":[{"role":"system","content":"sys"},{"role":"user","content":"burst"},{"role":"assistant","content":"act"}],"skillAxis":"operational"}"#,
                "\n"
            ),
        )
        .unwrap();
        let ds = TrainingDataset::from_chat_jsonl(&good, TrainingSource::OperatorCurated).unwrap();
        assert_eq!(ds.examples.len(), 1);
        assert_eq!(ds.examples[0].prompt, "sys\nburst");
        assert_eq!(ds.examples[0].completion, "act");
        assert_eq!(
            ds.examples[0].metadata.as_ref().unwrap()["skillAxis"],
            "operational"
        );

        let bad = dir.join("bad.jsonl");
        std::fs::write(
            &bad,
            r#"{"messages":[{"role":"assistant","content":"act"},{"role":"user","content":"q"}]}"#,
        )
        .unwrap();
        let err =
            TrainingDataset::from_chat_jsonl(&bad, TrainingSource::OperatorCurated).unwrap_err();
        assert!(err.contains("not the assistant turn"), "{err}");
        // Metadata pass-through cannot discard producer receipts or overwrite the
        // established top-level skill-axis field with conflicting nested metadata.
        let mut row = serde_json::json!({
            "messages": [{"role": "user", "content": "q"}, {"role": "assistant", "content": "a"}],
            "metadata": {"sourceRevision": "immutable-source", "skillAxis": "nested"},
            "skillAxis": "operational",
        });
        std::fs::write(&good, serde_json::to_vec(&row).unwrap()).unwrap();
        let ds =
            TrainingDataset::from_chat_jsonl(&good, TrainingSource::TeacherSynthesized).unwrap();
        assert_eq!(
            ds.examples[0].metadata.as_ref().unwrap(),
            &serde_json::json!({
                "sourceRevision": "immutable-source", "skillAxis": "operational",
            })
        );
        row["metadata"] = serde_json::json!(["not an object"]);
        std::fs::write(&good, serde_json::to_vec(&row).unwrap()).unwrap();
        let error = TrainingDataset::from_chat_jsonl(&good, TrainingSource::TeacherSynthesized)
            .unwrap_err();
        assert!(error.contains("metadata must be an object"), "{error}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // what this catches: TrainingSource MUST be a typed enum, not a
    // free-form string. The substrate's reputation system branches on
    // the source kind; a future caller passing "raw" or "Raw" or
    // "RAW" would silently miss the routing if this were stringly
    // typed.
    #[test]
    fn training_source_serializes_snake_case() {
        let s = serde_json::to_string(&TrainingSource::TeacherSynthesized).unwrap();
        assert_eq!(s, "\"teacher_synthesized\"");
        let m = serde_json::to_string(&TrainingSource::MeshInherited).unwrap();
        assert_eq!(m, "\"mesh_inherited\"");
    }

    // what this catches: LoRA alpha-vs-rank invariant. The doctrine
    // (and the validator) say alpha should NOT equal rank — set
    // alpha = rank * 2 as the rule of thumb. This test just pins
    // the field set so a future field rename doesn't silently break
    // downstream callers.
    #[test]
    fn lora_hyperparams_camelcase() {
        let h = LoRAHyperparams {
            rank: 8,
            alpha: 16,
            dropout: 0.05,
            target_modules: vec!["q_proj".into(), "v_proj".into()],
            top_layers: None,
        };
        let v = serde_json::to_value(&h).unwrap();
        assert_eq!(v["rank"], 8);
        assert_eq!(v["alpha"], 16);
        // f32 → JSON Number → f64 round-trip drifts by < 1e-6.
        // Compare with a tolerance instead of bitwise equality.
        let dropout = v["dropout"].as_f64().expect("dropout serializes as number");
        assert!(
            (dropout - 0.05).abs() < 1e-6,
            "dropout f32→f64 drift exceeded tolerance: {dropout}"
        );
        assert_eq!(v["targetModules"][0], "q_proj");
    }
}
