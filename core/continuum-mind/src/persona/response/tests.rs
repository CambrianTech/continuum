#![cfg(test)]
//! Pure-logic tests for the parts that don't require live inference.
//! End-to-end inference test happens through chat-validation.
#![cfg(test)]
use super::*;


// ─── Native multimodal helper tests ─────────────────────────────
//
// build_messages_with_media is the convergence point for sensory
// inputs. These tests pin its contract — no media → text path
// unchanged; media + capability → ContentPart::Image/Audio
// attached to the LAST user message; media without capability →
// text path (the bridge is upstream's job, not ours).

use crate::ai::types::{ContentPart, MessageContent};
use crate::cognition::tool_executor::types::MediaItemLite;
use crate::model_registry::Capability;
use crate::cognition::prompt_assembly::PromptMessage;
use std::collections::HashSet;

fn pm(role: &str, text: &str) -> PromptMessage {
    PromptMessage {
        role: role.to_string(),
        content: text.to_string(),
    }
}

fn img_b64(b64: &str) -> MediaItemLite {
    MediaItemLite {
        item_type: "image".to_string(),
        base64: Some(b64.to_string()),
        mime_type: Some("image/png".to_string()),
        description: None,
    }
}

/// What this catches: empty media short-circuit ever rewriting
/// the message shape into Parts. Without media, the text-only
/// path must remain byte-for-byte identical to before this
/// feature landed — otherwise we silently regress every existing
/// caller.
///
/// Validated 2026-04-21: removed the `if media.is_empty() return`
/// early-exit so the function falls through to the parts-building
/// branch with empty supported_parts; test passes trivially because
/// supported_parts.is_empty() also returns the text path. So the
/// short-circuit is redundant for correctness but reduces work.
/// Stronger mutation: changed the text-path map to wrap in Parts
/// instead of Text; test fails on the assert_eq with MessageContent::Text.
/// Reverted.
#[test]
fn no_media_returns_text_only_messages() {
    let prompt = vec![pm("system", "you are helpful"), pm("user", "hello")];
    let caps = HashSet::new();
    let out = build_messages_with_media(prompt, &[], &caps);
    assert_eq!(out.len(), 2);
    assert!(matches!(out[0].content, MessageContent::Text(_)));
    assert!(matches!(out[1].content, MessageContent::Text(_)));
}

/// What this catches: media present but model lacks Vision —
/// we MUST NOT attach the image. The bridge layer
/// (VisionDescriptionService) is responsible for converting
/// media→text upstream for incapable models; if we attached
/// raw image parts to a text-only model the inference call
/// would fail at the adapter or be silently ignored.
///
/// Validated 2026-04-21: removed the `model_caps.contains(...)`
/// guard from the image branch (always emit ContentPart::Image),
/// test fails because supported_parts is non-empty for a
/// no-capability model and the user message becomes Parts;
/// reverted.
#[test]
fn media_dropped_when_model_lacks_capability() {
    let prompt = vec![pm("user", "describe this")];
    let media = vec![img_b64("AAAA")];
    let caps = HashSet::new(); // model has NO Vision capability
    let out = build_messages_with_media(prompt, &media, &caps);
    assert_eq!(out.len(), 1);
    // New contract (2026-04-22): when model lacks the matching
    // capability, ContentPart::Image bytes MUST NOT attach. The
    // wrapper MAY be MessageContent::Parts(...) containing
    // ContentPart::Text description markers — that's an
    // improvement over silently dropping the attachment, because
    // the model now knows "an image was attached" without being
    // shown bytes it can't process.
    let has_image_bytes = match &out[0].content {
        MessageContent::Text(_) => false,
        MessageContent::Parts(parts) => {
            parts.iter().any(|p| matches!(p, ContentPart::Image { .. }))
        }
    };
    assert!(
        !has_image_bytes,
        "image bytes MUST NOT attach when model lacks Vision capability — got: {:?}",
        out[0].content
    );
}

