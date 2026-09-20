//! No-app replay test: prove that real captured persona-respond
//! inputs produce the right message shape WITHOUT loading any model,
//! booting the orchestrator, or touching Metal.
//!
//! # The bug class this catches
//!
//! 2026-04-22: Vision AI received image bytes correctly (fixture had
//! `messageMedia: 1`, `capabilities: ['vision', ...]`), but no
//! `ContentPart::Image` ever reached the inference adapter. The
//! capability lookup in `respond()` was returning empty mid-flight,
//! so `build_messages_with_media` saw no Vision cap and demoted the
//! image to a text marker. Vision encoder never fired.
//!
//! That bug took hours to localize because we only had logic-layer
//! unit tests (mocked inputs) and end-to-end live tests (boot the
//! whole world, send a chat message, eyeball the chat reply).
//! The middle was missing — a test that takes a REAL captured input
//! shape from disk and runs the message-build seam against it.
//!
//! # What this test does
//!
//! Walks every fixture in `~/.continuum/fixtures/persona-respond/`,
//! parses the `rust_request` payload (input that was actually sent
//! across the IPC in a live session), reconstructs the exact
//! arguments to `build_messages_with_media`, calls it, and asserts:
//!
//!   - If the fixture had `messageMedia` containing image items AND
//!     `capabilities` included `vision`, then the assembled output
//!     MUST contain a `ContentPart::Image` whose base64 matches one
//!     of the input items. Failing means we silently dropped bytes
//!     between IPC arrival and the model.
//!
//!   - If the fixture had `messageMedia` containing image items but
//!     `capabilities` did NOT include `vision`, then the assembled
//!     output MUST NOT contain any `ContentPart::Image` (text
//!     description marker is allowed). Failing means we routed bytes
//!     to a text-only model — wastes the encoder, may crash adapters
//!     that don't expect Parts.
//!
//!   - If the fixture had no `messageMedia`, the output MUST be all
//!     plain text (no Parts). Failing means we synthesized an
//!     attachment from nothing.
//!
//! # Why this is the right layer
//!
//! - **No model load**: the function under test is a pure
//!   transformation — `(prompt_messages, media, caps) -> messages`.
//!   Runs in microseconds.
//! - **Real input shapes**: fixtures are captured from live
//!   production traffic. Anything weird about real RAG outputs,
//!   real media payloads, real capability sets — present here.
//! - **Deterministic**: byte-identical inputs produce byte-identical
//!   outputs. Failure means a real regression, not flake.
//!
//! # Run
//!
//! ```bash
//! # Default — runs against whatever's in ~/.continuum/fixtures/persona-respond/
//! cargo test --release --features metal,accelerate \
//!   --test fixture_assembly_replay -- --nocapture
//! ```
//!
//! Skips cleanly when the fixture dir is empty (CI hosts).

use continuum_core::ai::types::{ContentPart, MessageContent};
use continuum_core::cognition::tool_executor::types::MediaItemLite;
use continuum_core::model_registry::Capability;
use continuum_core::persona::cognition_io::{
    build_respond_input, PersonaContext, Signal, SignalKind, SignalOriginator,
};
use continuum_core::persona::prompt_assembly::PromptMessage;
use continuum_core::persona::response::build_messages_with_media;
use serde_json::Value;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Once;
use uuid::Uuid;

/// Read every fixture in the standard dir. Returns empty vec if the
/// dir doesn't exist (CI / fresh dev box).
fn load_all_fixtures() -> Vec<(PathBuf, Value)> {
    let home = std::env::var("HOME").expect("HOME set");
    let dir = PathBuf::from(home).join(".continuum/fixtures/persona-respond");
    if !dir.exists() {
        return Vec::new();
    }
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let raw = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let v: Value = match serde_json::from_str(&raw) {
            Ok(v) => v,
            Err(_) => continue, // half-written or non-JSON; skip silently
        };
        out.push((path, v));
    }
    out
}

