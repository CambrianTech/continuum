//! Fast, deterministic sentiment and gesture extraction from text.
//!
//! Extracts emotional tone and body gesture cues via pattern matching.
//! Sub-microsecond execution — safe for the speak_in_call hot path.
//! No ML model required; runs synchronously inline.
//!
//! Emotion priority order (first match wins):
//! 1. Emoji (highest signal): 😊→Happy, 😢→Sad, 😠→Angry, 😮→Surprised, 😌→Relaxed
//! 2. Punctuation patterns: !!!→excited, ...→contemplative, ?!→surprised
//! 3. Keywords: "wonderful"→Happy, "sorry"→Sad, "wow"→Surprised
//!
//! Gesture extraction runs independently (scans full text for gesture keywords).

use crate::live::video::bevy_renderer::{Emotion, Gesture};

/// Result of sentiment analysis on a text fragment.
#[derive(Debug, Clone, Copy)]
pub struct SentimentResult {
    pub emotion: Emotion,
    /// Intensity of the detected emotion (0.0-1.0).
    pub intensity: f32,
    /// Body gesture to trigger (None = no gesture).
    pub gesture: Gesture,
}

impl SentimentResult {
    fn neutral() -> Self {
        Self { emotion: Emotion::Neutral, intensity: 0.0, gesture: Gesture::None }
    }

    fn new(emotion: Emotion, intensity: f32) -> Self {
        Self { emotion, intensity: intensity.clamp(0.0, 1.0), gesture: Gesture::None }
    }

    fn with_gesture(mut self, gesture: Gesture) -> Self {
        self.gesture = gesture;
        self
    }
}

/// Extract sentiment and gesture cues from text.
/// Returns the dominant emotion, intensity, and optional body gesture.
/// Designed for AI-generated text (chat messages, TTS utterances).
pub fn extract_sentiment(text: &str) -> SentimentResult {
    // Emotion detection (priority: emoji > punctuation > keywords)
    let mut result = if let Some(r) = scan_emoji(text) {
        r
    } else if let Some(r) = scan_punctuation(text) {
        r
    } else if let Some(r) = scan_keywords(text) {
        r
    } else {
        SentimentResult::neutral()
    };

    // Gesture detection — only if emoji didn't already set one.
    // Emoji gestures (👋→Wave, 🤔→Think) take priority over keyword detection.
    if result.gesture == Gesture::None {
        result.gesture = detect_gesture(text);
    }

    result
}

fn scan_emoji(text: &str) -> Option<SentimentResult> {
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

            // Wave gesture emoji
            '👋' => return Some(SentimentResult::new(Emotion::Happy, 0.6)
                .with_gesture(Gesture::Wave)),

            // Thinking emoji
            '🤔' => return Some(SentimentResult::new(Emotion::Neutral, 0.3)
                .with_gesture(Gesture::Think)),

            // Shrug emoji
            '🤷' => return Some(SentimentResult::new(Emotion::Neutral, 0.3)
                .with_gesture(Gesture::Shrug)),

            // Pointing emoji
            '👉' | '☝' | '👆' => return Some(SentimentResult::new(Emotion::Neutral, 0.3)
                .with_gesture(Gesture::Point)),

            _ => {}
        }
    }
    None
}

fn scan_punctuation(text: &str) -> Option<SentimentResult> {
    let trimmed = text.trim();

    let trailing_excl = trimmed.chars().rev().take_while(|&c| c == '!').count();
    if trailing_excl >= 3 {
        return Some(SentimentResult::new(Emotion::Surprised, 0.6));
    }
    if trailing_excl >= 2 {
        return Some(SentimentResult::new(Emotion::Happy, 0.5));
    }

    if trimmed.contains("?!") || trimmed.contains("!?") {
        return Some(SentimentResult::new(Emotion::Surprised, 0.6));
    }

    if trimmed.ends_with("...") || trimmed.ends_with("…") {
        return Some(SentimentResult::new(Emotion::Sad, 0.3));
    }

    None
}

