//! Vision description — Rust-owned multimodal inference orchestration.
//!
//! Pre-#1276 this lived in `system/vision/VisionInferenceProvider.ts`
//! (176 LOC) which selected a vision-capable model, built the describe
//! prompt, called `AIProviderDaemon.generateText`, and parsed the
//! response. Per the oxidizer rule (Joel 2026-05-15: "if not UI/UX it
//! is rust") all four steps belong here. The TS file becomes a thin
//! shim that calls `Commands.execute('cognition/vision-describe', ...)`.
//!
//! The actual inference call delegates to the existing `ai/generate`
//! IPC handler via `runtime::execute_json`, so the Rust adapters
//! (Anthropic / OpenAI / LlamaCpp / etc.) handle multimodal payload
//! shaping per their own native API contracts. This module only owns:
//!
//! 1. Vision-capable model selection (filter `model_registry` by
//!    `Capability::Vision` + the registered adapter set, prefer local).
//! 2. Prompt construction from `VisionDescribeOptions` flags.
//! 3. Multimodal request assembly (text + base64 image content parts).
//! 4. Response parsing into `VisionDescription`.
//!
//! Outlier-validation pair: codex's #1284 (AIDecisionService.evaluateGating
//! → cognition/should-respond) is the structured-decision shape; this
//! card is the freeform-shape. Same Rust+thin-TS-shim pattern.

use serde::{Deserialize, Serialize};
use std::time::Instant;
use ts_rs::TS;

use crate::model_registry::{self, Capability};
use crate::runtime;

/// Request shape for the `cognition/vision-describe` IPC.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/VisionDescribeRequest.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct VisionDescribeRequest {
    /// Base64-encoded image bytes. The Rust adapter shapes this for the
    /// destination provider's wire format (Anthropic native base64,
    /// OpenAI image_url, llama.cpp mmproj).
    pub base64_data: String,
    /// MIME type (e.g. `image/png`, `image/jpeg`).
    pub mime_type: String,
    #[serde(default)]
    pub options: VisionDescribeOptions,
}

/// Per-call describe knobs. All optional — defaults give a concise prose
/// description with no structured-extraction prompts.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/VisionDescribeOptions.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct VisionDescribeOptions {
    /// If set, force this model id (must still be vision-capable).
    #[ts(optional)]
    pub preferred_model: Option<String>,
    /// If set, force this provider id.
    #[ts(optional)]
    pub preferred_provider: Option<String>,
    /// If set, cap the description length in characters (cascades to
    /// `max_tokens = ceil(max_length / 4)` for the underlying generate
    /// call, mirroring the prior TS heuristic).
    #[ts(optional)]
    pub max_length: Option<u32>,
    /// Override the auto-built prompt with a caller-supplied one.
    #[ts(optional)]
    pub prompt: Option<String>,
    /// Append "List the main objects you see." to the prompt.
    #[serde(default)]
    pub detect_objects: bool,
    /// Append "Note the dominant colors." to the prompt.
    #[serde(default)]
    pub detect_colors: bool,
    /// Append "Read any text visible in the image." to the prompt.
    #[serde(default)]
    pub detect_text: bool,
}

/// Result envelope for the `cognition/vision-describe` IPC. Mirrors the
/// TS `VisionDescription` interface in `system/vision/VisionDescriptionService.ts`
/// (which is consumed unchanged by the rest of the vision pipeline).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/VisionDescription.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct VisionDescription {
    pub description: String,
    pub model_id: String,
    pub provider: String,
    pub timestamp: String,
    #[ts(optional)]
    pub objects: Option<Vec<String>>,
    #[ts(optional)]
    pub colors: Option<Vec<String>>,
    #[ts(optional)]
    pub text: Option<String>,
    #[ts(type = "number")]
    pub response_time_ms: u64,
}

/// Vision-capable model candidate for selection. Pulled out as a struct
/// (vs the prior `(String, String, bool)` tuple) so the priority logic
/// can be unit-tested without standing up the global model registry.
#[derive(Debug, Clone, PartialEq, Eq)]
struct VisionCandidate {
    model_id: String,
    provider_id: String,
    is_local: bool,
}

