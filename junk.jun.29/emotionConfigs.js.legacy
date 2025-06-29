/**
 * Emotion Configurations - TypeScript
 * Defines visual patterns, colors, and behaviors for different emotional states
 */
// Simple emotion configurations for testing
export function calculateEmotionProperties(emotion, intensity, target) {
    const baseConfig = {
        emoji: getEmotionEmoji(emotion),
        animation: getEmotionAnimation(emotion),
        color: getEmotionColor(emotion),
        intensity: intensity,
        target: target || 'main_display',
        persistent: false,
        returnToHome: true,
        duration: 3000
    };
    return baseConfig;
}
export function shouldReturnHome(emotion) {
    // Most emotions should return to home state
    const persistentEmotions = ['error', 'warning', 'processing'];
    return !persistentEmotions.includes(emotion);
}
export function isPersistentEmotion(emotion) {
    const persistentEmotions = ['error', 'warning', 'processing'];
    return persistentEmotions.includes(emotion);
}
function getEmotionEmoji(emotion) {
    const emojiMap = {
        love: '💖',
        joy: '😊',
        sadness: '😢',
        excitement: '🎉',
        thinking: '🤔',
        surprised: '😲',
        sleepy: '😴',
        angry: '😠',
        curious: '🧐',
        proud: '😤',
        error: '❌',
        warning: '⚠️',
        processing: '⚙️',
        success: '✅'
    };
    return emojiMap[emotion] || '💚';
}
function getEmotionAnimation(emotion) {
    const animationMap = {
        love: 'heart_pulse',
        joy: 'bounce_happy',
        sadness: 'slow_fade',
        excitement: 'rapid_bounce',
        thinking: 'slow_orbit',
        surprised: 'quick_scale',
        sleepy: 'gentle_sway',
        angry: 'rapid_shake',
        curious: 'tilt_investigate',
        proud: 'victory_pose',
        error: 'alert_flash',
        warning: 'caution_blink',
        processing: 'spin_loading',
        success: 'celebration_burst'
    };
    return animationMap[emotion] || 'gentle_pulse';
}
function getEmotionColor(emotion) {
    const colorMap = {
        love: '#ff69b4',
        joy: '#ffeb3b',
        sadness: '#64b5f6',
        excitement: '#ff5722',
        thinking: '#9c27b0',
        surprised: '#ffc107',
        sleepy: '#90a4ae',
        angry: '#f44336',
        curious: '#00bcd4',
        proud: '#4caf50',
        error: '#f44336',
        warning: '#ff9800',
        processing: '#2196f3',
        success: '#4caf50'
    };
    return colorMap[emotion] || '#00ff00';
}
//# sourceMappingURL=emotionConfigs.js.map