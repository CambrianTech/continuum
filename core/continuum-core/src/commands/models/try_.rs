//! `models/try` — verify a model by ACTUALLY RUNNING it, then write what we
//! learned back into the live universe.
//!
//! This is the first MUTATING `models/*` command: it runs a minimal text
//! generation (and, for a vision model, a text+image probe), measures the real
//! tokens/sec, and [`attach_verification`](crate::model_registry::live::ModelCatalog::attach_verification)s
//! a [`VerifyReport`] onto the live catalog entry. A subscriber (widget/persona)
//! sees the snapshot generation bump and the `verified` field populate WITHOUT a
//! reboot — the "rich, real-time API" payoff made concrete.
//!
//! ## Where the data comes from
//!
//! - The model row + its advertised capabilities: the live [`ModelCatalog`].
//! - The verdict: a real round-trip through the [`AdapterRegistry`] inference
//!   seam (the same seam `ai/generate` uses) — not an estimate, not a claim.
//! - The measured tps: output tokens ÷ wall-clock, the live correction to the
//!   catalog's startup estimate.
//!
//! ## Gating
//!
//! `Privileged` — it leases compute (runs the model) and mutates substrate
//! state. It is not a casual `AiSafe` read like `models/list`.

use std::sync::Arc;
use std::time::Instant;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use ts_rs::TS;

use crate::ai::adapter::InferenceDevice;
use crate::ai::types::ImageInput;
use crate::ai::{AdapterRegistry, ChatMessage, ContentPart, MessageContent, TextGenerationRequest};
use crate::model_registry::live::{ModelCatalog, VerifyReport};
use crate::model_registry::Capability;
use crate::modules::ai_provider::select_failure_message;
use crate::sdk_codegen::CommandError;

/// A 2×2 solid-red PNG (the smallest real image that survives a vision encoder).
/// Embedded so the vision probe sends a GENUINE image — not a placeholder the
/// model could trivially "pass" without seeing pixels.
const PROBE_IMAGE_PNG_BASE64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAIAAAD91JpzAAAAEklEQVR4nGP8z8DwnwEJMOEXAACqDgX9P0bjOgAAAABJRU5ErkJggg==";

/// Which model to verify, by its catalog id.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/model_registry/ModelsTryParams.ts")]
pub struct ModelsTryParams {
    /// The model id as it appears in `models/list` (e.g. `qwen2-vl-7b`). Fails
    /// loud if it is not in the live universe — verify what exists, don't invent.
    pub model_id: String,
}

crate::action_command! {
    /// Verify a model by actually running it: a minimal text generation, plus a
    /// text+image probe if the model advertises vision. Measures real tokens/sec
    /// and records the verdict onto the live catalog entry (so the universe shows
    /// it verified, with measured speed, with no reboot). Returns the report.
    pub struct ModelsTry {
        catalog: Arc<ModelCatalog>,
        registry: Arc<RwLock<AdapterRegistry>>,
    }
    name: "models/try",
    access: Privileged,
    params: ModelsTryParams,
    output: VerifyReport,
    run(this, _ctx, p) => {
        // 1. The model must exist in the live universe.
        let snap = this.catalog.snapshot();
        let live = snap.get(&p.model_id).ok_or_else(|| {
            CommandError::NotFound(format!(
                "unknown model id '{}' — call models/list to see the live universe, don't name a provider artifact directly",
                p.model_id
            ))
        })?;
        let claims_vision = live.model.has(Capability::Vision);

        // 2. Text smoke test — a real round-trip through the inference seam.
        let registry = this.registry.read().await;
        let (text_ok, measured_tps, text_detail) =
            run_text_probe(&registry, &p.model_id).await;

        // 3. Vision probe — only when the model claims it; a genuine 2×2 image.
        let (vision_ok, vision_detail) = if claims_vision {
            let (ok, detail) = run_vision_probe(&registry, &p.model_id).await;
            (Some(ok), Some(detail))
        } else {
            (None, None)
        };
        drop(registry);

        let detail = match vision_detail {
            Some(v) => format!("{text_detail}; vision: {v}"),
            None => format!("{text_detail}; vision: not advertised"),
        };
        let report = VerifyReport { text_ok, vision_ok, measured_tps, detail };

        // 4. Write the verdict back into the live universe (bumps generation).
        if !this.catalog.attach_verification(&p.model_id, report.clone()) {
            return Err(CommandError::Internal(format!(
                "model '{}' vanished from the live catalog during verification",
                p.model_id
            )));
        }

        Ok(report)
    }
}

