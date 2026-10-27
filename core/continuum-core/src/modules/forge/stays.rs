use super::*;

/// `forge/export` for the `gguf-lora` outcome — served by the continuum forge
/// CUSTODIAN over Contract C ([`crate::forge::protocol`]).
///
/// This is the corrected supply path for the genome page-in: it serializes the
/// STATELESS [`GgufLoraRequest`](crate::forge::protocol::GgufLoraRequest)
/// (checkpoint-in-body, no prior load-checkpoint, no hub fields), verifies the
/// custodian speaks our contract version at the `/health` handshake, and fails
/// LOUD on a missing base, an unreachable custodian, or a non-success envelope.
/// The emitted gene is exactly what `cognition/eval` pages into a live persona to
/// measure LIFT.
/// The base-id split (the L3 trinity bridge) lives at the call site: `p.base_model_id`
/// is the CANONICAL registry id (what serving/eval resolve to a GGUF, and what the
/// manifest records so `cognition/eval` can find the base lane), while `hf_base` is
/// that id resolved to its SAFETENSORS repo (`Model::hf_source`) — the only thing the
/// custodian's HF→PEFT→GGUF convert can consume. Resolution is the in-core handler's
/// job (it owns the live registry); this fn stays registry-free so it is unit-testable
/// by passing both ids explicitly. The custodian is a separate process and stays dumb,
/// receiving a pre-resolved HF id.
pub(super) async fn run_export_gguf_lora(
    custodian: &dyn crate::forge::custodian_client::ForgeCustodian,
    p: &ForgeExportParams,
    hf_base: &str,
    manifest_path: &std::path::Path,
) -> Result<CommandResult, String> {
    // A GGUF LoRA with no base is meaningless — the converter needs the base
    // architecture. Reject loudly at the boundary, never silently default. This is
    // the CANONICAL id the manifest is keyed on (eval filters genes by it).
    let base_model_id = p.base_model_id.clone().ok_or_else(|| {
        "format 'gguf-lora' requires base_model_id — the converter needs the base \
         architecture to produce a loadable adapter"
            .to_string()
    })?;

    // Self-provision the LOCAL custodian if it isn't up (managed-product: the
    // organism brings its own converter — no operator step). This single seam
    // covers BOTH the L3 sentinel's automatic export dispatch AND a manual
    // `forge/export`, so the genome loop closes without a hand-started sidecar
    // ([[managed-product-everything-self-provisions-no-operator-steps]], glass-boxed
    // 2026-07-20: the sentinel fired but died at "custodian unreachable"). A grid
    // custodian is never spawned here; the local-http path is the only caller.
    crate::forge::custodian_supervisor::ensure_local_custodian(custodian)
        .await
        .map_err(|e| format!("forge/export: {e}"))?;

    // Catch contract drift at the handshake, not as a malformed body deep in a
    // conversion (Contract C, R1/R2).
    custodian
        .ensure_contract()
        .await
        .map_err(|e| e.to_string())?;

    let req = crate::forge::protocol::GgufLoraRequest {
        checkpoint_format: p.checkpoint_format,
        checkpoint: p.checkpoint.clone(),
        save_directory: p.save_directory.clone(),
        base_model_id: hf_base.to_string(),
        outtype: p.outtype.clone(),
    };
    let result = custodian
        .export_gguf_lora(&req)
        .await
        .map_err(|e| e.to_string())?;

    if !result.success {
        return Err(format!(
            "custodian export (gguf-lora) failed: {}",
            result.message
        ));
    }

    // Close the 5th wire: a produced gene that isn't REGISTERED is a silently-lost
    // gene — the serving daemon reads this manifest at (re)spawn to populate
    // `llama-server --lora`, so without this the catalog stays empty and every
    // page-in fails (LIFT structurally unreachable). The producer is the only
    // place that knows BOTH the on-disk path AND the continuum base_model_id the
    // serving daemon filters on (genome::adapter_manifest docs). Fail LOUD if the
    // gene can't be registered — never return success for an unloadable gene.
    let adapter = trained_adapter_from_export(&result.details, &base_model_id)?;
    crate::genome::adapter_manifest::register_at(manifest_path, adapter.clone())
        .map_err(|e| format!("gene exported but manifest registration failed: {e}"))?;

    Ok(CommandResult::Json(json!({
        "format": "gguf-lora",
        "checkpoint": p.checkpoint,
        "save_directory": p.save_directory,
        "message": result.message,
        "details": result.details,
        "registered": { "alias": adapter.alias, "path": adapter.path, "base_model_id": adapter.base_model_id },
    })))
}

