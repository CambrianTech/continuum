//! Media-attachment policy for the persona response path.
//!
//! Decides, for the media items attached to a single inference call,
//! WHICH item attaches as raw bytes and which become text descriptions.
//! Separated from `build_messages_with_media` so the rule has a name
//! and a test surface, instead of being a `for` loop someone can
//! quietly delete.
//!
//! # The rule (`AtMostOneLatest`)
//!
//! At most ONE media item per inference call attaches as raw bytes —
//! the LATEST item the model can natively consume. Everything else
//! becomes a text description marker (using the upstream sensory
//! bridge's `description` if present, else a do-not-speculate marker).
//!
//! ## Why
//!
//! Each `ContentPart::Image` / `ContentPart::Audio` carrying real bytes
//! triggers a per-call multimodal context allocation in the inference
//! backend (~2 GB Metal alloc for qwen2-vl per call). Two simultaneous
//! image attachments = two concurrent encoder ops = Metal pipeline
//! pressure that has bricked the host (verified empirically 2026-04-22:
//! mouse-frozen, hard reset). Capping at one byte-attachment per
//! inference is the architectural guard.
//!
//! Joel's standing rule: "i would never let more than ONE message
//! deliver an image or tell the ais the image link" (2026-04-22).

use crate::cognition::tool_executor::types::MediaItemLite;
use crate::model_registry::Capability;
use std::collections::HashSet;

/// Policy for deciding how to expose attached media to the model.
#[derive(Debug, Clone, Copy)]
pub enum MediaPolicy {
    /// Production default. The latest natively-supported item attaches
    /// as bytes; everything else becomes a description.
    AtMostOneLatest,
    /// Force every item into the description path, even if the model
    /// is natively capable. Useful for forced-text-only test scenarios
    /// and debugging the description-marker path in isolation.
    AllAsDescriptions,
}

/// Outcome of applying a `MediaPolicy` to a media slice. Borrows the
/// items so we don't clone base64 payloads (potentially MBs each).
#[derive(Debug)]
pub struct MediaPlan<'a> {
    /// The single media item that attaches to the model as raw bytes,
    /// or `None` when the model has no native capability for any item
    /// in the slice (text-only persona path) or the slice is empty.
    pub attachable: Option<&'a MediaItemLite>,
    /// All items that did NOT win the byte slot — must be rendered
    /// as text descriptions. Order: source order, NOT reversed.
    pub descriptions: Vec<&'a MediaItemLite>,
}

impl<'a> MediaPlan<'a> {
    /// Empty plan: no attachable, no descriptions. Used when the
    /// caller passed an empty media slice.
    pub fn empty() -> Self {
        Self {
            attachable: None,
            descriptions: Vec::new(),
        }
    }
}

impl MediaPolicy {
    /// Apply this policy to a slice of media items, returning a
    /// `MediaPlan` the caller can render into `ContentPart`s.
    pub fn plan<'a>(
        &self,
        media: &'a [MediaItemLite],
        model_caps: &HashSet<Capability>,
    ) -> MediaPlan<'a> {
        if media.is_empty() {
            return MediaPlan::empty();
        }
        match self {
            MediaPolicy::AllAsDescriptions => MediaPlan {
                attachable: None,
                descriptions: media.iter().collect(),
            },
            MediaPolicy::AtMostOneLatest => self.plan_at_most_one_latest(media, model_caps),
        }
    }

    /// Walk media in REVERSE to find the latest natively-supported item;
    /// that item is the attachable. Everything else (older items, or
    /// items the model can't natively consume) becomes a description.
    fn plan_at_most_one_latest<'a>(
        &self,
        media: &'a [MediaItemLite],
        model_caps: &HashSet<Capability>,
    ) -> MediaPlan<'a> {
        let attachable_idx = media
            .iter()
            .enumerate()
            .rev()
            .find(|(_, m)| is_natively_supported(m, model_caps))
            .map(|(i, _)| i);

        let attachable = attachable_idx.map(|i| &media[i]);
        let descriptions = media
            .iter()
            .enumerate()
            .filter_map(|(i, m)| {
                if Some(i) == attachable_idx {
                    None
                } else {
                    Some(m)
                }
            })
            .collect();

        MediaPlan {
            attachable,
            descriptions,
        }
    }
}