/// Pure priority-ordering core. Pick the best `VisionCandidate` for
/// the given options, or `None` if `candidates` is empty.
///
/// Priority (mirrors the TS `selectModel` semantics):
///   1. `preferred_model` if set AND in `candidates`
///   2. `preferred_provider` if set AND has a candidate
///   3. First local-provider candidate
///   4. First candidate in the slice
///
/// Pure function — fully unit-testable. The registry IO is in the
/// caller (`select_vision_model`).
fn pick_vision_candidate<'a>(
    candidates: &'a [VisionCandidate],
    opts: &VisionDescribeOptions,
) -> Option<&'a VisionCandidate> {
    if candidates.is_empty() {
        return None;
    }

    // 1. Exact preferred_model match.
    if let Some(preferred) = opts.preferred_model.as_deref() {
        if let Some(c) = candidates.iter().find(|c| c.model_id == preferred) {
            return Some(c);
        }
    }

    // 2. preferred_provider's first candidate.
    if let Some(preferred) = opts.preferred_provider.as_deref() {
        if let Some(c) = candidates.iter().find(|c| c.provider_id == preferred) {
            return Some(c);
        }
    }

    // 3. Prefer a local provider when no explicit preference (free + private).
    if let Some(c) = candidates.iter().find(|c| c.is_local) {
        return Some(c);
    }

    // 4. Fall back to whatever's first.
    candidates.first()
}

/// Is a `llama-server`-provider vision row actually SERVABLE right now, per the
/// serving daemon's live snapshot? Pure — the snapshot IO lives in the caller.
///
/// The lane serves exactly ONE model, so a vision row is a real candidate only
/// when it IS the active model, the lane is decode-ready, AND the daemon's
/// verified multimodal verdict (`vision_ready`: declared Vision + resolved
/// mmproj + `/props modalities.vision`, #106) came back true. Anything less and
/// routing an image here would either hard-error at `select()` (model not
/// served) or be silently dropped by a text-only lane — the capability lie the
/// observe path must fail HONESTLY on instead
/// ([[fallbacks-are-illegal-fail-loud]]).
fn llama_server_row_ready(
    model_id: &str,
    snap: &crate::inference::llama_server::ServingSnapshot,
) -> Result<(), String> {
    if !snap.ready || snap.active_model.is_none() {
        return Err("serving lane has no ready model".to_string());
    }
    if snap.active_model.as_deref() != Some(model_id) {
        return Err(format!(
            "serving lane is occupied by {:?}, not this vision row — pin/pull it to bring \
             vision up (`models/pull` + `serving/pin`)",
            snap.active_model.as_deref().unwrap_or("<none>")
        ));
    }
    if !snap.vision_ready {
        return Err(
            "lane serves this model but its multimodal endpoint is NOT verified \
             (mmproj missing or /props reports no vision modality) — see the serving \
             daemon's `serving.vision.*` probes for the reason"
                .to_string(),
        );
    }
    Ok(())
}

/// Is this vision candidate SERVABLE right now? Registry rows *declare*
/// capability; this checks the lane behind them actually answers, so the
/// picker never selects a model whose `ai/generate` would hard-error at
/// `select()` (the pre-#106 failure: prefer-local picked a VL row with no
/// artifacts on disk and EVERY observe act died "no adapter", even while a
/// perfectly good cloud vision lane sat registered).
fn candidate_servable(m: &crate::model_registry::types::Model) -> Result<(), String> {
    let registry = model_registry::try_global().ok_or("model registry not initialized")?;
    let provider = registry
        .provider(&m.provider)
        .ok_or_else(|| format!("provider {:?} not in registry", m.provider))?;

    if m.provider == crate::inference::llama_server::PROVIDER_ID {
        // The local serving lane: gate on the daemon's LIVE snapshot.
        return llama_server_row_ready(&m.id, &crate::inference::llama_server::current_serving());
    }
    if m.provider == crate::inference::LLAMACPP_PROVIDER_ID {
        // The retiring in-process path is OPT-IN ONLY (CONTINUUM_LOCAL_LLAMA=1,
        // see AIProviderModule) — without the opt-in no adapter registers for
        // these rows, so they are never servable candidates.
        if crate::config_env::read("CONTINUUM_LOCAL_LLAMA").as_deref() != Some("1") {
            return Err("in-process llama.cpp is not opted in (CONTINUUM_LOCAL_LLAMA=1)".into());
        }
        if crate::model_registry::artifacts::resolve_gguf_for_model(m).is_none() {
            return Err("no local GGUF resolves for this row".into());
        }
        if crate::model_registry::artifacts::resolve_mmproj_for_model(m).is_none() {
            return Err("no mmproj projector resolves for this row".into());
        }
        return Ok(());
    }
    // Cloud (and other keyless local gateways): the adapter registers iff the
    // provider's API key secret is present — mirror that gate here so a keyless
    // boot never picks a cloud vision model whose `select()` would refuse.
    match provider.api_key_env.as_deref() {
        None => Ok(()),
        Some(env_key) if crate::secrets::get_secret(env_key).is_some() => Ok(()),
        Some(env_key) => Err(format!("no {env_key} secret — provider not registered")),
    }
}

