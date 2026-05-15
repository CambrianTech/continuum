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
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../shared/generated/cognition/VisionDescribeRequest.ts"
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
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../shared/generated/cognition/VisionDescribeOptions.ts"
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
    export_to = "../../../shared/generated/cognition/VisionDescription.ts"
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

/// Pick the best vision-capable model.
///
/// Priority (mirrors the TS `selectModel` semantics):
///   1. `preferred_model` if set AND vision-capable
///   2. `preferred_provider` if set AND has a vision-capable model
///   3. First local provider's first vision-capable model
///   4. First vision-capable model in the registry
///
/// Returns `(model_id, provider_id)` or `None` if no vision-capable
/// model is registered.
fn select_vision_model(opts: &VisionDescribeOptions) -> Option<(String, String)> {
    let registry = model_registry::try_global()?;

    // Collect (model_id, provider_id, is_local) triples for vision-capable models.
    let candidates: Vec<(String, String, bool)> = registry
        .models()
        .filter(|m| m.has(Capability::Vision))
        .filter_map(|m| {
            let provider = registry.provider(&m.provider)?;
            Some((
                m.id.clone(),
                m.provider.clone(),
                matches!(provider.kind, crate::model_registry::types::ProviderKind::Local),
            ))
        })
        .collect();

    if candidates.is_empty() {
        return None;
    }

    // 1. Exact preferred_model match (must be vision-capable).
    if let Some(preferred) = opts.preferred_model.as_deref() {
        if let Some((mid, pid, _)) = candidates.iter().find(|(mid, _, _)| mid == preferred) {
            return Some((mid.clone(), pid.clone()));
        }
    }

    // 2. preferred_provider's first vision-capable model.
    if let Some(preferred) = opts.preferred_provider.as_deref() {
        if let Some((mid, pid, _)) = candidates.iter().find(|(_, pid, _)| pid == preferred) {
            return Some((mid.clone(), pid.clone()));
        }
    }

    // 3. Prefer a local provider when no explicit preference (free + private).
    if let Some((mid, pid, _)) = candidates.iter().find(|(_, _, local)| *local) {
        return Some((mid.clone(), pid.clone()));
    }

    // 4. Fall back to whatever's first.
    let (mid, pid, _) = &candidates[0];
    Some((mid.clone(), pid.clone()))
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
) -> Result<Option<VisionDescription>, String> {
    let start = Instant::now();

    let Some((model_id, provider_id)) = select_vision_model(&req.options) else {
        return Ok(None);
    };

    let prompt = req
        .options
        .prompt
        .clone()
        .unwrap_or_else(|| build_prompt(&req.options));

    // Build the multimodal `ai/generate` request payload. Shape mirrors
    // what the TS-side AIProviderDaemon.generateText expects + what the
    // Rust adapters (Anthropic / OpenAI / LlamaCpp) parse out.
    let max_tokens = req
        .options
        .max_length
        .map(|len| u32::max(50, (len + 3) / 4))
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

    let response_value = runtime::execute_command_json("ai/generate", generate_params).await?;

    let finish_reason = response_value
        .get("finishReason")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let response_text = response_value
        .get("text")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if finish_reason == "error" || response_text.is_empty() {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_prompt_default_is_concise() {
        let prompt = build_prompt(&VisionDescribeOptions::default());
        assert_eq!(prompt, "Describe this image concisely.");
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
}
