//! Unsloth FORGE custodian proxy (#32) — the train / export / LoRA-catalog
//! surface, delegated over the custodian's HTTP management API.
//!
//! ## The domain cut this enforces
//! **unsloth = the model CUSTODIAN**: it owns the bytes (trained LoRAs live under
//! `~/.unsloth/studio/outputs`, verified via `GET /api/models/loras` →
//! `{"loras":[…],"outputs_dir":"…/.unsloth/studio/outputs"}`), the training
//! toolchain (it dispatches MLX on Apple Silicon / Unsloth-CUDA on NVIDIA
//! INTERNALLY — the caller never picks the engine), and the fuse/quant/GGUF
//! conversion. **continuum = the organism**: it holds HANDLES (a run id, a LoRA
//! path) and owns POLICY (`forge/decide` — adopt vs. train vs. shop the market).
//!
//! `ai/forge.rs` today TRESPASSES across that cut: it spawns `mlx_lm.lora` /
//! `mlx_lm.fuse` / llama.cpp itself and writes bytes under `~/.continuum/forge`.
//! That is custodian work living in the organism — the GGUF-gibberish + adapter
//! no-op bug class (`[[mlx-serving-gguf-breaks-hybrid-adapter-path-noop]]`). The
//! custodian ALREADY exposes the whole surface over HTTP (verified live against
//! Studio :8888, 201 routes incl. `/api/train/*`, `/api/export/*`,
//! `/api/models/loras`), so the repair is to DELETE the subprocess + byte-write
//! path in forge.rs and DELEGATE here — exactly as [`super::unsloth_control`]
//! delegates model load/unload. (`[[model-endpoint-fabric-adapter-router]]`,
//! `[[unsloth-universal-model-gateway]]`.)
//!
//! ## Why a trait (TDD), mirroring `unsloth_control`
//! The request SHAPE (what the custodian's body must contain) and the response
//! PARSE (the status fields the organism depends on) are the parts that must be
//! correct; they're tested apart from the network behind [`ForgeCustodian`].
//! [`UnslothForgeHttp`] is the real reqwest impl, sharing the ONE endpoint
//! accessor ([`super::unsloth_control::unsloth_base_url`]) and the same bearer
//! auth — the custodian endpoint lives in one place.
//!
//! ## Degrade-vs-fail-loud
//! These are organism ACTIONS (train this, export that), not a hot-path
//! pre-flight, so an unreachable custodian is a LOUD [`UnslothError`] the caller
//! surfaces with its cause — never a silent no-op (`[[fallbacks-are-illegal-fail-loud]]`).

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::unsloth_control::{unsloth_base_url, UnslothError};
use crate::config_env;

/// A training run the organism kicks off on the custodian. Fields map 1:1 to
/// `POST /api/train/start` (snake_case, so serde field names match the wire).
/// The recipe (`ForgeRecipe`) fills these; the organism never picks the engine —
/// the custodian dispatches MLX vs. CUDA from `model_name` + its own hardware.
#[derive(Debug, Clone, Serialize)]
pub struct ForgeTrainRequest {
    /// Base model id/path to fine-tune (the same id the gateway serves with).
    pub model_name: String,
    /// `"lora"` (the pageable genome layer — default) | `"full"`.
    pub training_type: String,
    /// Dataset format the custodian parses, e.g. `"sharegpt"` / `"alpaca"`.
    pub format_type: String,
    /// Local dataset file paths (the custodian reads them; e.g. the ShareGPT
    /// JSONL produced by `dataset/from-turns`). Omitted when empty.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub local_datasets: Vec<String>,
    pub num_epochs: u32,
    /// The custodian's API takes the learning rate as a STRING (e.g. `"1e-05"`);
    /// we pass the recipe value through verbatim.
    pub learning_rate: String,
    pub batch_size: u32,
    pub gradient_accumulation_steps: u32,
    pub max_seq_length: u32,
    /// 4-bit base load (QLoRA) — the custodian's memory/quality knob.
    pub load_in_4bit: bool,
    /// LoRA path (vs. full fine-tune). The genome layer is a LoRA.
    pub use_lora: bool,
    /// The genome knobs. We send rank/alpha EXPLICITLY — never let the recipe's
    /// values fall silently to the custodian's defaults (the old
    /// `write_mlx_lora_config` fail-loud-over-silent-substitution rule, now
    /// enforced on the wire). The API defaults happen to match (16/16) but the
    /// recipe owns them.
    pub lora_r: u32,
    pub lora_alpha: u32,
    pub lora_dropout: f64,
}