/// Pick the best vision-capable model from the global model registry.
///
/// Returns `(model_id, provider_id)` or `None` if no vision-capable model is
/// SERVABLE (declared capability alone is not enough — see
/// [`candidate_servable`]). Wraps `pick_vision_candidate` with the registry +
/// serving-snapshot IO; the priority logic itself lives in the pure helper for
/// tests. Skipped candidates are logged with their reason so an all-skipped
/// outcome (the honest "no eyes available" failure) is diagnosable, not mute.
fn select_vision_model(opts: &VisionDescribeOptions) -> Option<(String, String)> {
    let registry = model_registry::try_global()?;

    let candidates: Vec<VisionCandidate> = registry
        .models()
        .filter(|m| m.has(Capability::Vision))
        .filter_map(|m| {
            let provider = registry.provider(&m.provider)?;
            if let Err(why) = candidate_servable(m) {
                runtime::logger("cognition").info(&format!(
                    "vision-describe: skipping {:?} — {why}",
                    m.id
                ));
                return None;
            }
            Some(VisionCandidate {
                model_id: m.id.clone(),
                provider_id: m.provider.clone(),
                is_local: matches!(
                    provider.kind,
                    crate::model_registry::types::ProviderKind::Local
                ),
            })
        })
        .collect();

    pick_vision_candidate(&candidates, opts).map(|c| (c.model_id.clone(), c.provider_id.clone()))
}

/// Build the describe prompt from option flags.
///
/// Mirrors the TS `buildPrompt` exactly. Kept pure (no IO) so it's
/// trivially unit-testable and stable across migrations.
pub fn build_prompt(opts: &VisionDescribeOptions) -> String {
    let mut parts: Vec<String> = vec!["Describe this image concisely.".to_string()];
    if opts.detect_objects {
        parts.push("List the main objects you see.".to_string());
    }
    if opts.detect_colors {
        parts.push("Note the dominant colors.".to_string());
    }
    if opts.detect_text {
        parts.push("Read any text visible in the image.".to_string());
    }
    if let Some(max_length) = opts.max_length {
        parts.push(format!(
            "Keep the description under {} characters.",
            max_length
        ));
    }
    parts.join(" ")
}

/// Parsed view of a vision-LLM freeform response.
struct ParsedResponse {
    description: String,
    objects: Option<Vec<String>>,
    colors: Option<Vec<String>>,
    text: Option<String>,
}

/// Parse the LLM's freeform response into structured fields.
///
/// v1 (matches the prior TS): just trim + return as `description`. The
/// TS placeholder always returned `{ description: text.trim() }` and
/// never populated `objects` / `colors` / `text` — extracting those
/// would require a second LLM call or a structured-output mode the
/// pipeline doesn't yet wire up. Preserving the same behavior on
/// migration day; structured extraction is a future card.
fn parse_response(text: &str) -> ParsedResponse {
    ParsedResponse {
        description: text.trim().to_string(),
        objects: None,
        colors: None,
        text: None,
    }
}

