//! Sensory routing: native track vs. translator bridge, per modality.
//!
//! Continuum's rule (CLAUDE.md "Sensory Architecture"): EVERY persona sees,
//! hears, and speaks — regardless of whether its base model is natively
//! multimodal. The system bridges the gap so a text-only 3.5B model has the
//! same sensory experience as a natively-omni model. This module is the ONE
//! place that decision is made: given a model and a media [`Modality`] (the
//! shape of a LiveKit track), return whether the raw track flows straight
//! to/from the model ([`SensoryRoute::Native`]) or must pass through a
//! translator ([`SensoryRoute::Bridge`]).
//!
//! The decision reads the model's capability set — which the registry
//! hydrates authoritatively from each model's own artifact (the mmproj
//! projector's `clip.has_*_encoder` keys for local models, the provider
//! `/v1/models` listing for cloud). It is NEVER a guess from the model name.
//! That is the whole point: drop in a new video/audio model and it routes
//! natively the instant its artifact says it can; drop in a text-only model
//! and it transparently gets STT / TTS / vision-describe / object-detect.
//!
//! Video is not a raw-to-model path in this stack: a LiveKit video track is
//! decoded to frames before any model sees it, so [`Modality::VideoIn`]
//! always routes through [`BridgeKind::VideoFrameSample`], and each sampled
//! frame is then routed as an [`Modality::ImageIn`] (native to a vision
//! model, described/detected for a text-only one). This composition — video
//! decomposes to a stream of images — keeps one honest rule per modality.

use crate::model_registry::types::{Capability, Model};

/// A media modality a LiveKit track carries, from the model's point of view.
/// Input modalities describe what arrives from a peer's camera/mic/screen;
/// output modalities describe what the model must emit back onto a track.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modality {
    /// A single still image in — a pasted picture, a screenshot, or one
    /// sampled video frame. Native for a model that `has(Vision)`.
    ImageIn,
    /// A live video track in (camera / screen share). Never raw-to-model:
    /// always sampled to frames first, then each frame is an [`ImageIn`].
    VideoIn,
    /// A live audio track in (microphone). Native for a model that
    /// `has(AudioInput)`; otherwise transcribed by STT.
    AudioIn,
    /// An audio track out (the persona's spoken voice). Native for a model
    /// that `has(AudioOutput)`; otherwise synthesized by TTS from its text.
    AudioOut,
}

/// A translator that converts a modality to/from the text a non-native model
/// can handle. Each is a distinct worker the sensory layer invokes; naming
/// them as data (not `if model_name.contains(...)`) is what lets the router
/// stay a pure capability decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BridgeKind {
    /// Audio in → text (Whisper-class transcription).
    SpeechToText,
    /// Text → audio out (TTS synthesis of the model's spoken reply).
    TextToSpeech,
    /// Image in → text (a caption / VLM description). The general-purpose
    /// image→text bridge — the universal way a blind model "sees".
    VisionDescribe,
    /// Image in → text (structured object labels + boxes, YOLO-class). A
    /// specialization of the image→text bridge a detection-driven pipeline
    /// substitutes for [`VisionDescribe`] when it needs entities, not prose.
    ObjectDetect,
    /// Video in → a stream of image frames. Always applied first for video;
    /// each emitted frame is then routed as [`Modality::ImageIn`].
    VideoFrameSample,
}

impl BridgeKind {
    /// True when this bridge turns an inbound modality INTO text the model
    /// reads (STT, describe, detect). [`TextToSpeech`] and
    /// [`VideoFrameSample`] are not text-producing (one emits audio, one
    /// emits frames), so they return `false`.
    pub fn produces_text(self) -> bool {
        matches!(
            self,
            BridgeKind::SpeechToText | BridgeKind::VisionDescribe | BridgeKind::ObjectDetect
        )
    }
}

/// How a model consumes/produces a given modality.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SensoryRoute {
    /// The raw track flows straight to/from the model — the model's own
    /// artifact declares it handles this modality natively.
    Native,
    /// The modality must pass through a translator; the model handles only
    /// the bridge's text (or receives the bridge's frames).
    Bridge(BridgeKind),
}

impl SensoryRoute {
    pub fn is_native(self) -> bool {
        matches!(self, SensoryRoute::Native)
    }
}

