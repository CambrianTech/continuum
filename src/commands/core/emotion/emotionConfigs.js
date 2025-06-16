/**
 * Emotion Configurations for Continuon
 * Defines visual patterns, colors, and behaviors for different emotional states
 */

export const emotionConfigs = {
  // Fleeting emotions (fade automatically)
  love: {
    color: '#ff69b4',  // Pink
    pattern: 'heart_pulse',
    movement: 'floating_hearts',
    particles: ['💖', '💕', '💘'],
    glow: 'warm_pink',
    returnToHome: true,
    persistent: false
  },

  joy: {
    color: '#ffeb3b',  // Bright yellow
    pattern: 'bounce_happy',
    movement: 'rainbow_dance',
    particles: ['✨', '🌟', '💫'],
    glow: 'golden_bright',
    returnToHome: true,
    persistent: false
  },

  excitement: {
    color: '#ff5722',  // Orange-red
    pattern: 'rapid_bounce',
    movement: 'energetic_spiral',
    particles: ['⚡', '🔥', '💥'],
    glow: 'electric_orange',
    returnToHome: true,
    persistent: false
  },

  thinking: {
    color: '#9c27b0',  // Purple
    pattern: 'slow_orbit',
    movement: 'contemplative_circle',
    particles: ['💭', '🤔', '💡'],
    glow: 'thoughtful_purple',
    returnToHome: true,
    persistent: false
  },

  curious: {
    color: '#2196f3',  // Blue
    pattern: 'investigate',
    movement: 'scanning_pattern',
    particles: ['👀', '🔍', '❓'],
    glow: 'inquisitive_blue',
    returnToHome: true,
    persistent: false
  },

  surprised: {
    color: '#ffc107',  // Amber
    pattern: 'sudden_expand',
    movement: 'shock_recoil',
    particles: ['😲', '‼️', '⚠️'],
    glow: 'startled_yellow',
    returnToHome: true,
    persistent: false
  },

  grateful: {
    color: '#4caf50',  // Green
    pattern: 'warm_glow',
    movement: 'gentle_bow',
    particles: ['🙏', '💚', '✨'],
    glow: 'grateful_green',
    returnToHome: true,
    persistent: false
  },

  playful: {
    color: '#e91e63',  // Pink
    pattern: 'playful_wiggle',
    movement: 'silly_dance',
    particles: ['🎭', '🎨', '🎪'],
    glow: 'playful_pink',
    returnToHome: true,
    persistent: false
  },

  amazed: {
    color: '#00bcd4',  // Cyan
    pattern: 'wonder_expand',
    movement: 'awe_spiral',
    particles: ['🤯', '✨', '🌟'],
    glow: 'amazed_cyan',
    returnToHome: true,
    persistent: false
  },

  // Contextual emotions (borrowed temporarily)
  focused: {
    color: '#607d8b',  // Blue-gray
    pattern: 'steady_pulse',
    movement: 'locked_position',
    particles: ['🎯', '👁️', '🔒'],
    glow: 'focused_gray',
    returnToHome: true,
    persistent: false
  },

  confused: {
    color: '#795548',  // Brown
    pattern: 'uncertain_wobble',
    movement: 'lost_wander',
    particles: ['❓', '🤷', '🌀'],
    glow: 'confused_brown',
    returnToHome: true,
    persistent: false
  },

  sleepy: {
    color: '#673ab7',  // Deep purple
    pattern: 'drowsy_bob',
    movement: 'sleepy_drift',
    particles: ['😴', '💤', '🌙'],
    glow: 'dreamy_purple',
    returnToHome: true,
    persistent: false
  },

  // System States (usually persistent, don't return home)
  error: {
    color: '#f44336',  // Red
    pattern: 'urgent_pulse',
    movement: 'alert_shake',
    particles: ['❌', '⚠️', '🚨'],
    glow: 'danger_red',
    returnToHome: false,  // Stay visible for critical issues
    persistent: true
  },

  warning: {
    color: '#ff9800',  // Orange
    pattern: 'caution_pulse',
    movement: 'attention_bob',
    particles: ['⚠️', '⚡', '📢'],
    glow: 'warning_orange',
    returnToHome: false,  // Stay visible for warnings
    persistent: true
  },

  offline: {
    color: '#607d8b',  // Gray
    pattern: 'disconnected_fade',
    movement: 'lost_drift',
    particles: ['💔', '📵', '🌫️'],
    glow: 'dim_gray',
    returnToHome: false,  // Don't hide when offline
    persistent: true
  },

  connecting: {
    color: '#2196f3',  // Blue
    pattern: 'connection_search',
    movement: 'searching_orbit',
    particles: ['🔄', '📡', '⏳'],
    glow: 'connecting_blue',
    returnToHome: false,  // Show connection attempts
    persistent: true
  },

  processing: {
    color: '#9c27b0',  // Purple
    pattern: 'work_spin',
    movement: 'busy_rotation',
    particles: ['⚙️', '🔄', '💭'],
    glow: 'processing_purple',
    returnToHome: false,  // Show work in progress
    persistent: true
  },

  success: {
    color: '#4caf50',  // Green
    pattern: 'victory_burst',
    movement: 'celebration_bounce',
    particles: ['✅', '🎉', '⭐'],
    glow: 'success_green',
    returnToHome: true,  // Success celebrates then goes home
    persistent: false    // Success is fleeting
  }
};

/**
 * Get intensity multipliers for different emotion strengths
 */
export const intensityMultipliers = {
  subtle: 0.5,
  medium: 1.0,
  strong: 1.5,
  overwhelming: 2.0
};

/**
 * Calculate emotion properties based on intensity
 */
export function calculateEmotionProperties(emotion, intensity = 'medium') {
  const multiplier = intensityMultipliers[intensity] || 1.0;
  const config = emotionConfigs[emotion] || emotionConfigs.joy;
  
  return {
    ...config,
    bounceHeight: (config.baseHeight || 20) * multiplier,
    pulseCount: config.persistent ? 999 : Math.floor((config.basePulses || 5) * multiplier),
    particleCount: Math.floor((config.baseParticles || 3) * multiplier),
    glowIntensity: Math.min(1.0, 0.6 * multiplier),
    trailDensity: Math.min(10, Math.floor(3 * multiplier))
  };
}

/**
 * Determine if continuon should return home after emotion
 */
export function shouldReturnHome(emotion) {
  const config = emotionConfigs[emotion] || emotionConfigs.joy;
  return config.returnToHome !== false; // Default to true unless explicitly false
}

/**
 * Check if emotion should persist until cleared
 */
export function isPersistentEmotion(emotion) {
  const config = emotionConfigs[emotion] || emotionConfigs.joy;
  return config.persistent === true; // Default to false unless explicitly true
}