/// The handle returned by `POST /api/train/start` — the organism keeps the
/// `job_id` to poll [`ForgeCustodian::train_status`]. The byte custody stays
/// custodian-side; this is the reference.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TrainHandle {
    #[serde(default)]
    pub job_id: String,
    #[serde(default)]
    pub message: String,
}

/// Live training status — the inspectable progress of a run
/// (`GET /api/train/status`). `phase` is `"idle"`/`"training"`/…; `details`
/// carries step/loss for the metric stream.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TrainStatus {
    #[serde(default)]
    pub job_id: String,
    #[serde(default)]
    pub phase: String,
    #[serde(default)]
    pub is_training_running: bool,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub details: TrainProgress,
}

/// The numeric progress inside a run (`status.details`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TrainProgress {
    #[serde(default)]
    pub epoch: f64,
    #[serde(default)]
    pub step: u64,
    #[serde(default)]
    pub total_steps: u64,
    #[serde(default)]
    pub loss: Option<f64>,
}

/// What to package a trained checkpoint INTO — the genetic OUTCOME the organism
/// names. The custodian owns the HOW (unsloth fuses/converts/quantizes
/// internally); continuum only declares the target form of the genome artifact.
/// A future custodian that emits a different family of layers extends THIS enum,
/// never the organism's call shape.
#[derive(Debug, Clone, PartialEq)]
pub enum GenomeFormat {
    /// The pageable LoRA genome layer (the adapter as-is). `base_model_id` rides
    /// the genome-market card so the layer knows what base it composes onto.
    Lora { base_model_id: Option<String> },
    /// A fused + quantized standalone GGUF (`quantization` e.g. `"Q4_K_M"`).
    Gguf { quantization: String },
}

/// Package a trained checkpoint into a genome artifact — the ONE export surface.
/// `checkpoint` and `save_directory` are BOTH custodian-owned handles (paths
/// under `~/.unsloth`); the organism never touches the bytes. `push_to_hub` /
/// `repo_id` ride the genome-market publish path. The custodian's internal
/// load → fuse → convert → quantize sequence is invisible above this struct —
/// that is the point (Joel: "more of a black box with an adapter our only
/// visibility").
#[derive(Debug, Clone)]
pub struct PackageRequest {
    pub checkpoint: String,
    pub save_directory: String,
    pub format: GenomeFormat,
    pub max_seq_length: u32,
    pub load_in_4bit: bool,
    pub push_to_hub: bool,
    pub repo_id: Option<String>,
}

// ── Internal unsloth wire bodies (BELOW the trait seam) ─────────────────────
// These map 1:1 to the custodian's `/api/export/*` routes. They are an
// IMPLEMENTATION DETAIL of `UnslothForgeHttp::package`, not the organism-facing
// surface — the organism speaks `PackageRequest`/`GenomeFormat` only.

/// `POST /api/export/load-checkpoint` body — unsloth loads a trained run into its
/// exporter before packaging (its export endpoints operate on the loaded
/// checkpoint, not a path arg). `package` runs this first, internally.
#[derive(Debug, Clone, Serialize)]
struct LoadCheckpointRequest {
    checkpoint_path: String,
    max_seq_length: u32,
    load_in_4bit: bool,
}

/// `POST /api/export/export/lora` body.
#[derive(Debug, Clone, Serialize)]
struct ExportLoraRequest {
    save_directory: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    push_to_hub: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    repo_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    base_model_id: Option<String>,
}

/// `POST /api/export/export/gguf` body — the custodian fuses + converts +
/// quantizes (it owns the toolchain that knows the architecture; this is why
/// continuum must NOT hand-run `convert_lora_to_gguf.py`).
#[derive(Debug, Clone, Serialize)]
struct ExportGgufRequest {
    save_directory: String,
    /// e.g. `"q4_k_m"`, `"q8_0"`, `"f16"`.
    quantization_method: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    push_to_hub: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    repo_id: Option<String>,
}

