//! Emotion tags for expressive TTS — Stage A of the native-audio ladder
//! (VOICE-ENGINE-PLAN.md; Joel 2026-09-02: "intimately control the speech
//! mannerisms… so it wouldn't sound like text to speech").
//!
//! ONE sentiment source: the same sub-microsecond extraction that drives the
//! avatar's face ([`crate::live::session::sentiment`]) maps here to Orpheus's
//! trained emotion tags — voice and face express one state, never two
//! analyzers drifting. Stage B retires this module entirely: when speech
//! tokens come from the persona's own forward pass, mannerisms need no
//! injection because the speaking IS the thinking.

use crate::live::session::sentiment;
use crate::live::video::bevy_renderer::Emotion;

/// Minimum sentiment intensity before a tag is worth an interjection — a
/// mild reading decorated with a laugh sounds unhinged, not natural.
const TAG_INTENSITY_FLOOR: f32 = 0.5;

/// Map the shared sentiment result to an Orpheus interjection tag. `None` =
/// speak plainly (neutral/relaxed, or below the floor). Tags are from the
/// model's TRAINED set — an untrained tag gets read aloud as text.
fn tag_for(emotion: Emotion, intensity: f32) -> Option<&'static str> {
    if intensity < TAG_INTENSITY_FLOOR {
        return None;
    }
    match emotion {
        Emotion::Happy => Some("<chuckle>"),
        Emotion::Sad => Some("<sigh>"),
        Emotion::Surprised => Some("<gasp>"),
        Emotion::Angry => Some("<groan>"),
        _ => None,
    }
}

/// Decorate `text` with an emotion tag for adapters that understand them.
/// Orpheus-only by contract: every other engine would read `<sigh>` aloud.
/// Borrowed pass-through when nothing applies — the common case allocates
/// nothing.
pub fn decorate_for_adapter<'a>(text: &'a str, adapter: Option<&str>) -> std::borrow::Cow<'a, str> {
    if adapter != Some("orpheus") {
        return std::borrow::Cow::Borrowed(text);
    }
    let s = sentiment::extract_sentiment(text);
    match tag_for(s.emotion, s.intensity) {
        Some(tag) => std::borrow::Cow::Owned(format!("{tag} {text}")),
        None => std::borrow::Cow::Borrowed(text),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the two contract edges. (1) Non-Orpheus engines must
    // NEVER receive a tag — they read it aloud as text (the literal failure a
    // demo would ship). (2) A strong happy reading gets its chuckle; a flat
    // one stays plain — the intensity floor is what keeps voices natural
    // instead of unhinged.
    #[test]
    fn tags_are_orpheus_only_and_intensity_gated() {
        let excited = "This is wonderful!!! I love it!!!";
        let flat = "The file is in the src directory.";

        let kokoro = decorate_for_adapter(excited, Some("kokoro"));
        assert_eq!(kokoro, excited, "non-Orpheus engines get plain text");

        let orpheus_excited = decorate_for_adapter(excited, Some("orpheus"));
        assert!(
            orpheus_excited.starts_with('<'),
            "strong sentiment earns a tag: {orpheus_excited}"
        );

        let orpheus_flat = decorate_for_adapter(flat, Some("orpheus"));
        assert_eq!(orpheus_flat, flat, "flat text stays plain even on Orpheus");
    }
}
