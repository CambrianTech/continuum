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
//! **`respond()` is local-first by design.** Line 370 of
//! `persona/response.rs` hardcodes `registry.select(Some("local"),
//! Some(&input.model), InferenceDevice::Gpu)` — the Rust persona
//! pipeline will NOT route to Anthropic / OpenAI / any cloud provider
//! even if those are the only adapters registered. That's deliberate,
//! matches "native multimodal or nothing" (2026-04-21), and means
//! this test can only go green against a LOCAL vision-capable model.
//!
//! Which also means: until anvil's in-flight work lands
//! (the Rust catalog (catalog.rs) registers Qwen2-VL-7B with `Capability::Vision`
//! + `LlamaCppAdapter::generate_text` stops filter-mapping out
//! `ContentPart::Image` + `LlamaCppBackend` routes images through
//! mtmd — FFI side already in d32b8840a/6557dce34), the test stays
//! ignored. When it runs, it proves the pipeline in full — NOT
//! whether the forged vision model is accurate.
//!
//! Run explicitly once the local wiring is in:
//!
//!   cargo test --test vision_integration -- --ignored --nocapture

use continuum_core::cognition::tool_executor::types::MediaItemLite;
use continuum_core::persona::response::{respond, PersonaResponse, RespondInput};
use continuum_core::persona::turn_context::TurnContext;
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
        description: None,
    }];

    let mut caps = std::collections::HashSet::new();
    caps.insert(continuum_core::model_registry::Capability::Vision);

    RespondInput {
        persona: continuum_core::cognition::PersonaSlot {
            persona_id: Uuid::nil(),
            specialty: "vision".to_string(),
            display_name: "VisionTestPersona".to_string(),
        },
        // Per-turn shared context (continuum#1206). Room-level fields
        // moved off RespondInput into Arc<TurnContext>; constructing
        // here mirrors the projection done by `build_respond_input`
        // for the live IPC path.
        turn_context: TurnContext::arc(
            Uuid::nil(),
            Vec::new(),
            vec!["vision".to_string()],
        ),
        message_id: Uuid::nil(),
        message_text: "What do you see in this image?".to_string(),
        other_persona_names: Vec::new(),
        system_prompt: "You are a vision-capable assistant. Describe what you see in any image attached to the user's message. Keep the response under 40 words.".to_string(),
        model: model_id.to_string(),
        is_voice: false,
        message_media: media,
        // Vision capability — caller-declared, no registry lookup.
        capabilities: caps,
        recalled_engrams: Vec::new(),
        room_roster: Vec::new(),
        room_doctrine: None,
    }
}

/// Exercise the full Rust persona vision path against a local
/// vision-capable model. Runs once anvil's pieces land:
///
///   - `Qwen/Qwen2-VL-7B-Instruct` (or bartowski GGUF re-pack)
///     registered in the Rust catalog (catalog.rs) with `Capability::Vision`
///   - `LlamaCppAdapter::generate_text` stops filter_mapping out
///     `ContentPart::Image` (the current drop at llamacpp_adapter.rs)
///   - `LlamaCppBackend` wired through `MtmdContext::encode_image`
///     (anvil's FFI + safe wrapper landed in d32b8840a/6557dce34)
///
/// Until then: `panic!` with a descriptive message so the test doesn't
/// silently pass. Swap the panic body for the real flow once registry
/// + adapter + backend all expose the local Vision path.
#[tokio::test]
#[ignore]
async fn vision_roundtrip_local_qwen2_vl() {
    use std::path::PathBuf;

    continuum_core::model_registry::init_global().expect("seeded config loads");

    // The catalog row id we registered (anvil 2026-04-21). Memento's earlier
    // draft pointed at a forge name that doesn't exist yet —
    // `continuum-ai/qwen2-vl-7b-forged-GGUF` is the eventual forged
    // variant; until that bake exists, the bartowski Q4_K_M GGUF + its
    // sibling mmproj are the test target.
    let model_id = "qwen2-vl-7b-instruct";

    // Sanity: bail early with a specific message rather than letting
    // respond()'s generic "no adapter supports model" catch us.
    let reg = continuum_core::model_registry::global();
    let model_meta = reg.model(model_id).unwrap_or_else(|| {
        panic!(
            "'{model_id}' not in the Rust catalog (catalog.rs). Add a Vision-capable \
             entry (gguf_hint + mmproj + Capability::Vision). FFI side \
             shipped in d32b8840a / 6557dce34, dedup fix in f098c4331; \
             this test is the persona-pipeline end-to-end proof."
        )
    });

    // Skip cleanly when the GGUF/mmproj aren't on disk — same pattern as
    // tests/llamacpp_vision_integration.rs. CI hosts won't have these
    // 6 GB files; dev machines do.
    let model_path = model_meta.gguf_local_path.clone().expect(
        "qwen2-vl-7b-instruct should declare gguf_local_path in the Rust catalog (catalog.rs)",
    );
    if !model_path.exists() {
        eprintln!(
            "[vision-int] skipping — Qwen2-VL-7B GGUF not at {}. Pull via \
             `hf download bartowski/Qwen2-VL-7B-Instruct-GGUF \
             Qwen2-VL-7B-Instruct-Q4_K_M.gguf --local-dir ~/models/qwen2-vl-7b` \
             then re-run.",
            model_path.display()
        );
        return;
    }
    let _ = PathBuf::new(); // silence unused-import warn under skip path

    // Register the in-process LlamaCppAdapter into the global adapter
    // registry — production wires it through AIProviderModule on server
    // startup; tests need to do the same step explicitly. Without this,
    // respond() returns "No AI providers configured."
    {
        use continuum_core::ai::adapter::AIProviderAdapter;
        let registry_arc = continuum_core::modules::ai_provider::global_registry();
        let mut registry = registry_arc.write().await;
        let adapter: std::sync::Arc<dyn AIProviderAdapter> = std::sync::Arc::new(
            continuum_core::inference::llamacpp_adapter::LlamaCppAdapter::with_model_id(
                model_path.clone(),
                model_id.to_string(),
            ),
        );
        // Priority 0 = highest — beats DMR if it's also registered.
        registry.register(adapter, 0);
    }

    let input = build_vision_request(model_id);
    let response = respond(input).await.expect("respond() returned Err");

    match response {
        PersonaResponse::Silent { reason, .. } => {
            panic!(
                "persona chose Silent — local vision pipeline couldn't produce a response. reason: {reason}"
            );
        }
        PersonaResponse::Spoke {
            text, model_used, ..
        } => {
            assert!(
                !text.trim().is_empty(),
                "vision model returned empty text — pipeline likely dropped the image bytes"
            );
            // Soft content check: a vision model fed a red square should
            // mention red / color / image / square / small. Silent
            // byte-drop would produce text with none of these.
            let lower = text.to_lowercase();
            let image_aware_words = ["red", "color", "square", "image", "picture", "see", "small"];
            let hit = image_aware_words.iter().any(|w| lower.contains(w));
            assert!(
                hit,
                "response doesn't reference the image content — possible silent byte-drop. text: {text:?}"
            );
            eprintln!("✅ local vision roundtrip ({model_used}): {text}");
        }
    }
}
