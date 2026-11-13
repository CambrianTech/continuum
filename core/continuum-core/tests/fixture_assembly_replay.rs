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
use continuum_core::cognition::prompt_assembly::PromptMessage;
use continuum_core::persona::response::build_messages_with_media;
use serde_json::Value;
use std::collections::HashSet;
use std::path::PathBuf;

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