fn scan_keywords(text: &str) -> Option<SentimentResult> {
    let lower = text.to_lowercase();

    const HAPPY_STRONG: &[&str] = &[
        "wonderful", "fantastic", "amazing", "love it", "brilliant",
        "delighted", "thrilled", "overjoyed", "ecstatic",
    ];
    const HAPPY_MILD: &[&str] = &[
        "nice", "good", "great", "happy", "glad", "pleased",
        "enjoy", "lovely", "beautiful", "excellent",
    ];

    const SAD_STRONG: &[&str] = &[
        "heartbroken", "devastated", "terrible", "awful", "tragic",
        "miserable", "hopeless",
    ];
    const SAD_MILD: &[&str] = &[
        "sorry", "unfortunately", "sadly", "disappointed", "regret",
        "miss", "lonely", "upset",
    ];

    const ANGRY: &[&str] = &[
        "furious", "outraged", "infuriating", "ridiculous", "unacceptable",
        "frustrated", "annoyed", "irritated",
    ];

    const SURPRISED: &[&str] = &[
        "wow", "incredible", "unbelievable", "astonishing", "shocking",
        "whoa", "oh my", "can't believe", "no way", "mind-blowing",
    ];

    const RELAXED: &[&str] = &[
        "peaceful", "calm", "serene", "tranquil", "soothing",
        "gentle", "mellow", "at ease",
    ];

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

/// Detect body gesture cues from text content.
/// Independent from emotion — scans for action/intent keywords.
fn detect_gesture(text: &str) -> Gesture {
    let lower = text.to_lowercase();

    // Greeting/farewell → Wave
    const WAVE_KEYWORDS: &[&str] = &[
        "hello", "hi everyone", "hey everyone", "bye", "goodbye",
        "good morning", "good evening", "welcome", "farewell",
        "hi there", "greetings",
        "hi!", "hey!", "hi,", "hey,", "howdy", "what's up",
        "see you", "take care", "later!", "cheers",
    ];

    // Thinking/wondering → Think
    const THINK_KEYWORDS: &[&str] = &[
        "hmm", "let me think", "i wonder", "perhaps", "considering",
        "interesting question", "that's a good point", "let me consider",
        "pondering", "contemplating",
        "think about", "thinking", "consider", "thought about",
        "wonder if", "wonder about", "curious", "interesting",
        "reflect on", "hm,", "hm.",
    ];

    // Agreement/emphasis → Nod
    const NOD_KEYWORDS: &[&str] = &[
        "absolutely", "exactly", "definitely", "i agree", "that's right",
        "precisely", "indeed", "certainly", "of course", "without a doubt",
        "agree", "right!", "yes!", "yeah!", "correct", "true!",
        "good point", "makes sense", "totally", "yep", "sure!", "100%",
    ];

    // Uncertainty → Shrug
    const SHRUG_KEYWORDS: &[&str] = &[
        "not sure", "i don't know", "maybe", "who knows", "hard to say",
        "it depends", "uncertain", "either way", "it's debatable",
        "don't know", "dunno", "idk", "no idea", "tough to say",
        "could go either way", "your guess", "beats me",
    ];

    // Explanation → OpenHands
    const OPENHANDS_KEYWORDS: &[&str] = &[
        "here's the thing", "so basically", "let me explain",
        "the way i see it", "in other words", "to put it simply",
        "what i mean is", "the key insight",
        "basically", "essentially", "the thing is", "what i mean",
        "in essence", "to clarify", "for example", "for instance",
        "think of it as", "put simply",
    ];

    // Direction/emphasis → Point
    const POINT_KEYWORDS: &[&str] = &[
        "look at this", "right there", "check this out", "notice how",
        "specifically", "in particular", "the important part",
        "pay attention to",
        "this is", "here's", "that's the", "key thing",
        "important thing", "main point", "crucial",
        "notably", "particularly",
    ];

    // Check in priority order (most specific first)
    for kw in WAVE_KEYWORDS {
        if lower.contains(kw) { return Gesture::Wave; }
    }
    for kw in THINK_KEYWORDS {
        if lower.contains(kw) { return Gesture::Think; }
    }
    for kw in NOD_KEYWORDS {
        if lower.contains(kw) { return Gesture::Nod; }
    }
    for kw in SHRUG_KEYWORDS {
        if lower.contains(kw) { return Gesture::Shrug; }
    }
    for kw in OPENHANDS_KEYWORDS {
        if lower.contains(kw) { return Gesture::OpenHands; }
    }
    for kw in POINT_KEYWORDS {
        if lower.contains(kw) { return Gesture::Point; }
    }

    Gesture::None
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
        assert_eq!(r.gesture, Gesture::None);
    }

    #[test]
    fn emoji_takes_priority_over_keywords() {
        let r = extract_sentiment("I'm furious 😊");
        assert_eq!(r.emotion, Emotion::Happy);
    }

    #[test]
    fn gesture_detection_wave() {
        let r = extract_sentiment("Hello everyone! How are you?");
        assert_eq!(r.gesture, Gesture::Wave);
    }

    #[test]
    fn gesture_detection_think() {
        let r = extract_sentiment("Hmm, let me think about this for a moment.");
        assert_eq!(r.gesture, Gesture::Think);
    }

    #[test]
    fn gesture_detection_nod() {
        let r = extract_sentiment("Absolutely, that's exactly right.");
        assert_eq!(r.gesture, Gesture::Nod);
    }

    #[test]
    fn gesture_detection_shrug() {
        let r = extract_sentiment("I'm not sure, it depends on the context.");
        assert_eq!(r.gesture, Gesture::Shrug);
    }

    #[test]
    fn gesture_detection_openhands() {
        let r = extract_sentiment("Here's the thing about this approach...");
        assert_eq!(r.gesture, Gesture::OpenHands);
    }

    #[test]
    fn gesture_detection_point() {
        let r = extract_sentiment("Look at this example right here.");
        assert_eq!(r.gesture, Gesture::Point);
    }

    #[test]
    fn wave_emoji_triggers_gesture() {
        let r = extract_sentiment("Hey! 👋");
        assert_eq!(r.gesture, Gesture::Wave);
        assert_eq!(r.emotion, Emotion::Happy);
    }

    #[test]
    fn thinking_emoji_triggers_gesture() {
        let r = extract_sentiment("That's interesting 🤔");
        assert_eq!(r.gesture, Gesture::Think);
    }

    #[test]
    fn no_gesture_for_plain_text() {
        let r = extract_sentiment("The function returns a boolean value");
        assert_eq!(r.gesture, Gesture::None);
    }
}
