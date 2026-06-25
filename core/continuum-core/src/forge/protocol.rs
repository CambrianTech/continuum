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

/// `GET /health` response — liveness + capability + contract version. A client
/// reads `contract_version` and refuses a custodian it can't speak to.
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
}

impl HealthResponse {
    /// The standard healthy reply for a gguf-lora custodian at this contract
    /// version. The custodian's `/health` handler returns exactly this.
    pub fn ok_gguf_lora() -> Self {
        Self {
            status: "ok".to_string(),
            kind: "continuum-forge-custodian".to_string(),
            capability: CAPABILITY_GGUF_LORA.to_string(),
            contract_version: CONTRACT_VERSION,
        }
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
