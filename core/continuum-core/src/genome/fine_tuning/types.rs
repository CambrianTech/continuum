//! Typed surfaces shared across every [`super::FineTuningAdapter`]
//! impl — cloud providers, local Candle trainer, future cross-grid
//! airc fine-tuner.
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

/// One fine-tuning job's full input. Constructed by the caller (the
/// `genome/job-create` ServiceModule once it lands as Rust, the
/// `teacher` synthesizer once arc-3 wires up), handed to an
/// [`super::FineTuningAdapter`].
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
    /// The curated dataset. Rust callers construct it directly; the
    /// `genome/job-create` wire path may instead name an on-disk dataset
    /// (`datasetName`), which the command loads into this field before the
    /// request reaches any adapter — adapters always see a populated dataset.
    #[serde(default)]
    pub dataset: TrainingDataset,
    /// The gym that MEASURES this trait — a JSONL eval-set path (the
    /// `cognition/eval` `eval_set`). The dataset and this gym are two
    /// projections of the same recipe: train on the data, measure on
    /// the gym. The automatic adoption path
    /// ([`crate::modules::training_completion_sentinel`]) passes this
    /// verbatim to `cognition/eval`; when `None` the sentinel REFUSES to
    /// adopt rather than measuring against an arbitrary default gym —
    /// a gene the substrate can't fairly measure is never paged into a
    /// live persona ([[fallbacks-are-illegal-fail-loud]]). The
    /// `cognition/eval` command keeps its own coder-eval default for
    /// manual spot-checks; that default is a command affordance, not an
    /// adoption gate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eval_set: Option<String>,
    /// LoRA-specific hyperparams (rank, alpha, dropout, target
    /// modules). `None` lets the adapter pick its provider defaults —
    /// `Some` overrides them.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lora: Option<LoRAHyperparams>,
    /// Training schedule (epochs, batch size, learning rate, sequence
    /// length). `None` lets the adapter pick provider defaults.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schedule: Option<ScheduleParams>,
    /// Where the resulting artifact should land on local disk.
    /// `None` lets the adapter pick — usually
    /// `~/.continuum/genome/<persona>/<trait_kind>/<job_uuid>.safetensors`.
    /// Provider artifacts (OpenAI's `ft:gpt-4o-mini:...`) don't have a
    /// local path; the field is `None` in their [`TrainingArtifact`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_artifact_dir: Option<PathBuf>,
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

impl Default for TrainingDataset {
    /// The empty dataset the wire path deserializes when the caller named an
    /// on-disk dataset instead of inlining examples. Never valid to TRAIN on —
    /// `genome/job-create` rejects an empty `examples` before adapter selection.
    fn default() -> Self {
        Self {
            examples: Vec::new(),
            source: TrainingSource::OperatorCurated,
            validation_split: 0.1,
        }
    }
}