/// Decide how `model` handles `modality`: native track or which bridge.
///
/// The single decision point for LiveKit media routing. Reads only the
/// model's hydrated capability set — no name sniffing, no per-model special
/// case. For images the returned bridge is [`BridgeKind::VisionDescribe`],
/// the general image→text default; a detection-specific pipeline swaps in
/// [`BridgeKind::ObjectDetect`] itself. Video always samples to frames first
/// (see module docs), so the caller re-routes each frame as `ImageIn`.
pub fn route(model: &Model, modality: Modality) -> SensoryRoute {
    match modality {
        Modality::ImageIn => {
            if model.has(Capability::Vision) {
                SensoryRoute::Native
            } else {
                SensoryRoute::Bridge(BridgeKind::VisionDescribe)
            }
        }
        // A video track is decoded to frames before any model sees it, so
        // video is never native-to-model — it decomposes into a frame stream
        // the caller then routes per-frame as `ImageIn`.
        Modality::VideoIn => SensoryRoute::Bridge(BridgeKind::VideoFrameSample),
        Modality::AudioIn => {
            if model.has(Capability::AudioInput) {
                SensoryRoute::Native
            } else {
                SensoryRoute::Bridge(BridgeKind::SpeechToText)
            }
        }
        Modality::AudioOut => {
            if model.has(Capability::AudioOutput) {
                SensoryRoute::Native
            } else {
                SensoryRoute::Bridge(BridgeKind::TextToSpeech)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_registry::types::{Arch, Model};
    use std::collections::BTreeSet;

    /// Build a model carrying exactly the given capabilities — the only
    /// input `route` reads, so nothing else needs to be realistic.
    fn model_with_caps(caps: &[Capability]) -> Model {
        let mut m = Model {
            id: "test/model".into(),
            name: None,
            provider: "llamacpp-local".into(),
            arch: Arch::Unknown,
            context_window: 0,
            max_output_tokens: 0,
            tokens_per_second: 0.0,
            capabilities: BTreeSet::new(),
            cost_input_per_1k: 0.0,
            cost_output_per_1k: 0.0,
            gguf_hint: None,
            hf_source: None,
            gguf_local_path: None,
            mmproj_local_path: None,
            chat_template: None,
            multi_party_strategy: Default::default(),
            stop_sequences: Vec::new(),
            parameter_count: 0,
            sampling: crate::model_registry::types::ModelSampling::default(),
            persona_serving_eligible: true,
        };
        m.capabilities.extend(caps.iter().copied());
        m
    }

    // what this catches: outlier A — a natively omni model (vision + audio in,
    // audio out) routes EVERY sensory modality straight to the raw track. This
    // is the model that plugs directly into LiveKit audio/video with no bridge
    // worker in the path. If a regression forced it through STT/TTS/describe we
    // would be paying translator latency on a model that never needed it.
    #[test]
    fn natively_omni_model_routes_every_modality_to_the_raw_track() {
        let omni = model_with_caps(&[
            Capability::Vision,
            Capability::AudioInput,
            Capability::AudioOutput,
        ]);
        assert!(route(&omni, Modality::ImageIn).is_native());
        assert!(route(&omni, Modality::AudioIn).is_native());
        assert!(route(&omni, Modality::AudioOut).is_native());
        // Even omni: a video TRACK is still decoded to frames before the model
        // sees it — native vision applies per sampled frame, not to the track.
        assert_eq!(
            route(&omni, Modality::VideoIn),
            SensoryRoute::Bridge(BridgeKind::VideoFrameSample)
        );
    }

    // what this catches: outlier B — a text-only model (the 3.5B class) gets
    // the FULL translator set so it still sees, hears, and speaks: image→describe,
    // audio-in→STT, audio-out→TTS, video→frame-sample. This is the exact
    // "incapable model uses the bridge" column of the Sensory Architecture. A
    // regression that returned Native here would feed raw pixels/audio to a
    // model that can't read them — silent garbage, the worst failure.
    #[test]
    fn text_only_model_bridges_every_modality() {
        let text = model_with_caps(&[Capability::TextGeneration, Capability::Chat]);
        assert_eq!(
            route(&text, Modality::ImageIn),
            SensoryRoute::Bridge(BridgeKind::VisionDescribe)
        );
        assert_eq!(
            route(&text, Modality::AudioIn),
            SensoryRoute::Bridge(BridgeKind::SpeechToText)
        );
        assert_eq!(
            route(&text, Modality::AudioOut),
            SensoryRoute::Bridge(BridgeKind::TextToSpeech)
        );
        assert_eq!(
            route(&text, Modality::VideoIn),
            SensoryRoute::Bridge(BridgeKind::VideoFrameSample)
        );
    }

    // what this catches: the middle case is per-modality, not all-or-nothing —
    // a vision-only VL model takes image tracks natively but STILL needs STT for
    // audio and TTS to speak. Proves the router decides each modality against
    // its own capability, so a partially-multimodal model isn't wrongly treated
    // as fully native or fully text-only.
    #[test]
    fn vision_only_model_is_native_for_images_bridged_for_audio() {
        let vl = model_with_caps(&[Capability::Vision, Capability::Chat]);
        assert!(route(&vl, Modality::ImageIn).is_native());
        assert_eq!(
            route(&vl, Modality::AudioIn),
            SensoryRoute::Bridge(BridgeKind::SpeechToText)
        );
        assert_eq!(
            route(&vl, Modality::AudioOut),
            SensoryRoute::Bridge(BridgeKind::TextToSpeech)
        );
    }

    // what this catches: the inbound bridges that feed the model produce text
    // (STT, describe, detect); the outbound/decompose bridges do not (TTS emits
    // audio, frame-sample emits images). A caller assembling the model's text
    // context relies on this split to know which bridge outputs to concatenate.
    #[test]
    fn only_inbound_translators_produce_text() {
        assert!(BridgeKind::SpeechToText.produces_text());
        assert!(BridgeKind::VisionDescribe.produces_text());
        assert!(BridgeKind::ObjectDetect.produces_text());
        assert!(!BridgeKind::TextToSpeech.produces_text());
        assert!(!BridgeKind::VideoFrameSample.produces_text());
    }
}
