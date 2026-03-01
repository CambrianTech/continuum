//! Fast, deterministic sentiment extraction from text.
//!
//! Extracts emotional tone via pattern matching (emoji → punctuation → keywords).
//! Sub-microsecond execution — safe for the speak_in_call hot path.
//! No ML model required; runs synchronously inline.
//!
//! Priority order (first match wins):
//! 1. Emoji (highest signal): 😊→Happy, 😢→Sad, 😠→Angry, 😮→Surprised, 😌→Relaxed
//! 2. Punctuation patterns: !!!→excited, ...→contemplative, ?!→surprised
//! 3. Keywords: "wonderful"→Happy, "sorry"→Sad, "wow"→Surprised

use crate::live::video::bevy_renderer::Emotion;

/// Result of sentiment analysis on a text fragment.
#[derive(Debug, Clone, Copy)]
pub struct SentimentResult {
    pub emotion: Emotion,
    /// Intensity of the detected emotion (0.0-1.0).
    pub intensity: f32,
}

impl SentimentResult {
    fn neutral() -> Self {
        Self { emotion: Emotion::Neutral, intensity: 0.0 }
    }

    fn new(emotion: Emotion, intensity: f32) -> Self {
        Self { emotion, intensity: intensity.clamp(0.0, 1.0) }
    }
}

/// Extract sentiment from text. Returns the dominant emotion and intensity.
/// Designed for AI-generated text (chat messages, TTS utterances).
pub fn extract_sentiment(text: &str) -> SentimentResult {
    // 1. Emoji scan (highest signal — explicit emotional intent)
    if let Some(result) = scan_emoji(text) {
        return result;
    }

    // 2. Punctuation patterns
    if let Some(result) = scan_punctuation(text) {
        return result;
    }

    // 3. Keyword matching (case-insensitive)
    if let Some(result) = scan_keywords(text) {
        return result;
    }

    SentimentResult::neutral()
}

fn scan_emoji(text: &str) -> Option<SentimentResult> {
    // Scan for emotion-bearing emoji. First match wins (leftmost = most salient).
    for ch in text.chars() {
        match ch {
            // Happy
            '😊' | '😄' | '😃' | '😁' | '🥰' | '😍' | '🤗' | '💕' | '❤' | '♥'
            | '😀' | '🙂' | '☺' | '💖' | '💗' | '😻' | '🎉' | '🥳' =>
                return Some(SentimentResult::new(Emotion::Happy, 0.8)),

            // Sad
            '😢' | '😭' | '😞' | '😔' | '🥺' | '😿' | '💔' | '😥' | '🙁' | '☹' =>
                return Some(SentimentResult::new(Emotion::Sad, 0.7)),

            // Angry
            '😠' | '😡' | '🤬' | '💢' | '👿' | '😤' =>
                return Some(SentimentResult::new(Emotion::Angry, 0.7)),

            // Surprised
            '😮' | '😲' | '🤯' | '😱' | '😳' | '🫢' | '❗' | '‼' | '⁉' =>
                return Some(SentimentResult::new(Emotion::Surprised, 0.8)),

            // Relaxed
            '😌' | '😴' | '🧘' | '☮' | '🍃' | '✨' | '🌸' =>
                return Some(SentimentResult::new(Emotion::Relaxed, 0.6)),

            _ => {}
        }
    }
    None
}

fn scan_punctuation(text: &str) -> Option<SentimentResult> {
    let trimmed = text.trim();

    // Count trailing exclamation marks
    let trailing_excl = trimmed.chars().rev().take_while(|&c| c == '!').count();
    if trailing_excl >= 3 {
        return Some(SentimentResult::new(Emotion::Surprised, 0.6));
    }
    if trailing_excl >= 2 {
        return Some(SentimentResult::new(Emotion::Happy, 0.5));
    }

    // "?!" or "!?" pattern
    if trimmed.contains("?!") || trimmed.contains("!?") {
        return Some(SentimentResult::new(Emotion::Surprised, 0.6));
    }

    // Trailing ellipsis — contemplative/sad
    if trimmed.ends_with("...") || trimmed.ends_with("…") {
        return Some(SentimentResult::new(Emotion::Sad, 0.3));
    }

    None
}