impl TrainingDataset {
    /// Load a chat-format dataset (the `dataset/*` commands' `{messages:[...]}`
    /// JSONL) into training pairs: every message before the final assistant turn
    /// folds into `prompt` in render order (system first — the same text the live
    /// turn saw), the final assistant turn is `completion`. This is the
    /// dataset-by-NAME seam: `dataset/from-captures` writes the corpus,
    /// `genome/job-create` consumes it by name — the recipe stays data on disk,
    /// never a multi-megabyte example blob hand-carried through argv.
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
            examples.push(TrainingExample {
                prompt,
                completion,
                metadata: row
                    .get("skillAxis")
                    .map(|a| serde_json::json!({ "skillAxis": a })),
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
    #[ts(type = "Record<string, unknown> | undefined")]
    pub metadata: Option<serde_json::Value>,
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
    /// rank * 2`. Setting `alpha == rank` is a common mistake the
    /// `genome/job-create` validator warns on.
    pub alpha: u32,
    /// Dropout on the LoRA branch during training. Common: 0.0, 0.05,
    /// 0.1. Defaults to 0.0 for most providers.
    pub dropout: f32,
    /// Which transformer projection layers to inject LoRA into.
    /// Empty `Vec` lets the adapter pick provider defaults
    /// (usually `q_proj` + `v_proj`).
    pub target_modules: Vec<String>,
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

// ─── Handle + Status ─────────────────────────────────────────────────

/// What [`super::FineTuningAdapter::create_job`] returns. Acts as a
/// correlation token across the substrate side (`local_id`) and the
/// provider side (`provider_job_id`).
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/fine_tuning/JobHandle.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct JobHandle {
    /// Which adapter created this — used by the registry to look the
    /// adapter back up on `poll` / `cancel`.
    pub provider_id: String,
    /// Provider-side identifier (OpenAI's `ftjob-abc123`, Mistral's
    /// `job-xyz`, etc). For [`super::FineTuningAdapter`] impls that
    /// run in-process (local Candle), this echoes `local_id` as a
    /// string.
    pub provider_job_id: String,
    /// Substrate-side correlation id. Stable; safe to persist; used
    /// by telemetry / alloy lineage.
    #[ts(type = "string")]
    pub local_id: Uuid,
}

/// Current state of a training job. Returned by
/// [`super::FineTuningAdapter::poll`].
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/fine_tuning/TrainingStatus.ts"
)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum TrainingStatus {
    /// Job accepted; not yet started running.
    Queued,
    /// Running. `progress_pct` is best-effort; some providers report
    /// nothing and the adapter floors it at the epoch percentage.
    #[serde(rename_all = "camelCase")]
    Running {
        progress_pct: f32,
        current_epoch: u32,
    },
    /// Terminal success. `artifact` is what genome paging /
    /// forge-alloy consume.
    Completed { artifact: TrainingArtifact },
    /// Terminal failure. `error` is the typed surface; the substrate
    /// branches on it for retry vs surface-to-operator.
    Failed { error: String },
    /// Terminal — operator-initiated stop, or provider-side abort.
    Cancelled,
}

// ─── Artifact ────────────────────────────────────────────────────────

/// The on-disk SHAPE of a completed training artifact — the load-bearing
/// fact the completion sentinel keys its supply step on. A persona pages in a
/// GGUF-lora; everything else is an intermediate that must be CONVERTED first
/// (the custodian's job). Declaring the format on the artifact (rather than
/// sniffing the provider id — that's tracked smell #70) lets the sentinel ask
/// "is this pageable, or does it need a convert dispatch?" without knowing
/// which trainer produced it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/fine_tuning/ArtifactFormat.ts"
)]
#[serde(rename_all = "kebab-case")]
pub enum ArtifactFormat {
    /// Apple `mlx_lm.lora` output dir (`adapters.safetensors` +
    /// `adapter_config.json`). NOT directly pageable — the forge custodian
    /// converts it to a GGUF-lora gene (locally today, on a grid GPU node
    /// tomorrow, same `ForgeCustodian` trait) before eval/page-in.
    MlxAdapterDir,
    /// A pageable GGUF-lora — `llama-server --lora` (and the genome page-in)
    /// loads it directly. No convert step.
    GgufLora,
    /// Candle synthetic-base LoRA safetensors (the `local-candle` skeleton,
    /// tasks #231-#233). Not yet a loadable gene against a real base.
    CandleSafetensors,
    /// Provider-hosted (OpenAI etc.) — no local weights kept; the inference
    /// adapter pulls on demand. No local convert. The conservative default
    /// for an unmarked artifact: never silently treated as a local pageable gene.
    #[default]
    ProviderHosted,
}