/// Pull `MediaItemLite[]` from the wire shape (`rust_request.messageMedia`,
/// camelCase keys per the TS mixin). Returns empty vec when absent or
/// malformed — same defensive parsing the IPC handler does.
fn extract_media(rust_request: &Value) -> Vec<MediaItemLite> {
    // Post-IPC-reshape shape: rust_request.signal.media (nested under signal).
    // Pre-reshape (legacy) shape: rust_request.messageMedia (flat).
    // Read both so the replay handles fixtures captured before AND after the
    // 2026-04-22 cognition/respond IPC reshape (commit 983d30102).
    let arr = match rust_request
        .get("signal")
        .and_then(|s| s.get("media"))
        .and_then(|v| v.as_array())
        .or_else(|| rust_request.get("messageMedia").and_then(|v| v.as_array()))
    {
        Some(a) => a,
        None => return Vec::new(),
    };
    arr.iter()
        .filter_map(|item| {
            let item_type = item
                .get("itemType")
                .or_else(|| item.get("item_type"))?
                .as_str()?
                .to_string();
            let base64 = item
                .get("base64")
                .and_then(|v| v.as_str())
                .map(String::from);
            let mime_type = item
                .get("mimeType")
                .or_else(|| item.get("mime_type"))
                .and_then(|v| v.as_str())
                .map(String::from);
            let description = item
                .get("description")
                .and_then(|v| v.as_str())
                .map(String::from);
            Some(MediaItemLite {
                item_type,
                base64,
                mime_type,
                description,
            })
        })
        .collect()
}

/// Parse the fixture's `rust_request.capabilities` (kebab-case
/// strings) into the `Capability` HashSet the message builder
/// expects. Same flow the IPC handler uses.
fn extract_capabilities(rust_request: &Value) -> HashSet<Capability> {
    // Post-IPC-reshape: rust_request.personaContext.capabilities.
    // Pre-reshape (legacy): rust_request.capabilities.
    // Read both shapes — same reasoning as extract_media().
    let arr = match rust_request
        .get("personaContext")
        .and_then(|p| p.get("capabilities"))
        .and_then(|v| v.as_array())
        .or_else(|| rust_request.get("capabilities").and_then(|v| v.as_array()))
    {
        Some(a) => a,
        None => return HashSet::new(),
    };
    arr.iter()
        .filter_map(|s| s.as_str())
        .filter_map(|s| serde_json::from_value(Value::String(s.to_string())).ok())
        .collect()
}

/// Reconstruct a minimal PromptMessage list from the fixture's
/// rust_request. The exact assembled prompt is built inside
/// `respond()` from system_prompt + recent_history + the trigger;
/// for this test we only need the LAST user message (where media
/// attaches per build_messages_with_media's contract). The
/// recent_history doesn't carry media itself, so its precise
/// reconstruction isn't needed to test the byte-attachment seam.
fn synth_prompt_messages(rust_request: &Value) -> Vec<PromptMessage> {
    // New shape (post-rip): rust_request.signal.text
    // Legacy shape: rust_request.messageText
    let user_text = rust_request
        .get("signal")
        .and_then(|s| s.get("text"))
        .and_then(|v| v.as_str())
        .or_else(|| rust_request.get("messageText").and_then(|v| v.as_str()))
        .unwrap_or("")
        .to_string();
    vec![PromptMessage {
        role: "user".to_string(),
        content: user_text,
    }]
}

