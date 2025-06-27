/**
 * TypeScript types for Emotion Command system
 * Clean, typed interfaces for emotion handling
 */
export const VALID_EMOTIONS = [
    // Fleeting emotions (fade automatically)
    'love', 'joy', 'excitement', 'surprised', 'grateful', 'playful', 'amazed',
    // Contextual emotions (fade unless persist=true)
    'thinking', 'curious', 'focused', 'confused', 'sleepy',
    // System states (should usually persist)
    'error', 'warning', 'offline', 'connecting', 'processing', 'success'
];
//# sourceMappingURL=types.js.map