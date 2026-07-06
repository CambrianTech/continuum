//! `forge::protocol` — the SINGLE source of truth for the forge-custodian wire
//! contract. Both ends import these types: the custodian binary
//! (`bin/forge_custodian.rs`) that SERVES the routes, and the core-side client
//! that CALLS them. Hand-duplicating the request/response on each side (the
//! state before this module) let the two drift silently — and they already HAD:
//! the custodian required `checkpoint` in the gguf-lora body while the core's
//! copy (`inference/unsloth_forge.rs::ExportGgufLoraRequest`) omitted it and
//! carried `push_to_hub`/`repo_id` the custodian never reads. A POST from that
//! client would fail the custodian's deserialization. One type, two importers,
//! compile-time drift protection — the daemon contract the hardening pass exists
//! to establish.
//!
//! ## The contract is STATELESS
//! The request names the trained MLX checkpoint directly; there is no prior
//! `load-checkpoint` call (that was unsloth's stateful exporter shape, retired
//! with `[[unsloth-universal-model-gateway]]` / task #52). A custodian holds no
//! per-session state between requests.
//!
//! ## Contract versioning
//! [`CONTRACT_VERSION`] rides [`HealthResponse`] so a client can refuse — loudly
//! — a custodian whose contract it does not speak. Drift is then caught at the
//! handshake, not as a malformed body deep in a conversion. Bump it on ANY
//! breaking change to the request/response shapes below.
//!
//! ## Route paths
//! The route strings live here too ([`ROUTE_GGUF_LORA`], [`ROUTE_HEALTH`]) so the
//! server's route registration and the client's POST path are the SAME constant —
//! a renamed route can't silently 404 one side.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The forge-custodian wire-contract version. Bump on any breaking change to the
/// request/response shapes in this module. A client compares this against the
/// custodian's [`HealthResponse::contract_version`] and fails loud on mismatch
/// rather than POSTing a body the custodian can't parse.
pub const CONTRACT_VERSION: u32 = 1;

/// Route: convert a trained MLX checkpoint into a GGUF LoRA adapter.
pub const ROUTE_GGUF_LORA: &str = "/api/export/export/gguf-lora";
/// Route: liveness + capability + contract-version handshake.
pub const ROUTE_HEALTH: &str = "/health";

/// The custodian capability tag a client matches against before dispatching work.
/// One capability today; a custodian that grows formats appends, never renames.
pub const CAPABILITY_GGUF_LORA: &str = "gguf-lora";

/// Default bind/connect address for the forge custodian. The custodian binary
/// BINDS here (overridable via `FORGE_CUSTODIAN_ADDR`); the client CONNECTS here.
/// ONE source so the two halves can't disagree on where the custodian lives.
pub const DEFAULT_CUSTODIAN_ADDR: &str = "127.0.0.1:8899";

/// `POST /api/export/export/gguf-lora` body — the ONE shared definition.
///
/// Stateless: `checkpoint` names the trained MLX run directly (holds
/// `adapters.safetensors` + `adapter_config.json`). The custodian converts the
/// adapter → GGUF LoRA against `base_model_id` (its bundled
/// `convert_lora_to_gguf.py` needs the base architecture) and writes the bytes
/// under `save_directory` (custodian-owned output). The emitted artifact is
/// exactly what `cognition/eval`'s `gene` param pages into a live persona to
/// measure lift — this is the page-in supply contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GgufLoraRequest {
    /// The trained MLX checkpoint dir (holds `adapters.safetensors` +
    /// `adapter_config.json`). Named directly because the contract is stateless.
    pub checkpoint: String,
    /// Where to write the GGUF LoRA — custodian-owned output path.
    pub save_directory: String,
    /// The base the adapter composes onto — the converter needs its architecture.
    /// REQUIRED: a GGUF LoRA with no base is meaningless, so the invariant lives
    /// in the type, not a runtime check.
    pub base_model_id: String,
    /// GGUF adapter weight type, e.g. `"f16"`. Quantizing a small LoRA buys
    /// little, so `f16` (preserve the trained signal) is the default.
    #[serde(default = "default_outtype")]
    pub outtype: String,
}

/// The default adapter weight type — `f16` preserves the trained LoRA signal.
pub fn default_outtype() -> String {
    "f16".to_string()
}

/// The custodian export result envelope (`{success, message, details}`).
///
/// `success = false` carries the failure reason in `message`; a transport-level
/// failure (custodian unreachable) is a DIFFERENT class the client surfaces as
/// its own error, never folded into this body. `details` is the typed-output
/// escape hatch (output path, tensor count, …) until those are promoted to
/// named fields in a later contract version.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ExportResult {
    #[serde(default)]
    pub success: bool,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub details: Value,
}