/// Can this model natively consume this media item as raw bytes?
/// Image needs `Capability::Vision`, audio needs `Capability::AudioInput`.
/// Other types (video, etc.) always fall through to the description path
/// — we don't ship a video-byte path yet.
fn is_natively_supported(m: &MediaItemLite, caps: &HashSet<Capability>) -> bool {
    match m.item_type.as_str() {
        "image" => caps.contains(&Capability::Vision),
        "audio" => caps.contains(&Capability::AudioInput),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    //! Pure-function tests for the policy. No I/O, no async, no
    //! inference. The only thing under test is "given media + caps,
    //! who wins the byte slot and who becomes a description".
    use super::*;

    fn item(item_type: &str) -> MediaItemLite {
        MediaItemLite {
            item_type: item_type.to_string(),
            base64: Some("ZmFrZQ==".to_string()),
            mime_type: Some(format!("{item_type}/test")),
            description: None,
        }
    }

    fn vision_only() -> HashSet<Capability> {
        let mut s = HashSet::new();
        s.insert(Capability::Vision);
        s
    }

    fn audio_only() -> HashSet<Capability> {
        let mut s = HashSet::new();
        s.insert(Capability::AudioInput);
        s
    }

    fn vision_and_audio() -> HashSet<Capability> {
        let mut s = HashSet::new();
        s.insert(Capability::Vision);
        s.insert(Capability::AudioInput);
        s
    }

    /// What this catches: empty media slice should produce an empty
    /// plan, NOT a None-attachable + non-empty-descriptions plan.
    #[test]
    fn empty_media_yields_empty_plan() {
        let plan = MediaPolicy::AtMostOneLatest.plan(&[], &vision_only());
        assert!(plan.attachable.is_none());
        assert!(plan.descriptions.is_empty());
    }

    /// What this catches: single image to vision-capable model — that
    /// image must be the attachable, descriptions empty. The trivial
    /// case but the regression baseline.
    #[test]
    fn single_image_vision_capable_attaches() {
        let media = vec![item("image")];
        let plan = MediaPolicy::AtMostOneLatest.plan(&media, &vision_only());
        assert!(plan.attachable.is_some());
        assert_eq!(plan.attachable.unwrap().item_type, "image");
        assert!(plan.descriptions.is_empty());
    }

    /// What this catches: single image to text-only model — no
    /// attachable, the image must end up in descriptions so the
    /// downstream marker path runs (otherwise the model gets nothing
    /// and hallucinates from prompt context).
    #[test]
    fn single_image_no_capability_becomes_description() {
        let media = vec![item("image")];
        let plan = MediaPolicy::AtMostOneLatest.plan(&media, &HashSet::new());
        assert!(plan.attachable.is_none());
        assert_eq!(plan.descriptions.len(), 1);
        assert_eq!(plan.descriptions[0].item_type, "image");
    }

    /// What this catches: THE CORE RULE. Three images, vision-capable
    /// model. Only the LAST attaches as bytes; the first two become
    /// descriptions. If this regresses, every multi-image trigger
    /// fires three vision-encoder calls and bricks the Mac.
    #[test]
    fn multiple_images_only_latest_attaches() {
        let media = vec![item("image"), item("image"), item("image")];
        let plan = MediaPolicy::AtMostOneLatest.plan(&media, &vision_only());
        assert!(plan.attachable.is_some(), "latest image must attach");
        // Pointer identity: the attachable IS the last slice element.
        assert!(std::ptr::eq(
            plan.attachable.unwrap() as *const _,
            &media[2] as *const _,
        ));
        assert_eq!(plan.descriptions.len(), 2);
        // Order preserved: descriptions are the first two, in source order.
        assert!(std::ptr::eq(
            plan.descriptions[0] as *const _,
            &media[0] as *const _
        ));
        assert!(std::ptr::eq(
            plan.descriptions[1] as *const _,
            &media[1] as *const _
        ));
    }

    /// What this catches: mixed image+audio with vision+audio model.
    /// The LATEST item wins regardless of type — so audio at index 1
    /// attaches, image at index 0 becomes a description. Catches the
    /// bug where someone hardcodes "prefer image" or "prefer audio"
    /// instead of "prefer latest".
    #[test]
    fn mixed_image_then_audio_audio_wins_when_latest() {
        let media = vec![item("image"), item("audio")];
        let plan = MediaPolicy::AtMostOneLatest.plan(&media, &vision_and_audio());
        assert_eq!(plan.attachable.unwrap().item_type, "audio");
        assert_eq!(plan.descriptions.len(), 1);
        assert_eq!(plan.descriptions[0].item_type, "image");
    }

    /// What this catches: audio at end of slice but model lacks
    /// AudioInput — must walk back and find the image (which IS
    /// supported), attach it, and demote the audio to a description.
    /// "Latest natively-supported" not "latest period".
    #[test]
    fn unsupported_latest_falls_back_to_supported_earlier() {
        let media = vec![item("image"), item("audio")];
        let plan = MediaPolicy::AtMostOneLatest.plan(&media, &vision_only());
        assert_eq!(plan.attachable.unwrap().item_type, "image");
        assert_eq!(plan.descriptions.len(), 1);
        assert_eq!(plan.descriptions[0].item_type, "audio");
    }

    /// What this catches: nothing in the slice is natively supported
    /// (audio sent to vision-only model, no images) → no attachable,
    /// every item becomes a description so the bridge text path runs.
    #[test]
    fn none_supported_yields_no_attachable() {
        let media = vec![item("audio"), item("audio")];
        let plan = MediaPolicy::AtMostOneLatest.plan(&media, &vision_only());
        assert!(plan.attachable.is_none());
        assert_eq!(plan.descriptions.len(), 2);
    }

    /// What this catches: AllAsDescriptions ignores capability and
    /// puts EVERY item in descriptions, no attachable. Used by
    /// forced-text-only callers (test scenarios, debug overrides).
    #[test]
    fn all_as_descriptions_attaches_nothing() {
        let media = vec![item("image"), item("audio")];
        let plan = MediaPolicy::AllAsDescriptions.plan(&media, &vision_and_audio());
        assert!(plan.attachable.is_none());
        assert_eq!(plan.descriptions.len(), 2);
    }

    /// What this catches: unknown item_type ("video", "file") falls
    /// to the description path even when the model has Vision/Audio
    /// caps. Forward-compat for media types we don't byte-handle yet.
    #[test]
    fn unknown_type_becomes_description() {
        let media = vec![item("video")];
        let plan = MediaPolicy::AtMostOneLatest.plan(&media, &vision_and_audio());
        assert!(plan.attachable.is_none());
        assert_eq!(plan.descriptions.len(), 1);
        assert_eq!(plan.descriptions[0].item_type, "video");
    }

    /// What this catches: audio-only model + image+audio in slice.
    /// The audio attaches; the image (no Vision cap) demotes to
    /// description. Symmetric to the vision-only case.
    #[test]
    fn audio_only_model_audio_wins() {
        let media = vec![item("audio"), item("image")];
        let plan = MediaPolicy::AtMostOneLatest.plan(&media, &audio_only());
        assert_eq!(plan.attachable.unwrap().item_type, "audio");
        assert_eq!(plan.descriptions.len(), 1);
        assert_eq!(plan.descriptions[0].item_type, "image");
    }
}