/// Bridge from a legacy-shape fixture's `rust_request` (flat field
/// layout from before the Recipe IPC) to a `Signal + PersonaContext`
/// pair the new path consumes. Test-only — production code never
/// sees the legacy shape (the IPC handler now requires the new wire
/// shape; old shape returns a JSON-parse error from the IPC).
///
/// Captured fixtures are too valuable to discard (they're the
/// regression corpus), so this helper translates them into the new
/// shape FOR THE TEST. As fixtures are re-captured in the new shape
/// post-rip, this helper goes away.
fn signal_and_ctx_from_legacy_fixture(
    rust_request: &Value,
) -> Result<(Signal, PersonaContext), String> {
    // New shape (post-IPC-reshape commit 983d30102): rust_request already
    // has `signal` + `personaContext` as nested objects matching the wire
    // shape exactly. Deserialize directly. No reconstruction needed.
    if let (Some(signal_json), Some(ctx_json)) = (
        rust_request.get("signal"),
        rust_request.get("personaContext"),
    ) {
        let signal: Signal = serde_json::from_value(signal_json.clone())
            .map_err(|e| format!("new-shape signal deserialize failed: {e}"))?;
        let ctx: PersonaContext = serde_json::from_value(ctx_json.clone())
            .map_err(|e| format!("new-shape personaContext deserialize failed: {e}"))?;
        return Ok((signal, ctx));
    }

    // Legacy shape (pre-rip): flat fields on rust_request — reconstruct
    // Signal + PersonaContext below. Once all captured fixtures are
    // re-recorded in the new shape, this branch + the helper closures
    // below go away.
    let get_str = |key: &str| -> Option<String> {
        rust_request
            .get(key)
            .and_then(|v| v.as_str())
            .map(String::from)
    };
    let get_uuid = |key: &str| -> Result<Uuid, String> {
        let s = get_str(key).ok_or_else(|| format!("missing field '{key}'"))?;
        Uuid::parse_str(&s).map_err(|e| format!("invalid uuid for '{key}': {e}"))
    };

    let persona_id = get_uuid("personaId")?;
    let room_id = get_uuid("roomId")?;
    let message_id = get_uuid("messageId")?;
    let display_name = get_str("personaName").unwrap_or_else(|| "AI".to_string());
    let specialty = get_str("specialty").unwrap_or_else(|| "general".to_string());
    let model = get_str("model").ok_or_else(|| "missing 'model'".to_string())?;
    let system_prompt = get_str("systemPrompt").unwrap_or_default();
    let message_text = get_str("messageText").unwrap_or_default();
    let is_voice = rust_request
        .get("isVoice")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let known_specialties: Vec<String> = rust_request
        .get("knownSpecialties")
        .and_then(|v| serde_json::from_value::<Vec<String>>(v.clone()).ok())
        .unwrap_or_else(|| vec![specialty.clone()]);
    let recent_history: Vec<continuum_core::cognition::RecentMessage> = rust_request
        .get("recentHistory")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    let id = item.get("id")?.as_str()?.parse::<Uuid>().ok()?;
                    let sender_name = item
                        .get("senderName")
                        .or_else(|| item.get("sender_name"))?
                        .as_str()?
                        .to_string();
                    let text = item.get("text")?.as_str()?.to_string();
                    Some(continuum_core::cognition::RecentMessage {
                        id,
                        sender_name,
                        text,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let media = extract_media(rust_request);
    let capabilities: Vec<Capability> = extract_capabilities(rust_request).into_iter().collect();

    let signal = Signal {
        kind: SignalKind::ChatMessage,
        text: message_text,
        media,
        originator: SignalOriginator::User {
            user_id: Uuid::nil(),
        },
        timestamp_ms: 0,
        message_id: Some(message_id),
    };
    let ctx = PersonaContext {
        persona_id,
        display_name,
        specialty,
        model,
        capabilities,
        system_prompt,
        recent_history,
        known_specialties,
        other_persona_names: Vec::new(),
        room_id: Some(room_id),
        is_voice,
    };
    Ok((signal, ctx))
}

#[test]
fn fixtures_replay_through_message_builder() {
    let fixtures = load_all_fixtures();
    if fixtures.is_empty() {
        eprintln!(
            "[fixture-replay] no fixtures at ~/.continuum/fixtures/persona-respond/ — \
             run the live system first to capture some, then re-run this test."
        );
        return;
    }

    let mut total = 0usize;
    let mut vision_with_image_ok = 0usize;
    let mut text_only_with_image_ok = 0usize;
    let mut no_media_ok = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for (path, fixture) in &fixtures {
        let Some(rust_request) = fixture.get("rust_request") else {
            continue;
        };
        total += 1;

        let media = extract_media(rust_request);
        let caps = extract_capabilities(rust_request);
        let prompt = synth_prompt_messages(rust_request);
        let out = build_messages_with_media(prompt, &media, &caps);

        let last = out
            .last()
            .expect("builder always returns at least one message");
        let image_parts: Vec<&ContentPart> = match &last.content {
            MessageContent::Text(_) => Vec::new(),
            MessageContent::Parts(parts) => parts
                .iter()
                .filter(|p| matches!(p, ContentPart::Image { .. }))
                .collect(),
        };

        let has_image_input = media.iter().any(|m| m.item_type == "image");
        let has_vision_cap = caps.contains(&Capability::Vision);
        let fname = path.file_name().unwrap().to_string_lossy().into_owned();

        if has_image_input && has_vision_cap {
            // CONTRACT: at least one ContentPart::Image must reach
            // the model. Empty here = silent drop bug (today's bug).
            if image_parts.is_empty() {
                failures.push(format!(
                    "[{fname}] image input + Vision cap but ZERO ContentPart::Image \
                     emitted — silent encoder bypass. Input had {} media items, \
                     cap set: {:?}",
                    media.len(),
                    caps
                ));
            } else {
                // Policy bound: AT MOST ONE image ever attaches as bytes
                // (the AtMostOneLatest rule, persona/media_policy.rs).
                if image_parts.len() > 1 {
                    failures.push(format!(
                        "[{fname}] {} ContentPart::Image entries emitted from {} \
                         input items — AtMostOneLatest policy violated, multi-encoder \
                         hazard. Caps: {:?}",
                        image_parts.len(),
                        media.len(),
                        caps
                    ));
                } else {
                    vision_with_image_ok += 1;
                }
            }
        } else if has_image_input && !has_vision_cap {
            // CONTRACT: image bytes MUST NOT reach a non-vision
            // model (wastes encoder, may crash text-only adapters).
            // Text marker is the expected fallback.
            if !image_parts.is_empty() {
                failures.push(format!(
                    "[{fname}] image input but NO Vision cap — yet {} \
                     ContentPart::Image emitted. Bytes routed to text-only model. \
                     Caps: {:?}",
                    image_parts.len(),
                    caps
                ));
            } else {
                text_only_with_image_ok += 1;
            }
        } else if !has_image_input {
            // CONTRACT: no input media, no synthesized output media.
            if !image_parts.is_empty() {
                failures.push(format!(
                    "[{fname}] no image in input but {} ContentPart::Image \
                     emitted — synthesized from nothing.",
                    image_parts.len()
                ));
            } else {
                no_media_ok += 1;
            }
        }
    }

    eprintln!(
        "[fixture-replay] processed {} fixtures: vision+image OK={}, \
         text+image-as-marker OK={}, no-media OK={}, failures={}",
        total,
        vision_with_image_ok,
        text_only_with_image_ok,
        no_media_ok,
        failures.len()
    );

    if !failures.is_empty() {
        for f in &failures {
            eprintln!("  ✗ {f}");
        }
        panic!(
            "{} fixture(s) violated the message-builder contract. \
             Each violation is a regression in the multimodal IPC seam — \
             real captured prod input shape, real expected output shape, real broken.",
            failures.len()
        );
    }
}

// ─── Real-model behavior replay ──────────────────────────────────────────
//
// Above test proves the message SHAPE is correct. This one proves the
// MODEL actually sees and describes the image — the behavior question
// "did the AI receive bytes and produce vision-grounded text?" that
// no shape test can answer.
//
// Same fixtures, same input shape, same `respond_input_from_value`
// transformation the live IPC handler uses. Then the SAME `respond()`
// the live system calls. Real qwen2-vl model. Real Metal. Real bytes.
// Asserts the response text contains visual-content words — empty
// or generic response = encoder didn't fire.
//
// Marked `#[ignore]` because it loads ~5GB of model into Metal and
// takes ~10s per fixture. Dev machines run it via:
//
//   cargo test --release --features metal,accelerate \
//     --test fixture_assembly_replay -- --ignored --nocapture
//
// CI hosts won't have qwen2-vl-7b on disk and skip via the
// model-missing branch. The cheap shape test above STAYS in CI; this
// one is the heavy "did the model REALLY see it" gate that runs
// locally before we ship vision changes.

static REGISTER_ONCE: Once = Once::new();

async fn ensure_llamacpp_qwen2vl_registered() -> Option<()> {
    use continuum_core::ai::AIProviderAdapter;
    use continuum_core::inference::{LlamaCppAdapter, LLAMACPP_PROVIDER_ID};

    if REGISTER_ONCE.is_completed() {
        return Some(());
    }

    continuum_core::model_registry::init_global().expect("model_registry::init_global");
    let registry = continuum_core::model_registry::global();
    let model_meta = registry.model("qwen2-vl-7b-instruct").or_else(|| {
        eprintln!(
            "[fixture-replay-behavior] 'qwen2-vl-7b-instruct' not in the Rust catalog (catalog.rs)"
        );
        None
    })?;
    let gguf = model_meta.gguf_local_path.as_ref()?.clone();
    if !gguf.exists() {
        eprintln!(
            "[fixture-replay-behavior] qwen2-vl GGUF not at {} — skipping. \
             Pull via `hf download bartowski/Qwen2-VL-7B-Instruct-GGUF \
             Qwen2-VL-7B-Instruct-Q4_K_M.gguf --local-dir ~/models/qwen2-vl-7b`",
            gguf.display()
        );
        return None;
    }

    REGISTER_ONCE.call_once(|| {});
    let registry_arc = continuum_core::modules::ai_provider::global_registry();
    let mut registry_lock = registry_arc.write().await;

    // Mirror production registration: walk every llamacpp-local row whose
    // GGUF is on disk and register an adapter. This is what
    // `register_adapters` does at boot — DO NOT cherry-pick just qwen2-vl,
    // because if production loads N adapters and the test loads 1, the
    // test isn't reproducing prod conditions.
    for m in registry.models_for_provider(LLAMACPP_PROVIDER_ID) {
        let Some(gguf_path) = m.gguf_local_path.as_ref() else {
            continue;
        };
        if !gguf_path.exists() {
            continue;
        }
        let mut adapter: Box<dyn AIProviderAdapter> = Box::new(LlamaCppAdapter::with_model_id(
            gguf_path.clone(),
            m.id.clone(),
        ));
        adapter
            .initialize()
            .await
            .unwrap_or_else(|e| panic!("init failed for {}: {e}", m.id));
        registry_lock.register(std::sync::Arc::from(adapter), 0);
        eprintln!("[fixture-replay-behavior] registered adapter '{}'", m.id);
    }
    Some(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "loads real qwen2-vl GGUF (~5GB Metal) + ~10s/fixture; run via --ignored --nocapture"]
async fn vision_fixture_describes_image_via_real_model() {
    use continuum_core::persona::response::{respond, PersonaResponse};

    if ensure_llamacpp_qwen2vl_registered().await.is_none() {
        return;
    }

    let fixtures = load_all_fixtures();
    if fixtures.is_empty() {
        eprintln!(
            "[fixture-replay-behavior] no fixtures — capture some via the \
             live system first, then re-run."
        );
        return;
    }

    // Find every fixture that's a real vision+image capture: image media
    // present, capabilities include "vision", AND base64 payload is
    // non-empty (resolved by PRG, not just a metadata stub). These are
    // the captures where production WOULD have called the vision encoder.
    let vision_image_fixtures: Vec<&(PathBuf, Value)> = fixtures
        .iter()
        .filter(|(_, fixture)| {
            let Some(rust_request) = fixture.get("rust_request") else {
                return false;
            };
            let media = extract_media(rust_request);
            let caps = extract_capabilities(rust_request);
            let has_real_image = media.iter().any(|m| {
                m.item_type == "image"
                    && m.base64.as_deref().map(|b| !b.is_empty()).unwrap_or(false)
            });
            has_real_image && caps.contains(&Capability::Vision)
        })
        .collect();

    if vision_image_fixtures.is_empty() {
        eprintln!(
            "[fixture-replay-behavior] no fixtures with image+Vision-cap+real-bytes — \
             send an image to a vision-capable persona via chat, then re-run."
        );
        return;
    }

    eprintln!(
        "[fixture-replay-behavior] {} vision+image fixture(s) to replay through real qwen2-vl",
        vision_image_fixtures.len()
    );

    let visual_signal_words: &[&str] = &[
        "image",
        "photo",
        "picture",
        "shows",
        "see",
        "depicts",
        "screenshot",
        "color",
        "background",
        "appears",
        "contains",
        "object",
        "red",
        "blue",
        "green",
        "yellow",
        "black",
        "white",
        "brick",
        "cat",
        "dog",
        "person",
        "wallet",
    ];

    let mut passed = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for (path, fixture) in &vision_image_fixtures {
        let fname = path.file_name().unwrap().to_string_lossy().into_owned();
        let rust_request = fixture.get("rust_request").unwrap();

        // Build Signal + PersonaContext from the captured fixture (legacy
        // shape — fixtures predate the post-rip IPC), then run the
        // chat-shaped projection. Media-bearing signal + vision-capable
        // persona = same effective path the IPC handler takes for vision
        // input. The test gate is "the projection + respond() pair
        // produces working output for the same fixtures the live IPC
        // exercises."
        let (signal, ctx) = match signal_and_ctx_from_legacy_fixture(rust_request) {
            Ok(pair) => pair,
            Err(e) => {
                failures.push(format!(
                    "[{fname}] could not build Signal+PersonaContext: {e}"
                ));
                continue;
            }
        };
        let input = match build_respond_input(&signal, &ctx) {
            Ok(i) => i,
            Err(e) => {
                failures.push(format!("[{fname}] build_respond_input failed: {e}"));
                continue;
            }
        };
        let model_name = input.model.clone();
        let media_summary: Vec<String> = input
            .message_media
            .iter()
            .map(|m| {
                format!(
                    "{}({}b)",
                    m.item_type,
                    m.base64.as_deref().map(|s| s.len()).unwrap_or(0)
                )
            })
            .collect();
        eprintln!(
            "[fixture-replay-behavior] >>> {fname} model={model_name} media=[{}]",
            media_summary.join(",")
        );

        let response_start = std::time::Instant::now();
        let response = match respond(input).await {
            Ok(r) => r,
            Err(e) => {
                failures.push(format!("[{fname}] respond() returned Err: {e}"));
                continue;
            }
        };
        let response_ms = response_start.elapsed().as_millis();

        match response {
            PersonaResponse::Silent { reason, .. } => {
                failures.push(format!(
                    "[{fname}] persona chose Silent — vision pipeline never produced \
                     a response. reason: {reason}"
                ));
            }
            PersonaResponse::Spoke {
                text, model_used, ..
            } => {
                let trimmed = text.trim();
                if trimmed.len() < 30 {
                    failures.push(format!(
                        "[{fname}] response too short ({} chars) — encoder likely didn't \
                         process the image. model_used={model_used}, text={text:?}",
                        trimmed.len()
                    ));
                    continue;
                }
                let lower = text.to_lowercase();
                let hit = visual_signal_words.iter().any(|w| lower.contains(w));
                if !hit {
                    failures.push(format!(
                        "[{fname}] response has no visual-content words (encoder \
                         likely bypassed). model_used={model_used}, response={text:?}"
                    ));
                    continue;
                }
                eprintln!(
                    "[fixture-replay-behavior] ✅ {fname} ({}ms): {}",
                    response_ms,
                    trimmed.chars().take(140).collect::<String>()
                );
                passed += 1;
            }
        }
    }

    eprintln!(
        "[fixture-replay-behavior] result: {passed} passed, {} failed (of {} vision+image fixtures)",
        failures.len(),
        vision_image_fixtures.len()
    );

    if !failures.is_empty() {
        for f in &failures {
            eprintln!("  ✗ {f}");
        }
        panic!(
            "{} vision+image fixture(s) failed real-model replay. The encoder did not \
             produce vision-grounded text from bytes that DID arrive at Rust. \
             This is the bug Joel hit 2026-04-22 — the seam between IPC and adapter.",
            failures.len()
        );
    }
}