// ── Genome training-lifecycle types (relocated from the retired
// inference/unsloth_forge.rs — task #52). The organism-facing contract for
// train/status/package/probe, home'd HERE beside the export contract so the ONE
// forge protocol module is the single source of truth every custodian impl
// (NativeMlxCustodian, ForgeCustodianHttp) speaks. Engine-neutral; not Unsloth-
// specific (the Unsloth wire bodies died with the module). ────────────────────

/// A training run the organism kicks off on a custodian. The recipe
/// (`ForgeRecipe`) fills these; the organism never picks the engine — the
/// custodian dispatches MLX vs CUDA from `model_name` + its own hardware.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForgeTrainRequest {
    /// Base model id/path to fine-tune (the same id the gateway serves with).
    pub model_name: String,
    /// `"lora"` (the pageable genome layer — default) | `"full"`.
    pub training_type: String,
    /// Dataset format the custodian parses, e.g. `"sharegpt"` / `"alpaca"`.
    pub format_type: String,
    /// Local dataset file paths (e.g. the ShareGPT JSONL from `dataset/from-turns`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub local_datasets: Vec<String>,
    pub num_epochs: u32,
    /// Learning rate as a STRING (e.g. `"1e-05"`) — passed through verbatim.
    pub learning_rate: String,
    pub batch_size: u32,
    pub gradient_accumulation_steps: u32,
    pub max_seq_length: u32,
    /// 4-bit base load (QLoRA) — the custodian's memory/quality knob.
    pub load_in_4bit: bool,
    /// LoRA path (vs full fine-tune). The genome layer is a LoRA.
    pub use_lora: bool,
    /// Genome knobs sent EXPLICITLY — the recipe owns them, never silent defaults.
    pub lora_r: u32,
    pub lora_alpha: u32,
    pub lora_dropout: f64,
}

/// The handle a custodian returns when a run starts — the organism keeps the
/// `job_id` to observe [`TrainStatus`]. Byte custody stays custodian-side.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TrainHandle {
    #[serde(default)]
    pub job_id: String,
    #[serde(default)]
    pub message: String,
}

/// Live training status — the inspectable progress of a run. `phase` is
/// `"idle"`/`"training"`/…; `details` carries step/loss for the metric stream.
/// Sourced from the job actor's published watch snapshot — a READ, never a poll.
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
/// names. The custodian owns the HOW; continuum declares the target form. A
/// custodian that emits a new family of layers extends THIS enum, never the call.
#[derive(Debug, Clone, PartialEq)]
pub enum GenomeFormat {
    /// The pageable LoRA genome layer (adapter as-is, PEFT/safetensors).
    Lora { base_model_id: Option<String> },
    /// A fused + quantized standalone GGUF (`quantization` e.g. `"Q4_K_M"`).
    Gguf { quantization: String },
    /// A GGUF LoRA *adapter* — the pageable gene `llama-server --lora` loads and
    /// the per-request `"lora":[{id,scale}]` dial scales. DISTINCT from [`Gguf`]
    /// (a fused standalone model): this is the adapter ALONE, the thing the genome
    /// pages in/out over a shared base — the SUPPLY side of the page-in loop
    /// (`cognition/eval`'s `gene` pages exactly this in to measure lift).
    /// `base_model_id` is REQUIRED (a GGUF LoRA with no base is meaningless — the
    /// invariant lives in the type). `outtype` = adapter weight type (`"f16"`).
    GgufLora {
        base_model_id: String,
        outtype: String,
    },
}

/// Package a trained checkpoint into a genome artifact — the ONE export surface.
/// `checkpoint`/`save_directory` are custodian-owned handles; the organism never
/// touches the bytes. `push_to_hub`/`repo_id` ride the genome-market publish path.
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

/// The LoRA catalog a custodian owns. `outputs_dir` is the byte-custody root —
/// the proof the trained bytes live custodian-side.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LoraCatalog {
    #[serde(default)]
    pub loras: Vec<Value>,
    #[serde(default)]
    pub outputs_dir: String,
}

/// What a forge custodian can do RIGHT NOW — DISCOVERED by probing, never declared
/// by config. A forge daemon routes grid forge demand against this: don't dispatch
/// to a `busy` custodian; prefer one that already `held_genes` the base; treat
/// `!reachable` as "route elsewhere or fail loud". The fabric's self-organizing
/// primitive at the forge tier (`[[model-endpoint-fabric-adapter-router]]`).
#[derive(Debug, Clone, Default, Serialize)]
pub struct ForgeCapability {
    /// The custodian answered the probe (reachable + healthy).
    pub reachable: bool,
    /// A run is in flight — a new train would queue/contend for the engine.
    pub busy: bool,
    /// The current run's phase (`"idle"`/`"training"`/…) for the fleet snapshot.
    pub phase: String,
    /// How many trained LoRA genome layers this custodian already holds.
    pub held_genes: usize,
    /// The byte-custody root (proof the bytes live custodian-side).
    pub outputs_dir: String,
}