/// Params for `forge/publish` (#99 L4). Camel-case on the wire. Validation +
/// the lift gate live in `PublishRequest::build`, so this struct is just the raw
/// facts a completed forge run knows about the layer.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ForgePublishParams {
    /// Target repo (`namespace/name`), e.g. `continuum-ai/devstral-code-asha`.
    pub(super) repo_id: String,
    /// Local gguf-lora gene file to upload.
    pub(super) gene_path: String,
    pub(super) base_model: String,
    pub(super) trait_kind: String,
    #[serde(default)]
    pub(super) persona_name: Option<String>,
    #[serde(default)]
    pub(super) project_type: Option<String>,
    #[serde(default)]
    pub(super) score: Option<i64>,
    #[serde(default)]
    pub(super) epochs: Option<i64>,
    #[serde(default)]
    pub(super) rank: Option<i64>,
    /// Held-out lift as a fraction (0.051 = +5.1pts). Gate is `> 0`.
    pub(super) lift: f64,
    /// Which publisher to use: `"huggingface"` (default) or `"github"`.
    #[serde(default)]
    pub(super) target: Option<String>,
}

/// Validate + gate the layer, select a Publisher adapter by target, upload, and
/// return the receipt. Owner-gated at the ACL (network publish is consent-gated);
/// everything malformed/unmeasured/regressing is refused inside
/// `PublishRequest::build` before any transport is touched.
pub(super) async fn run_publish(p: ForgePublishParams) -> Result<CommandResult, String> {
    use crate::forge::publish_request::{PublishInputs, PublishRequest};

    let inputs = PublishInputs {
        repo_id: p.repo_id,
        gene_path: std::path::PathBuf::from(p.gene_path),
        base_model: p.base_model,
        trait_kind: p.trait_kind,
        persona_name: p.persona_name,
        project_type: p.project_type,
        score: p.score,
        epochs: p.epochs,
        rank: p.rank,
        lift: p.lift,
        // forge/publish (this legacy verb) predates gene signatures; genome/push
        // is the signature-carrying path.
        signature_json: None,
        provenance_json: None, // operator/CLI publish path: no citizen key in scope — unsigned (commons ranks untrusted)
        parent_alloy_hashes: Vec::new(),
    };
    let req = PublishRequest::build(&inputs, |path| path.exists())
        .map_err(|e| format!("forge/publish: {e}"))?;

    // one dispatcher, one staged bundle, proven by read-back (forge::publisher)
    let target = p.target.clone().unwrap_or_else(|| "huggingface".to_string());
    let (bundle, mut results) = crate::forge::publisher::publish_everywhere(&req, std::slice::from_ref(&target))
        .await
        .map_err(|e| format!("forge/publish: {e}"))?;
    let (_, outcome) = results.pop().ok_or("forge/publish: no target was attempted")?;
    let receipt = outcome.map_err(|e| format!("forge/publish: {e}"))?;
    Ok(CommandResult::Json(serde_json::json!({
        "transport": receipt.transport,
        "location": receipt.location,
        "digest": bundle.digest,
        "liftPct": req.lift_pct,
        "tags": req.tags,
    })))
}

/// Project the custodian's success `details` + the continuum `base_model_id` into a
/// [`TrainedAdapter`](crate::genome::adapter_manifest::TrainedAdapter) ready to
/// register. Pure (no IO) so the extraction is unit-testable without a custodian.
/// Fail LOUD if `details.output` is absent/empty — the custodian ALWAYS reports the
/// produced path on success (forge_custodian.rs), so a missing one is a contract
/// breach, not a gene we silently drop. Alias is the gene's file stem (logs only;
/// the per-request page-in resolver matches on `path`, never the alias).
pub(super) fn trained_adapter_from_export(
    details: &Value,
    base_model_id: &str,
) -> Result<crate::genome::adapter_manifest::TrainedAdapter, String> {
    let output = details
        .get("output")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            "custodian export succeeded but `details.output` (the gene path) is \
             missing — cannot register an unlocatable gene"
                .to_string()
        })?;
    let path = std::path::PathBuf::from(output);
    let alias = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "gene".to_string());
    Ok(crate::genome::adapter_manifest::TrainedAdapter {
        alias,
        path,
        base_model_id: base_model_id.to_string(),
    })
}

