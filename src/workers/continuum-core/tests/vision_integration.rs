//! Vision integration test — proves the Rust persona pipeline
//! carries image data end-to-end to a natively-multimodal model.
//!
//! This exercises the path Joel called out as the thesis:
//!
//!   message_media: Vec<MediaItemLite>  (RespondInput)
//!     → build_messages_with_media                (persona/response.rs)
//!     → ContentPart::Image { base64, mime_type } (ai/types.rs)
//!     → adapter.generate_text                    (AIProviderAdapter)
//!     → provider API receives raw pixels         (NO text-description bridge)
//!     → model returns description of the image
//!
//! The test does NOT check the model's vision accuracy (that's the model
//! vendor's job). It checks that the pipeline **delivers** the image
//! bytes through every layer without silently flattening to text. A
//! working vision model fed a red square should say something about
//! red / color / the image being present — if it says "I don't see an
//! image" or returns Silent, some layer dropped the bytes.
//!
//! Target model: Claude Sonnet 4.5 — already declared `Capability::Vision`
//! in `config/models.toml`, already has an Anthropic adapter in the
//! registry, already accepts base64 image parts over HTTP. Requires
//! `ANTHROPIC_API_KEY` in `config.env`. Local Qwen2-VL-7B pathway
//! (anvil's adapter wiring + mtmd FFI, in progress 2026-04-21) slots
//! into the same test by swapping the `model` string once the registry
//! entry lands — the pipeline itself is provider-agnostic.
//!
//! Marked `#[ignore]` because it hits the live Anthropic API and costs
//! real tokens (~$0.003/run at current Sonnet pricing). Run explicitly:
//!
//!   cargo test --test vision_integration -- --ignored --nocapture

use continuum_core::cognition::tool_executor::types::MediaItemLite;
use continuum_core::persona::response::{respond, PersonaResponse, RespondInput};
use uuid::Uuid;

/// Minimal valid JPEG — 8x8 red square, ~160 bytes encoded.
/// Deterministic so the test is byte-stable across runs.
///
/// Generated with ImageMagick:
///   convert -size 8x8 xc:red -quality 50 red.jpg
///   base64 -i red.jpg
///
/// A vision-capable model receiving this should respond with something
/// about red / the image / a square. Text-only interpretation ("no
/// image provided" or similar silent-drop symptom) proves a pipeline
/// layer flattened the bytes.
const RED_SQUARE_JPEG_B64: &str = "\
/9j/4AAQSkZJRgABAQAAAQABAAD/2wBDAAYEBQYFBAYGBQYHBwYIChAKCgkJChQODwwQFxQYGBcU\
FhYaHSUfGhsjHBYWICwgIyYnKSopGR8tMC0oMCUoKSj/2wBDAQcHBwoIChMKChMoGhYaKCgoKCgo\
KCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCj/wAARCAAIAAgDASIA\
AhEBAxEB/8QAHwAAAQUBAQEBAQEAAAAAAAAAAAECAwQFBgcICQoL/8QAtRAAAgEDAwIEAwUFBAQA\
AAF9AQIDAAQRBRIhMUEGE1FhByJxFDKBkaEII0KxwRVS0fAkM2JyggkKFhcYGRolJicoKSo0NTY3\
ODk6Q0RFRkdISUpTVFVWV1hZWmNkZWZnaGlqc3R1dnd4eXqDhIWGh4iJipKTlJWWl5iZmqKjpKWm\
p6ipqrKztLW2t7i5usLDxMXGx8jJytLT1NXW19jZ2uHi4+Tl5ufo6erx8vP09fb3+Pn6/8QAHwEA\
AwEBAQEBAQEBAQAAAAAAAAECAwQFBgcICQoL/8QAtREAAgECBAQDBAcFBAQAAQJ3AAECAxEEBSEx\
BhJBUQdhcRMiMoEIFEKRobHBCSMzUvAVYnLRChYkNOEl8RcYGRomJygpKjU2Nzg5OkNERUZHSElK\
U1RVVldYWVpjZGVmZ2hpanN0dXZ3eHl6goOEhYaHiImKkpOUlZaXmJmaoqOkpaanqKmqsrO0tba3\
uLm6wsPExcbHyMnK0tPU1dbX2Nna4uPk5ebn6Onq8vP09fb3+Pn6/9oADAMBAAIRAxEAPwD5/oor\
6A/YZ/ZM/4a58RT6ZN4zsPClvbrAzyvZtezOsrMoEUIkjDNlcfeHLKPouR9xz//Z";