/// The custodian's export result envelope (`{success, message, details}`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExportResult {
    #[serde(default)]
    pub success: bool,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub details: serde_json::Value,
}

/// The LoRA catalog the custodian owns (`GET /api/models/loras`). `outputs_dir`
/// is the byte-custody root (verified `~/.unsloth/studio/outputs`) — the proof
/// the bytes live custodian-side, not under `~/.continuum/forge`. Per-entry
/// typing follows the first real trained LoRA (the live list is empty today);
/// the load-bearing fact captured now is the custody root + the handles list.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct LoraCatalog {
    #[serde(default)]
    pub loras: Vec<serde_json::Value>,
    #[serde(default)]
    pub outputs_dir: String,
}

/// The custodian's forge surface the organism delegates to. Behind a trait so
/// request-building + response-parsing are TDD-tested apart from the HTTP I/O,
/// matching [`super::unsloth_control::UnslothControl`].
#[async_trait]
pub trait ForgeCustodian: Send + Sync {
    /// Kick off a training run on the custodian (`POST /api/train/start`). The
    /// custodian picks the engine (MLX/CUDA) and owns the output bytes; we get a
    /// [`TrainHandle`] to poll.
    async fn train_start(&self, req: &ForgeTrainRequest) -> Result<TrainHandle, UnslothError>;
    /// Poll the live run (`GET /api/train/status`).
    async fn train_status(&self) -> Result<TrainStatus, UnslothError>;
    /// Package a trained checkpoint into a genome artifact (LoRA or GGUF). The
    /// custodian owns the HOW — unsloth's load-checkpoint → fuse → convert →
    /// quantize sequence is an implementation detail BELOW this seam, so a future
    /// custodian that exports in one shot, or emits a different layer family,
    /// drops in without touching the organism. (Joel: "more of a black box with
    /// an adapter our only visibility.") Surfaces a failed package as a LOUD
    /// [`UnslothError`], never a silent no-op (the bug class #32 was opened to
    /// kill).
    async fn package(&self, req: &PackageRequest) -> Result<ExportResult, UnslothError>;
    /// The trained-LoRA catalog the custodian holds (`GET /api/models/loras`).
    async fn list_loras(&self) -> Result<LoraCatalog, UnslothError>;
}

/// Real reqwest impl over the custodian's `/api/{train,export,models}/*` surface.
/// Reuses the single endpoint accessor + bearer auth from `unsloth_control` so
/// the custodian host lives in exactly one place.
pub struct UnslothForgeHttp {
    host: String,
    api_key: Option<String>,
    client: reqwest::Client,
}

impl UnslothForgeHttp {
    /// Build from config (host via [`unsloth_base_url`], key via the one config
    /// owner). Fresh pooled client; cheap to clone if a caller wants to share.
    pub fn from_config() -> Self {
        Self::with_client(reqwest::Client::new())
    }

    /// Reuse an existing pooled `reqwest::Client` (shares the connection pool).
    pub fn with_client(client: reqwest::Client) -> Self {
        Self {
            host: unsloth_base_url(),
            api_key: config_env::read("UNSLOTH_API_KEY"),
            client,
        }
    }

    /// Load a trained run into unsloth's exporter — a private wire step BELOW the
    /// trait. unsloth's `/api/export/export/*` routes act on the loaded
    /// checkpoint, so `package` runs this first; the organism never sees it.
    async fn load_checkpoint(
        &self,
        req: &LoadCheckpointRequest,
    ) -> Result<ExportResult, UnslothError> {
        self.post_json("/api/export/load-checkpoint", serde_json::to_value(req).unwrap())
            .await
    }

    fn authed(&self, rb: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.api_key {
            Some(key) => rb.header("Authorization", format!("Bearer {key}")),
            None => rb,
        }
    }