/// Top-level entry — describe an image via the best available
/// vision-capable model.
///
/// Returns `Ok(None)` when no vision model is registered or generation
/// fails (matching the prior TS `Promise<VisionDescription | null>`
/// contract). Returns `Err` on caller errors (malformed params,
/// `runtime::execute_json` failure, etc.).
pub async fn describe_image(
    req: VisionDescribeRequest,
    executor: &std::sync::Arc<crate::runtime::CommandExecutor>,
) -> Result<Option<VisionDescription>, String> {
    let start = Instant::now();

    let Some((model_id, provider_id)) = select_vision_model(&req.options) else {
        return Ok(None);
    };

    // If the caller asked for a specific model and we couldn't honor it,
    // log the substitution so the call site can audit which provider
    // actually ran. Quiet on the no-preference path (the common case).
    if let Some(requested) = req.options.preferred_model.as_deref() {
        if requested != model_id {
            runtime::logger("cognition").info(&format!(
                "vision-describe: preferred_model {:?} unavailable, substituted {:?} (from provider {:?})",
                requested, model_id, provider_id,
            ));
        }
    }

    let prompt = req
        .options
        .prompt
        .clone()
        .unwrap_or_else(|| build_prompt(&req.options));

    // Build the multimodal `ai/generate` request payload. Shape mirrors
    // what the TS-side AIProviderDaemon.generateText expects + what the
    // Rust adapters (Anthropic / OpenAI / LlamaCpp) parse out.
    //
    // `div_ceil` so a max_length of e.g. 100 chars maps to ceil(100/4)
    // = 25 tokens (vs the prior `(len + 3) / 4` which computed the same
    // value but obscured intent). The 50-token floor keeps the request
    // viable when callers pass small max_length hints.
    let max_tokens = req
        .options
        .max_length
        .map(|len| u32::max(50, len.div_ceil(4)))
        .unwrap_or(500);

    let generate_params = serde_json::json!({
        "messages": [{
            "role": "user",
            "content": [
                { "type": "text", "text": prompt },
                {
                    "type": "image",
                    "image": {
                        "base64": req.base64_data,
                        "mimeType": req.mime_type,
                    },
                },
            ],
        }],
        "model": model_id,
        "provider": provider_id,
        "maxTokens": max_tokens,
        "temperature": 0.3,
    });

    let response_value = executor.execute_json("ai/generate", generate_params).await?;

    // ai/generate's wire format serializes FinishReason via Display
    // (`modules/ai_provider.rs::response_to_json`); the sentinel string
    // matches `crate::ai::types::FinishReason::Error`'s Display impl.
    // Deserialize back to the typed enum so any future variant rename
    // is caught at compile time on both sides of the wire.
    let finish_reason: Option<crate::ai::types::FinishReason> = response_value
        .get("finishReason")
        .and_then(|v| v.as_str())
        .and_then(|s| serde_json::from_value(serde_json::Value::String(s.to_string())).ok());
    let response_text = response_value
        .get("text")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if matches!(finish_reason, Some(crate::ai::types::FinishReason::Error))
        || response_text.is_empty()
    {
        return Ok(None);
    }

    let parsed = parse_response(response_text);

    Ok(Some(VisionDescription {
        description: parsed.description,
        model_id,
        provider: provider_id,
        timestamp: chrono::Utc::now().to_rfc3339(),
        objects: parsed.objects,
        colors: parsed.colors,
        text: parsed.text,
        response_time_ms: start.elapsed().as_millis() as u64,
    }))
}

/// The live [`FrameDescriber`](crate::media::FrameDescriber) — the sensory bridge that
/// `media/` promises but cannot implement itself (the layering rule forbids `media/`
/// from depending on `cognition/`). This is the ONE production implementor: it routes a
/// frame's bytes through [`describe_image`] (i.e. `ai/generate` on the best available
/// vision model) and hands back the prose a non-vision persona reads to "see"
/// ([[perception-feedback-must-not-blow-rag]]).
///
/// A [`MediaFrame`](crate::media::MediaFrame) caches the result per content hash, so
/// wrapping this describer ONCE and pointing every persona's frame at it means the same
/// image is described a single time and shared — N personas in a call cost one describe
/// per distinct frame, not N ([[media-is-compute-once-zero-copy-hardware-grade]]).
pub struct VisionDescribeFramer {
    executor: std::sync::Arc<crate::runtime::CommandExecutor>,
    options: VisionDescribeOptions,
}

impl VisionDescribeFramer {
    /// Bridge over the given command executor with default describe options
    /// (concise prose, best available vision model).
    pub fn new(executor: std::sync::Arc<crate::runtime::CommandExecutor>) -> Self {
        Self {
            executor,
            options: VisionDescribeOptions::default(),
        }
    }

    /// Bridge with caller-tuned options (e.g. `detect_objects`, a length cap, or a
    /// pinned model) applied to every frame this describer handles.
    pub fn with_options(
        executor: std::sync::Arc<crate::runtime::CommandExecutor>,
        options: VisionDescribeOptions,
    ) -> Self {
        Self { executor, options }
    }
}

/// Encode + shape a frame's raw bytes into a [`VisionDescribeRequest`]. Pulled out as a
/// pure function so the base64 + option threading is unit-testable without standing up a
/// `CommandExecutor` or the model registry (the executor IO lives in `describe_image`).
fn build_frame_request(
    source: &[u8],
    mime: &str,
    options: VisionDescribeOptions,
) -> VisionDescribeRequest {
    use base64::Engine;
    VisionDescribeRequest {
        base64_data: base64::engine::general_purpose::STANDARD.encode(source),
        mime_type: mime.to_string(),
        options,
    }
}