/// What this catches: with media + Vision capability, the LAST
/// user message MUST become MessageContent::Parts containing
/// the original text + a ContentPart::Image carrying the base64
/// payload. Native sight on natively-capable models is the
/// thesis (per Joel 2026-04-21 + README "Full embodiment");
/// failing this means we silently revert to bridging.
///
/// Validated 2026-04-21: changed Capability::Vision to
/// Capability::AudioInput in the image branch's match, test
/// fails because supported_parts is empty for a Vision-only
/// model and the user message stays as Text; reverted.
#[test]
fn vision_model_receives_native_image_part() {
    let prompt = vec![
        pm("system", "you describe images"),
        pm("user", "what is this?"),
    ];
    let media = vec![img_b64("PNG_BASE64_DATA")];
    let mut caps = HashSet::new();
    caps.insert(Capability::Vision);
    let out = build_messages_with_media(prompt, &media, &caps);
    assert_eq!(out.len(), 2);
    // System message untouched.
    assert!(matches!(out[0].content, MessageContent::Text(_)));
    // User message converted to Parts(text + image).
    let parts = match &out[1].content {
        MessageContent::Parts(p) => p,
        _ => panic!("expected Parts on user message"),
    };
    assert_eq!(parts.len(), 2);
    match &parts[0] {
        ContentPart::Text { text } => assert_eq!(text, "what is this?"),
        _ => panic!("first part should be the original text"),
    }
    match &parts[1] {
        ContentPart::Image { image } => {
            assert_eq!(image.base64.as_deref(), Some("PNG_BASE64_DATA"));
            assert_eq!(image.mime_type.as_deref(), Some("image/png"));
        }
        _ => panic!("second part should be the image"),
    }
}

/// What this catches: media attaches to the LAST user-role
/// message, not the first or to a system message. With
/// multi-turn history the most recent user turn carries the
/// current message + the image the user just shared.
///
/// Validated 2026-04-21: changed `messages.iter().rposition` to
/// `position` (first instead of last), test fails because the
/// FIRST user message gets the image instead of the last;
/// reverted.
#[test]
fn image_attaches_to_last_user_turn_not_first() {
    let prompt = vec![
        pm("user", "earlier turn"),
        pm("assistant", "earlier reply"),
        pm("user", "current turn"),
    ];
    let media = vec![img_b64("X")];
    let mut caps = HashSet::new();
    caps.insert(Capability::Vision);
    let out = build_messages_with_media(prompt, &media, &caps);
    // First user message stays text.
    match &out[0].content {
        MessageContent::Text(t) => assert_eq!(t, "earlier turn"),
        _ => panic!("first user turn should remain text"),
    }
    // Last user message becomes Parts.
    match &out[2].content {
        MessageContent::Parts(p) => {
            assert!(
                p.iter().any(|x| matches!(x, ContentPart::Image { .. })),
                "last user turn should carry the image"
            );
        }
        _ => panic!("last user turn should be Parts"),
    }
}

/// What this catches: audio attachment requires the AudioInput
/// capability — Vision alone does NOT permit audio. Each modality
/// has its own capability gate; no cross-bleed.
///
/// Validated 2026-04-21: changed `Capability::AudioInput` to
/// `Capability::Vision` in the audio match arm, test fails
/// because vision-only model wrongly receives audio; reverted.
#[test]
fn audio_requires_audio_input_capability() {
    let prompt = vec![pm("user", "what did i say")];
    let audio = MediaItemLite {
        item_type: "audio".to_string(),
        base64: Some("WAV_DATA".to_string()),
        mime_type: Some("audio/wav".to_string()),
        description: None,
    };
    let mut vision_only = HashSet::new();
    vision_only.insert(Capability::Vision);
    let out = build_messages_with_media(prompt.clone(), &[audio.clone()], &vision_only);
    // Vision-only model: audio bytes MUST NOT attach. Wrapper MAY
    // be Parts(Text-marker) per the new policy contract — what
    // matters is no ContentPart::Audio carrying real bytes.
    let has_audio_bytes = match &out[0].content {
        MessageContent::Text(_) => false,
        MessageContent::Parts(parts) => {
            parts.iter().any(|p| matches!(p, ContentPart::Audio { .. }))
        }
    };
    assert!(
        !has_audio_bytes,
        "audio bytes MUST NOT attach when model lacks AudioInput capability — got: {:?}",
        out[0].content
    );

    let mut audio_capable = HashSet::new();
    audio_capable.insert(Capability::AudioInput);
    let out = build_messages_with_media(prompt, &[audio], &audio_capable);
    // Audio-capable model: audio attaches.
    match &out[0].content {
        MessageContent::Parts(p) => {
            assert!(p.iter().any(|x| matches!(x, ContentPart::Audio { .. })));
        }
        _ => panic!("audio-capable model should receive Parts"),
    }
}