/// What a successful training run produces. Flows directly into forge / alloy
/// for signing + provenance, and into genome paging once a persona requests the
/// corresponding skill. Its [`ArtifactFormat`] tells the completion sentinel
/// whether the artifact is a pageable gene or needs a custodian convert first.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/fine_tuning/TrainingArtifact.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct TrainingArtifact {
    /// Provider-side model identifier
    /// (OpenAI: `ft:gpt-4o-mini-2024-07-18:org:trait:abc`, Mistral:
    /// `ft:mistral-large-latest:...`, local Candle: substrate-chosen
    /// safetensors filename).
    pub model_id: String,
    /// Local path to the downloaded weights, if any. `None` when the
    /// artifact lives provider-side and we don't keep a local copy
    /// (the inference adapter pulls it on demand).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_path: Option<PathBuf>,
    /// The artifact's on-disk shape. Drives whether the completion sentinel
    /// dispatches a custodian convert before eval/page-in. Defaults to
    /// `ProviderHosted` for backward-compatible deserialization of artifacts
    /// persisted before this field existed.
    #[serde(default)]
    pub format: ArtifactFormat,
    pub metrics: JobMetrics,
}

/// Per-job observed metrics. Substrate-side telemetry +
/// reputation signal input
/// ([[forge-alloy-secures-commodity-zero-trust-plus-reputation]]).
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/fine_tuning/JobMetrics.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct JobMetrics {
    #[ts(type = "number")]
    pub trained_tokens: u64,
    /// Validation loss at the end of training. Lower is better; the
    /// substrate uses this to decide if the layer is worth shipping
    /// into the mesh.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub final_loss: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub final_validation_loss: Option<f64>,
    #[ts(type = "number")]
    pub wall_clock_ms: u64,
    /// Trillion-token-cost USD reported by the provider, if it tells
    /// us. Helps the substrate's economic dispatcher
    /// ([[forge-alloy-secures-commodity-zero-trust-plus-reputation]])
    /// pick the cheapest viable provider per request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
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

    // what this catches: TrainingStatus is the typed return of
    // poll(). A future change to the variant set (e.g. adding
    // `Paused`) without updating downstream match arms must produce
    // a compile error, not a runtime mismatch. This test pins the
    // current variant set + its tagged JSON shape.
    #[test]
    fn training_status_serializes_tagged_snake_case() {
        let q = serde_json::to_value(TrainingStatus::Queued).unwrap();
        assert_eq!(q["state"], "queued");

        let r = serde_json::to_value(TrainingStatus::Running {
            progress_pct: 50.0,
            current_epoch: 2,
        })
        .unwrap();
        assert_eq!(r["state"], "running");
        assert_eq!(r["progressPct"], 50.0);
        assert_eq!(r["currentEpoch"], 2);

        let f = serde_json::to_value(TrainingStatus::Failed {
            error: "oom".into(),
        })
        .unwrap();
        assert_eq!(f["state"], "failed");
        assert_eq!(f["error"], "oom");
    }

    // what this catches: JobHandle.local_id must round-trip as a
    // UUID, not a stringly-typed counter. Substrate-side telemetry
    // joins on this id; a future caller serializing as `usize` would
    // break the join silently.
    #[test]
    fn job_handle_local_id_is_uuid() {
        let h = JobHandle {
            provider_id: "openai".into(),
            provider_job_id: "ftjob-abc".into(),
            local_id: Uuid::nil(),
        };
        let v = serde_json::to_value(&h).unwrap();
        // ts-rs camelCases — verify the wire shape downstream tooling
        // will see.
        assert_eq!(v["providerId"], "openai");
        assert_eq!(v["providerJobId"], "ftjob-abc");
        assert!(v["localId"].is_string());
        assert_eq!(
            v["localId"].as_str().unwrap(),
            "00000000-0000-0000-0000-000000000000"
        );
    }

    // what this catches: LoRA alpha-vs-rank invariant. The doctrine
    // (and the validator) say alpha should NOT equal rank — set
    // alpha = rank * 2 as the rule of thumb. This test just pins
    // the field set so a future field rename doesn't silently break
    // downstream callers; the semantic check lives in the
    // genome/job-create validator.
    #[test]
    fn lora_hyperparams_camelcase() {
        let h = LoRAHyperparams {
            rank: 8,
            alpha: 16,
            dropout: 0.05,
            target_modules: vec!["q_proj".into(), "v_proj".into()],
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