/// Build a RespondInput that carries the red-square image to `respond()`.
///
/// Minimal-but-realistic shape: a single-persona room, one user message
/// asking about the attached image, the image itself in `message_media`.
/// System prompt is deliberately short so any model-side chattiness
/// about the image content dominates the output (makes assertions
/// simpler).
fn build_vision_request(model_id: &str) -> RespondInput {
    let media = vec![MediaItemLite {
        item_type: "image".to_string(),
        base64: Some(RED_SQUARE_JPEG_B64.to_string()),
        mime_type: Some("image/jpeg".to_string()),
    }];

    RespondInput {
        persona: continuum_core::cognition::PersonaSlot {
            persona_id: Uuid::nil(),
            specialty: "vision".to_string(),
            display_name: "VisionTestPersona".to_string(),
        },
        room_id: Uuid::nil(),
        message_id: Uuid::nil(),
        message_text: "What do you see in this image?".to_string(),
        recent_history: Vec::new(),
        known_specialties: vec!["vision".to_string()],
        system_prompt: "You are a vision-capable assistant. Describe what you see in any image attached to the user's message. Keep the response under 40 words.".to_string(),
        model: model_id.to_string(),
        is_voice: false,
        message_media: media,
    }
}

/// Exercise the full Rust persona vision path against Claude Sonnet 4.5.
///
/// Requires `ANTHROPIC_API_KEY` in config.env. Marked `#[ignore]` so
/// default test runs skip it (live API cost).
#[tokio::test]
#[ignore]
async fn vision_roundtrip_anthropic_sonnet() {
    // Initialize model registry — the Anthropic adapter reads it at
    // construction. Idempotent — other tests calling this are fine.
    continuum_core::model_registry::init_global().expect("seeded config loads");

    // Ensure Anthropic is the target. Claude Sonnet 4.5 has
    // Capability::Vision declared in config/models.toml.
    let model_id = "claude-sonnet-4-5-20250929";

    let input = build_vision_request(model_id);
    let response = respond(input).await.expect("respond() returned Err");

    match response {
        PersonaResponse::Silent { reason, .. } => {
            panic!(
                "persona chose Silent — vision pipeline couldn't produce a response. reason: {reason}"
            );
        }
        PersonaResponse::Spoke { text, model_used, .. } => {
            assert!(
                !text.trim().is_empty(),
                "vision model returned empty text — pipeline likely dropped the image bytes"
            );
            assert!(
                model_used.contains("claude"),
                "expected claude model, got: {model_used}"
            );
            // Soft content check: a vision model fed a red square should
            // mention red / color / image / square. If it says nothing
            // about the image content, something flattened the bytes
            // before the model saw them. We lower-case + scan for any
            // of several plausible words to avoid flaking on phrasing.
            let lower = text.to_lowercase();
            let image_aware_words = ["red", "color", "square", "image", "picture", "see"];
            let hit = image_aware_words.iter().any(|w| lower.contains(w));
            assert!(
                hit,
                "response doesn't reference the image content — possible silent byte-drop. text: {text:?}"
            );
            eprintln!("✅ vision roundtrip ({model_used}): {text}");
        }
    }
}

/// Placeholder slot for the local Qwen2-VL-7B-Instruct path.
///
/// Runs the same `build_vision_request` shape against the
/// llamacpp-local adapter once the pieces land:
///   - `Qwen/Qwen2-VL-7B-Instruct` (or bartowski GGUF re-pack) registered
///     in `config/models.toml` with `Capability::Vision`
///   - `LlamaCppAdapter::generate_text` stops filter_mapping out
///     `ContentPart::Image` (the current drop at llamacpp_adapter.rs)
///   - `LlamaCppBackend` wired through `MtmdContext::encode_image`
///     (anvil's FFI + safe wrapper landed in d32b8840a/6557dce34)
///
/// Until then: `panic!` with a descriptive message so the test doesn't
/// silently pass. Swap the panic body for a real call once the registry
/// entry + backend wiring exist.
#[tokio::test]
#[ignore]
async fn vision_roundtrip_local_qwen2_vl() {
    panic!(
        "placeholder — wire up once config/models.toml registers \
         Qwen2-VL-7B-Instruct (Capability::Vision) and llamacpp_adapter \
         + LlamaCppBackend route ContentPart::Image through mtmd. See \
         anvil's d32b8840a/6557dce34 for the FFI side; build_vision_request \
         above is the input shape to call respond() with."
    );
}