/// `forge/health` — surface the local custodian's Contract C
/// [`HealthResponse`](crate::forge::protocol::HealthResponse) as a command result.
/// Taken over the trait (not the concrete HTTP client) so it is unit-testable with
/// a fake custodian. Fail-loud: an unreachable custodian surfaces as an error, not
/// a fabricated "healthy" — the honest reading is the whole point of the probe (R4).
pub(super) async fn run_health(
    custodian: &dyn crate::forge::custodian_client::ForgeCustodian,
) -> Result<CommandResult, String> {
    let health = custodian.health().await.map_err(|e| e.to_string())?;
    let json = serde_json::to_value(health).map_err(|e| format!("forge/health: serialize: {e}"))?;
    Ok(CommandResult::Json(json))
}

//=============================================================================
// forge/decide — "assemble the best self, or train" (the product-thesis decision)
//
// The request has been scored against the trust-scoped market: each candidate
// (an existing model / LoRA genome) has a measured `score` from `vdd/score`, and
// the current self has a `baseline`. THIS decides, deterministically: adopt the
// best candidate that clears the adopt margin (assemble — zero training), else
// the market came up short → TRAIN a fresh layer to fill the gap. Pure — the
// candidates' scores come from elsewhere; this is the decision the whole
// "ask anything → assemble best self, or train" loop turns on.
// See docs/genome/SELF-EVOLVING-GENOME.md + memory ask-anything-assemble-best-self-or-train.
//=============================================================================
/// One scored candidate from the (trust-scoped) market search: a model or LoRA
/// genome, with its measured eval `score` (from `vdd/score`).
#[derive(Debug, Clone, Deserialize)]
pub(super) struct Candidate {
    pub(super) label: String,
    /// Held-out eval score ∈ [0,1] (the `vdd/score` of assembling this candidate).
    pub(super) score: f64,
}

#[derive(Debug, Deserialize)]
pub(super) struct DecideParams {
    /// The current self's score on the same held-out set (base / status quo).
    pub(super) baseline: f64,
    /// Scored candidates from the market (may be empty → nothing to assemble).
    #[serde(default)]
    pub(super) candidates: Vec<Candidate>,
    /// Minimum lift over baseline to bother ADOPTING a candidate rather than
    /// training. Below this, the market hasn't earned the swap → train.
    #[serde(default = "default_adopt_margin")]
    pub(super) adopt_margin: f64,
}

pub(super) fn default_adopt_margin() -> f64 {
    0.02
}

/// Pure decision: assemble the best-scoring candidate if it clears
/// `baseline + adopt_margin`; otherwise train. Returns the decision + the lift
/// the winning (or best-available) candidate would provide.
pub(super) fn decide_assemble_or_train(p: &DecideParams) -> Value {
    // Best available candidate by measured capability (score). Cost-weighted
    // value-density is the eviction/composition decision, not this one.
    let best = p.candidates.iter().max_by(|a, b| {
        a.score
            .partial_cmp(&b.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    match best {
        Some(c) if c.score - p.baseline >= p.adopt_margin => serde_json::json!({
            "action": "assemble",
            "chosen": c.label,
            "score": c.score,
            "baseline": p.baseline,
            "lift": c.score - p.baseline,
            "reason": "market candidate clears the adopt margin — assemble (zero training)",
        }),
        Some(c) => serde_json::json!({
            "action": "train",
            "best_candidate": c.label,
            "best_score": c.score,
            "baseline": p.baseline,
            "best_lift": c.score - p.baseline,
            "gap": p.adopt_margin - (c.score - p.baseline),
            "reason": "no market candidate clears the adopt margin — forage + train to fill the gap",
        }),
        None => serde_json::json!({
            "action": "train",
            "baseline": p.baseline,
            "reason": "market returned no candidates — train from the best available base",
        }),
    }
}