/// `GET /health` response — liveness + capability + contract version + the
/// readiness/capacity detail a router (the model-endpoint fabric) scores against.
///
/// `status`/`kind`/`capability`/`contract_version` are the discovery basics; a
/// client reads `contract_version` and refuses a custodian it can't speak to.
/// `ready`/`slots_total`/`slots_available` are the ROUTING inputs (R4): a custodian
/// that is alive but `ready == false` (its converter tooling is missing) or has
/// `slots_available == 0` (saturated) should not be handed new work. They are
/// `#[serde(default)]` so an older custodian that omits them deserializes fine
/// (the additive, non-breaking shape — no `CONTRACT_VERSION` bump).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthResponse {
    /// `"ok"` when serving. Present for human/`curl` legibility.
    pub status: String,
    /// What this process is — `"continuum-forge-custodian"`.
    pub kind: String,
    /// What it can do RIGHT NOW (e.g. [`CAPABILITY_GGUF_LORA`]).
    pub capability: String,
    /// The wire-contract version this custodian speaks ([`CONTRACT_VERSION`]).
    pub contract_version: u32,
    /// Can it actually do work right now — i.e. its conversion tooling resolves?
    /// A live-but-not-ready custodian must not be routed to. Defaults `true` so an
    /// older custodian that predates the field reads as ready.
    #[serde(default = "default_ready")]
    pub ready: bool,
    /// Total concurrent conversion slots this custodian advertises (R3 bound).
    #[serde(default)]
    pub slots_total: u32,
    /// Slots free RIGHT NOW. `0` ⇒ saturated; route elsewhere. (`slots_total` with
    /// a `0` default just means "capacity unknown" for an older custodian.)
    #[serde(default)]
    pub slots_available: u32,
}

/// `ready` defaults to `true` for back-compat with custodians predating the field.
pub fn default_ready() -> bool {
    true
}

impl HealthResponse {
    /// The standard healthy reply for a gguf-lora custodian at this contract
    /// version, ready, with the given capacity. The custodian's `/health` handler
    /// fills `ready` + slot counts from its live state.
    pub fn gguf_lora(ready: bool, slots_total: u32, slots_available: u32) -> Self {
        Self {
            status: "ok".to_string(),
            kind: "continuum-forge-custodian".to_string(),
            capability: CAPABILITY_GGUF_LORA.to_string(),
            contract_version: CONTRACT_VERSION,
            ready,
            slots_total,
            slots_available,
        }
    }

    /// Minimal healthy reply (ready, capacity unspecified) — used by tests and any
    /// caller that only needs the discovery basics.
    pub fn ok_gguf_lora() -> Self {
        Self::gguf_lora(true, 0, 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // what this catches: the request a client SERIALIZES is exactly what the
    // server DESERIALIZES — the round-trip a hand-duplicated pair could (and did)
    // break. If a field is renamed/dropped on one side only, this fails.
    #[test]
    fn gguf_lora_request_round_trips() {
        let req = GgufLoraRequest {
            checkpoint: "/runs/coder-3b".to_string(),
            save_directory: "/genes".to_string(),
            base_model_id: "continuum-ai/qwen3.5-4b-code-forged-GGUF".to_string(),
            outtype: "f16".to_string(),
        };
        let wire = serde_json::to_value(&req).unwrap();
        let back: GgufLoraRequest = serde_json::from_value(wire).unwrap();
        assert_eq!(req, back);
    }

    // what this catches: `checkpoint` is REQUIRED on the contract (the field the
    // old core-side copy omitted). A body without it must fail to deserialize —
    // proof the stateless contract is enforced, not silently defaulted.
    #[test]
    fn request_without_checkpoint_fails_loud() {
        let err = serde_json::from_value::<GgufLoraRequest>(json!({
            "save_directory": "/genes",
            "base_model_id": "b"
        }))
        .expect_err("missing checkpoint must fail deserialization");
        assert!(err.to_string().contains("checkpoint"), "got: {err}");
    }

    // what this catches: omitted `outtype` defaults to f16 (preserve the trained
    // signal); a recipe that wants a quantized adapter sets it explicitly.
    #[test]
    fn outtype_defaults_to_f16() {
        let req: GgufLoraRequest = serde_json::from_value(json!({
            "checkpoint": "/c", "save_directory": "/s", "base_model_id": "b"
        }))
        .unwrap();
        assert_eq!(req.outtype, "f16");
    }

    // what this catches: the health handshake reports the SAME version constant
    // the client compiles against — the runtime drift check is only honest if
    // ok_gguf_lora() can't fall out of sync with CONTRACT_VERSION.
    #[test]
    fn health_reports_current_contract_version() {
        let h = HealthResponse::ok_gguf_lora();
        assert_eq!(h.contract_version, CONTRACT_VERSION);
        assert_eq!(h.capability, CAPABILITY_GGUF_LORA);
        // and it round-trips so a client can read it back.
        let wire = serde_json::to_value(&h).unwrap();
        let back: HealthResponse = serde_json::from_value(wire).unwrap();
        assert_eq!(h, back);
    }
}