#[async_trait::async_trait]
impl crate::media::FrameDescriber for VisionDescribeFramer {
    async fn describe(&self, source: &[u8], mime: &str) -> Result<String, String> {
        let req = build_frame_request(source, mime, self.options.clone());
        match describe_image(req, &self.executor).await? {
            Some(desc) => Ok(desc.description),
            // No vision-capable model resolved (or the model errored / returned empty).
            // Fail loud — a describe with no real sight must never fabricate a
            // placeholder a persona would read AS having seen the frame
            // ([[fallbacks-are-illegal-fail-loud]]). `MediaFrame` caches this `Err` per
            // content hash, so the gap is surfaced deterministically, not silently
            // retried per persona.
            None => Err(format!(
                "no vision-capable model available to describe this {mime} frame — bring \
                 up a VL model (or grant a vision provider) before a persona can see it"
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_prompt_default_is_concise() {
        let prompt = build_prompt(&VisionDescribeOptions::default());
        assert_eq!(prompt, "Describe this image concisely.");
    }

    // what this catches: the live FrameDescriber bridge encodes a frame's raw bytes as
    // standard base64 and threads the caller's describe options through to the
    // ai/generate request — the wiring the compute-once description cell depends on to
    // let a non-vision persona "see". A regression here silently corrupts every frame
    // the perception pipeline sends for description.
    #[test]
    fn build_frame_request_base64_encodes_and_threads_options() {
        let opts = VisionDescribeOptions {
            detect_objects: true,
            max_length: Some(120),
            ..Default::default()
        };
        let req = build_frame_request(b"hello", "image/png", opts);
        assert_eq!(req.base64_data, "aGVsbG8=", "base64('hello')");
        assert_eq!(req.mime_type, "image/png");
        assert!(req.options.detect_objects);
        assert_eq!(req.options.max_length, Some(120));
    }

    #[test]
    fn build_prompt_appends_object_directive() {
        let opts = VisionDescribeOptions {
            detect_objects: true,
            ..Default::default()
        };
        let prompt = build_prompt(&opts);
        assert!(prompt.contains("List the main objects"));
    }

    #[test]
    fn build_prompt_appends_all_directives_in_order() {
        let opts = VisionDescribeOptions {
            detect_objects: true,
            detect_colors: true,
            detect_text: true,
            max_length: Some(120),
            ..Default::default()
        };
        let prompt = build_prompt(&opts);
        assert!(prompt.contains("Describe this image concisely."));
        assert!(prompt.contains("List the main objects"));
        assert!(prompt.contains("dominant colors"));
        assert!(prompt.contains("Read any text"));
        assert!(prompt.contains("under 120 characters"));
    }

    #[test]
    fn parse_response_trims_and_returns_description_only() {
        let parsed = parse_response("  hello world  \n");
        assert_eq!(parsed.description, "hello world");
        assert!(parsed.objects.is_none());
        assert!(parsed.colors.is_none());
        assert!(parsed.text.is_none());
    }

    // ─── select_vision_model 4-branch priority logic ──────────────────────
    //
    // pick_vision_candidate is the pure core; select_vision_model is the
    // registry-IO wrapper. Tests target the pure core so each branch is
    // exercised without standing up the global model registry.

    fn cand(model: &str, provider: &str, is_local: bool) -> VisionCandidate {
        VisionCandidate {
            model_id: model.to_string(),
            provider_id: provider.to_string(),
            is_local,
        }
    }

    #[test]
    fn pick_vision_candidate_returns_none_when_empty() {
        assert!(pick_vision_candidate(&[], &VisionDescribeOptions::default()).is_none());
    }

    #[test]
    fn pick_vision_candidate_priority_1_preferred_model_wins_over_local() {
        // preferred_model picks the named model EVEN when a local
        // alternative exists. Caller intent beats local-cost preference.
        let candidates = vec![
            cand("local-llava", "llamacpp-local", true),
            cand("claude-vision", "anthropic", false),
        ];
        let opts = VisionDescribeOptions {
            preferred_model: Some("claude-vision".to_string()),
            ..Default::default()
        };
        let picked = pick_vision_candidate(&candidates, &opts).unwrap();
        assert_eq!(picked.model_id, "claude-vision");
        assert_eq!(picked.provider_id, "anthropic");
    }

    #[test]
    fn pick_vision_candidate_priority_2_preferred_provider_wins_over_local() {
        // preferred_provider with no preferred_model picks the FIRST
        // candidate from that provider, even when a local exists.
        let candidates = vec![
            cand("local-llava", "llamacpp-local", true),
            cand("gpt-4o", "openai", false),
            cand("gpt-4o-mini", "openai", false),
        ];
        let opts = VisionDescribeOptions {
            preferred_provider: Some("openai".to_string()),
            ..Default::default()
        };
        let picked = pick_vision_candidate(&candidates, &opts).unwrap();
        assert_eq!(picked.provider_id, "openai");
        // First openai candidate, not the second.
        assert_eq!(picked.model_id, "gpt-4o");
    }

    #[test]
    fn pick_vision_candidate_priority_3_prefers_local_when_no_preference() {
        // No preference → local provider wins (free + private).
        let candidates = vec![
            cand("claude-vision", "anthropic", false),
            cand("gpt-4o", "openai", false),
            cand("local-llava", "llamacpp-local", true),
        ];
        let picked = pick_vision_candidate(&candidates, &VisionDescribeOptions::default()).unwrap();
        assert!(picked.is_local);
        assert_eq!(picked.model_id, "local-llava");
    }

    #[test]
    fn pick_vision_candidate_priority_4_first_when_no_local_no_preference() {
        // No local, no preference → first candidate.
        let candidates = vec![
            cand("claude-vision", "anthropic", false),
            cand("gpt-4o", "openai", false),
        ];
        let picked = pick_vision_candidate(&candidates, &VisionDescribeOptions::default()).unwrap();
        assert_eq!(picked.model_id, "claude-vision");
    }

    #[test]
    fn pick_vision_candidate_unknown_preferred_model_falls_through_to_local() {
        // preferred_model that doesn't match any candidate falls through
        // to the next priority — local wins. (The describe_image caller
        // logs the substitution for audit.)
        let candidates = vec![
            cand("claude-vision", "anthropic", false),
            cand("local-llava", "llamacpp-local", true),
        ];
        let opts = VisionDescribeOptions {
            preferred_model: Some("nonexistent-vision-model".to_string()),
            ..Default::default()
        };
        let picked = pick_vision_candidate(&candidates, &opts).unwrap();
        assert!(picked.is_local);
        assert_eq!(picked.model_id, "local-llava");
    }

    // what this catches (#106): the observe path may route to the local serving lane
    // ONLY when the lane is ready, serving THIS model, and its multimodal endpoint is
    // VERIFIED (`vision_ready`). A regression that accepts a ready-but-unverified lane
    // re-opens the capability lie (images POSTed to a text-only lane, silently
    // dropped); one that accepts a different active model re-opens the pre-#106 bug
    // where every observe act died at select() while a good lane sat elsewhere.
    #[test]
    fn llama_server_row_is_servable_only_when_active_ready_and_vision_verified() {
        use crate::inference::llama_server::ServingSnapshot;
        let mut snap = ServingSnapshot::empty();

        // Nothing serving → not servable.
        assert!(llama_server_row_ready("vl-model", &snap).is_err());

        // Ready but the lane serves a DIFFERENT model → not servable, and the
        // reason names the occupant (the operator's pin/pull hint).
        snap.ready = true;
        snap.active_model = Some("coder-14b".into());
        snap.vision_ready = false;
        let err = llama_server_row_ready("vl-model", &snap).unwrap_err();
        assert!(err.contains("coder-14b"), "{err}");

        // Serving this model but multimodal endpoint NOT verified → not servable.
        snap.active_model = Some("vl-model".into());
        let err = llama_server_row_ready("vl-model", &snap).unwrap_err();
        assert!(err.contains("NOT verified"), "{err}");

        // All three facts line up → servable.
        snap.vision_ready = true;
        assert!(llama_server_row_ready("vl-model", &snap).is_ok());
    }

    #[test]
    fn pick_vision_candidate_unknown_preferred_provider_falls_through_to_first() {
        // preferred_provider that doesn't match falls through. With no
        // local, picks first.
        let candidates = vec![
            cand("claude-vision", "anthropic", false),
            cand("gpt-4o", "openai", false),
        ];
        let opts = VisionDescribeOptions {
            preferred_provider: Some("groq".to_string()),
            ..Default::default()
        };
        let picked = pick_vision_candidate(&candidates, &opts).unwrap();
        assert_eq!(picked.model_id, "claude-vision");
    }
}