fn scan_keywords(text: &str) -> Option<SentimentResult> {
    let lower = text.to_lowercase();

    // Happy keywords (high confidence)
    const HAPPY_STRONG: &[&str] = &[
        "wonderful", "fantastic", "amazing", "love it", "brilliant",
        "delighted", "thrilled", "overjoyed", "ecstatic",
    ];
    const HAPPY_MILD: &[&str] = &[
        "nice", "good", "great", "happy", "glad", "pleased",
        "enjoy", "lovely", "beautiful", "excellent",
    ];

    // Sad keywords
    const SAD_STRONG: &[&str] = &[
        "heartbroken", "devastated", "terrible", "awful", "tragic",
        "miserable", "hopeless",
    ];
    const SAD_MILD: &[&str] = &[
        "sorry", "unfortunately", "sadly", "disappointed", "regret",
        "miss", "lonely", "upset",
    ];

    // Angry keywords
    const ANGRY: &[&str] = &[
        "furious", "outraged", "infuriating", "ridiculous", "unacceptable",
        "frustrated", "annoyed", "irritated",
    ];

    // Surprised keywords
    const SURPRISED: &[&str] = &[
        "wow", "incredible", "unbelievable", "astonishing", "shocking",
        "whoa", "oh my", "can't believe", "no way", "mind-blowing",
    ];

    // Relaxed keywords
    const RELAXED: &[&str] = &[
        "peaceful", "calm", "serene", "tranquil", "soothing",
        "gentle", "mellow", "at ease",
    ];

    // Check in priority order (strong signals first)
    for kw in HAPPY_STRONG {
        if lower.contains(kw) {
            return Some(SentimentResult::new(Emotion::Happy, 0.7));
        }
    }
    for kw in SAD_STRONG {
        if lower.contains(kw) {
            return Some(SentimentResult::new(Emotion::Sad, 0.7));
        }
    }
    for kw in ANGRY {
        if lower.contains(kw) {
            return Some(SentimentResult::new(Emotion::Angry, 0.6));
        }
    }
    for kw in SURPRISED {
        if lower.contains(kw) {
            return Some(SentimentResult::new(Emotion::Surprised, 0.7));
        }
    }
    for kw in RELAXED {
        if lower.contains(kw) {
            return Some(SentimentResult::new(Emotion::Relaxed, 0.5));
        }
    }
    // Mild signals last (lower priority)
    for kw in HAPPY_MILD {
        if lower.contains(kw) {
            return Some(SentimentResult::new(Emotion::Happy, 0.5));
        }
    }
    for kw in SAD_MILD {
        if lower.contains(kw) {
            return Some(SentimentResult::new(Emotion::Sad, 0.4));
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emoji_detection() {
        let r = extract_sentiment("This is great! 😊");
        assert_eq!(r.emotion, Emotion::Happy);
        assert!(r.intensity > 0.5);

        let r = extract_sentiment("I'm so sorry 😢");
        assert_eq!(r.emotion, Emotion::Sad);

        let r = extract_sentiment("Wow! 😮");
        assert_eq!(r.emotion, Emotion::Surprised);
    }

    #[test]
    fn punctuation_detection() {
        let r = extract_sentiment("What just happened?!");
        assert_eq!(r.emotion, Emotion::Surprised);

        let r = extract_sentiment("This is amazing!!!");
        assert_eq!(r.emotion, Emotion::Surprised);

        let r = extract_sentiment("I suppose so...");
        assert_eq!(r.emotion, Emotion::Sad);
    }

    #[test]
    fn keyword_detection() {
        let r = extract_sentiment("That was a wonderful presentation");
        assert_eq!(r.emotion, Emotion::Happy);

        let r = extract_sentiment("I'm really sorry about that");
        assert_eq!(r.emotion, Emotion::Sad);

        let r = extract_sentiment("Wow, that's incredible!");
        assert_eq!(r.emotion, Emotion::Surprised);

        let r = extract_sentiment("This is so peaceful and calm");
        assert_eq!(r.emotion, Emotion::Relaxed);
    }

    #[test]
    fn neutral_for_plain_text() {
        let r = extract_sentiment("The function returns a boolean value");
        assert_eq!(r.emotion, Emotion::Neutral);
        assert!(r.intensity < 0.01);
    }

    #[test]
    fn emoji_takes_priority_over_keywords() {
        // Emoji should win even if keyword says something else
        let r = extract_sentiment("I'm furious 😊");
        assert_eq!(r.emotion, Emotion::Happy);
    }
}