    /// POST `body` to `{host}{path}` and deserialize the JSON response into `T`.
    /// Transport error → `Unreachable`; non-2xx (with body) or bad JSON → `Api`.
    async fn post_json<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: serde_json::Value,
    ) -> Result<T, UnslothError> {
        let url = format!("{}{}", self.host, path);
        let resp = self
            .authed(self.client.post(&url))
            .json(&body)
            .send()
            .await
            .map_err(|e| UnslothError::Unreachable(e.to_string()))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(UnslothError::Api(format!("{path} {status}: {text}")));
        }
        resp.json::<T>()
            .await
            .map_err(|e| UnslothError::Api(format!("{path}: decode {e}")))
    }

    /// GET `{host}{path}` and deserialize into `T`.
    async fn get_json<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T, UnslothError> {
        let url = format!("{}{}", self.host, path);
        let resp = self
            .authed(self.client.get(&url))
            .send()
            .await
            .map_err(|e| UnslothError::Unreachable(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(UnslothError::Api(format!("{path} {}", resp.status())));
        }
        resp.json::<T>()
            .await
            .map_err(|e| UnslothError::Api(format!("{path}: decode {e}")))
    }
}

#[async_trait]
impl ForgeCustodian for UnslothForgeHttp {
    async fn train_start(&self, req: &ForgeTrainRequest) -> Result<TrainHandle, UnslothError> {
        self.post_json("/api/train/start", serde_json::to_value(req).unwrap())
            .await
    }
    async fn train_status(&self) -> Result<TrainStatus, UnslothError> {
        self.get_json("/api/train/status").await
    }
    async fn package(&self, req: &PackageRequest) -> Result<ExportResult, UnslothError> {
        // unsloth's export ops act on a LOADED checkpoint — load it first. This
        // two-step is an unsloth-ism kept BELOW the trait so it never leaks to the
        // organism; a one-shot custodian would just skip it.
        let loaded = self
            .load_checkpoint(&LoadCheckpointRequest {
                checkpoint_path: req.checkpoint.clone(),
                max_seq_length: req.max_seq_length,
                load_in_4bit: req.load_in_4bit,
            })
            .await?;
        if !loaded.success {
            return Err(UnslothError::Api(format!(
                "load-checkpoint failed for {}: {}",
                req.checkpoint, loaded.message
            )));
        }
        match &req.format {
            GenomeFormat::Lora { base_model_id } => {
                let body = ExportLoraRequest {
                    save_directory: req.save_directory.clone(),
                    push_to_hub: req.push_to_hub,
                    repo_id: req.repo_id.clone(),
                    base_model_id: base_model_id.clone(),
                };
                self.post_json("/api/export/export/lora", serde_json::to_value(body).unwrap())
                    .await
            }
            GenomeFormat::Gguf { quantization } => {
                let body = ExportGgufRequest {
                    save_directory: req.save_directory.clone(),
                    quantization_method: quantization.clone(),
                    push_to_hub: req.push_to_hub,
                    repo_id: req.repo_id.clone(),
                };
                self.post_json("/api/export/export/gguf", serde_json::to_value(body).unwrap())
                    .await
            }
        }
    }
    async fn list_loras(&self) -> Result<LoraCatalog, UnslothError> {
        self.get_json("/api/models/loras").await
    }
}