/// Run a one-line text generation and measure tokens/sec. Returns
/// `(text_ok, measured_tps, detail)`. A select/generate failure is a verdict
/// (`text_ok = false` with the reason), not a command error — the point of
/// `models/try` is to RECORD what happened, not abort.
async fn run_text_probe(
    registry: &AdapterRegistry,
    model_id: &str,
) -> (bool, Option<f32>, String) {
    let request = TextGenerationRequest {
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: MessageContent::Text("Reply with the single word: ok".to_string()),
            name: None,
        }],
        system_prompt: None,
        model: Some(model_id.to_string()),
        provider: None,
        temperature: Some(0.0),
        max_tokens: Some(16),
        top_p: None,
        top_k: None,
        repeat_penalty: None,
        frequency_penalty: None,
        repeat_last_n: None,
        stop_sequences: None,
        tools: None,
        tool_choice: None,
        response_format: None,
        active_adapters: None,
        request_id: None,
        user_id: None,
        room_id: None,
        purpose: Some("models/try:text".to_string()),
        persona_id: None,
    };

    let adapter = match registry.select(None, Some(model_id), InferenceDevice::default()) {
        Some((_, adapter)) => adapter,
        None => {
            return (
                false,
                None,
                select_failure_message(registry, None, Some(model_id)),
            )
        }
    };

    let started = Instant::now();
    match adapter.generate_text(request).await {
        Ok(resp) => {
            let secs = started.elapsed().as_secs_f32();
            let tps = if secs > 0.0 && resp.usage.output_tokens > 0 {
                Some(resp.usage.output_tokens as f32 / secs)
            } else {
                None
            };
            let ok = !resp.text.trim().is_empty();
            (ok, tps, format!("text: {} tokens in {:.2}s", resp.usage.output_tokens, secs))
        }
        Err(e) => (false, None, format!("text generation failed: {e}")),
    }
}

/// Run a text+image generation against a genuine 2×2 image. Returns
/// `(vision_ok, detail)`.
async fn run_vision_probe(registry: &AdapterRegistry, model_id: &str) -> (bool, String) {
    let request = TextGenerationRequest {
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: MessageContent::Parts(vec![
                ContentPart::Text {
                    text: "What is in this image? Answer in one word.".to_string(),
                },
                ContentPart::Image {
                    image: ImageInput {
                        url: None,
                        base64: Some(PROBE_IMAGE_PNG_BASE64.to_string()),
                        mime_type: Some("image/png".to_string()),
                    },
                },
            ]),
            name: None,
        }],
        system_prompt: None,
        model: Some(model_id.to_string()),
        provider: None,
        temperature: Some(0.0),
        max_tokens: Some(16),
        top_p: None,
        top_k: None,
        repeat_penalty: None,
        frequency_penalty: None,
        repeat_last_n: None,
        stop_sequences: None,
        tools: None,
        tool_choice: None,
        response_format: None,
        active_adapters: None,
        request_id: None,
        user_id: None,
        room_id: None,
        purpose: Some("models/try:vision".to_string()),
        persona_id: None,
    };

    let adapter = match registry.select(None, Some(model_id), InferenceDevice::default()) {
        Some((_, adapter)) => adapter,
        None => return (false, "no adapter for vision probe".to_string()),
    };

    match adapter.generate_text(request).await {
        Ok(resp) if !resp.text.trim().is_empty() => {
            (true, format!("vision probe answered ({} tokens)", resp.usage.output_tokens))
        }
        Ok(_) => (false, "vision probe returned empty".to_string()),
        Err(e) => (false, format!("vision probe failed: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_registry::catalog;
    use crate::sdk_codegen::{ActionCommand, Ctx};

    fn empty_registry() -> Arc<RwLock<AdapterRegistry>> {
        Arc::new(RwLock::new(AdapterRegistry::new()))
    }

    // what this catches: verifying a model that is not in the live universe fails
    // loud (NotFound), naming the resolver path — never silently no-ops or
    // fabricates a verdict for a nonexistent id.
    #[tokio::test]
    async fn unknown_model_fails_loud() {
        let reg = catalog::registry().expect("Rust catalog must validate");
        let cat = Arc::new(ModelCatalog::from_registry(&reg));
        let cmd = ModelsTry {
            catalog: cat,
            registry: empty_registry(),
        };
        let err = cmd
            .run(
                &Ctx::default(),
                ModelsTryParams {
                    model_id: "does-not-exist".to_string(),
                },
            )
            .await
            .expect_err("unknown model must fail loud");
        assert!(matches!(err, CommandError::NotFound(_)));
    }

    // what this catches: for a real, known model with no adapter registered, the
    // probe records a verdict (text_ok = false) and ATTACHES it to the live
    // catalog (generation bumps) rather than erroring — models/try records what
    // happened, it does not abort on an unservable model. Vision verdict is None
    // for a text-only model.
    #[tokio::test]
    async fn known_text_model_without_adapter_records_failed_verdict() {
        let reg = catalog::registry().expect("Rust catalog must validate");
        let cat = Arc::new(ModelCatalog::from_registry(&reg));
        // Pick a deterministic text-only model id from the seeded universe.
        let snap = cat.snapshot();
        let id = snap
            .models
            .values()
            .find(|m| !m.model.has(Capability::Vision))
            .map(|m| m.model.id.clone())
            .expect("seeded universe must contain a text-only model");
        let gen_before = snap.generation;
        drop(snap);

        let cmd = ModelsTry {
            catalog: cat.clone(),
            registry: empty_registry(),
        };
        let report = cmd
            .run(&Ctx::default(), ModelsTryParams { model_id: id.clone() })
            .await
            .expect("known model records a verdict, never errors on no-adapter");
        assert!(!report.text_ok, "no adapter ⇒ text probe fails");
        assert!(report.vision_ok.is_none(), "text-only model ⇒ no vision verdict");

        // The verdict was written into the live universe.
        let after = cat.snapshot();
        assert!(after.generation > gen_before, "attach_verification bumps generation");
        assert!(after.get(&id).unwrap().status.verified.is_some());
    }

    #[test]
    fn name_mirrors_path() {
        assert_eq!(ModelsTry::NAME, "models/try");
    }
}