/// Convenience for `serde_json::to_value` on a request without unwrapping at the
/// call site (kept for callers building bodies ad hoc).
pub fn to_body<T: Serialize>(req: &T) -> serde_json::Value {
    serde_json::to_value(req).unwrap_or_else(|_| json!({}))
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the train request must serialize to the custodian's
    // EXACT body contract (snake_case keys, learning_rate as a STRING, datasets
    // under local_datasets). Regression here = /api/train/start 422s and the
    // whole genome loop silently can't kick off a run.
    #[test]
    fn train_request_serializes_to_custodian_body() {
        let req = ForgeTrainRequest {
            model_name: "unsloth/Qwen3.5-4B".into(),
            training_type: "lora".into(),
            format_type: "sharegpt".into(),
            local_datasets: vec!["/data/coder.jsonl".into()],
            num_epochs: 3,
            learning_rate: "1e-05".into(),
            batch_size: 2,
            gradient_accumulation_steps: 1,
            max_seq_length: 2048,
            load_in_4bit: true,
            use_lora: true,
            lora_r: 32,
            lora_alpha: 64,
            lora_dropout: 0.0,
        };
        let v = to_body(&req);
        assert_eq!(v["model_name"], "unsloth/Qwen3.5-4B");
        assert_eq!(v["training_type"], "lora");
        assert_eq!(v["format_type"], "sharegpt");
        assert_eq!(v["local_datasets"][0], "/data/coder.jsonl");
        // learning_rate is a STRING on the wire, not a float.
        assert!(v["learning_rate"].is_string(), "learning_rate must serialize as string");
        assert_eq!(v["learning_rate"], "1e-05");
        // the genome knobs ride explicitly — never silently the custodian's default.
        assert_eq!(v["lora_r"], 32);
        assert_eq!(v["lora_alpha"], 64);
        assert_eq!(v["use_lora"], true);
    }

    // what this catches: an empty dataset list must be OMITTED, not sent as `[]`
    // — the custodian distinguishes "use hf_dataset" from "empty local set".
    #[test]
    fn empty_local_datasets_is_omitted() {
        let req = ForgeTrainRequest {
            model_name: "m".into(),
            training_type: "lora".into(),
            format_type: "sharegpt".into(),
            local_datasets: vec![],
            num_epochs: 1,
            learning_rate: "1e-05".into(),
            batch_size: 1,
            gradient_accumulation_steps: 1,
            max_seq_length: 1024,
            load_in_4bit: false,
            use_lora: true,
            lora_r: 16,
            lora_alpha: 16,
            lora_dropout: 0.0,
        };
        let v = to_body(&req);
        assert!(v.get("local_datasets").is_none(), "empty datasets must be omitted");
    }

    // what this catches: the GGUF export request carries the quantization method
    // + save_directory (custodian-owned) and omits push_to_hub when false — the
    // custodian owns the conversion, so this body is the whole organism→custodian
    // contract for it.
    #[test]
    fn gguf_export_request_shape() {
        let req = ExportGgufRequest {
            save_directory: "/Users/x/.unsloth/studio/outputs/run1".into(),
            quantization_method: "q4_k_m".into(),
            push_to_hub: false,
            repo_id: None,
        };
        let v = to_body(&req);
        assert_eq!(v["quantization_method"], "q4_k_m");
        assert!(v["save_directory"].as_str().unwrap().contains(".unsloth"));
        assert!(v.get("push_to_hub").is_none(), "push_to_hub:false must be omitted");
        assert!(v.get("repo_id").is_none());
    }

    // what this catches: the live train-status shape we depend on parses (the
    // exact JSON the custodian returned on 2026-06-24). Regression = a status
    // field rename silently zeroes our progress/idle detection.
    #[test]
    fn train_status_parses_live_idle_shape() {
        let body = r#"{"job_id":"","phase":"idle","is_training_running":false,
            "eval_enabled":false,"message":"Ready to train","error":null,
            "details":{"epoch":0,"step":0,"total_steps":0,"loss":null,
            "learning_rate":null},"metric_history":null}"#;
        let s: TrainStatus = serde_json::from_str(body).unwrap();
        assert_eq!(s.phase, "idle");
        assert!(!s.is_training_running);
        assert_eq!(s.message, "Ready to train");
        assert_eq!(s.details.total_steps, 0);
        assert!(s.error.is_none());
    }

    // what this catches: the LoRA catalog parses AND surfaces the custody root —
    // proof the bytes live under ~/.unsloth (the domain cut), not ~/.continuum.
    // Regression = forge.rs silently re-points byte custody into the organism.
    #[test]
    fn lora_catalog_parses_outputs_dir() {
        let body = r#"{"loras":[],"outputs_dir":"/Users/joel/.unsloth/studio/outputs"}"#;
        let c: LoraCatalog = serde_json::from_str(body).unwrap();
        assert!(c.loras.is_empty());
        assert!(
            c.outputs_dir.contains(".unsloth"),
            "custody root must be under ~/.unsloth, not the organism's ~/.continuum"
        );
    }

    // what this catches: the export result envelope parses success+message+details.
    #[test]
    fn export_result_parses() {
        let body = r#"{"success":true,"message":"exported","details":{"path":"/x/lora"}}"#;
        let r: ExportResult = serde_json::from_str(body).unwrap();
        assert!(r.success);
        assert_eq!(r.message, "exported");
        assert_eq!(r.details["path"], "/x/lora");
    }
}
