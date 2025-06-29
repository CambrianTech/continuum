/**
 * Emotion Configurations for Continuon
 * Defines visual patterns, colors, and behaviors for different emotional states
 */

const emotionConfigs = {
  // Fleeting emotions (fade automatically)
  love: {
    color: '#ff69b4',  // Pink
    pattern: 'heart_pulse',
    movement: 'floating_hearts',
    particles: ['💖', '💕', '💘'],
    glow: 'warm_pink',
    returnToHome: true,
    persistent: false,
    baseHeight: 20,
    basePulses: 6,
    baseParticles: 5
  },

  joy: {
    color: '#ffeb3b',  // Bright yellow
    pattern: 'bounce_happy',
    movement: 'rainbow_dance',
    particles: ['✨', '🌟', '💫'],
    glow: 'golden_bright',
    returnToHome: true,
    persistent: false,
    baseHeight: 30,
    basePulses: 8,
    baseParticles: 6
  },

  excitement: {
    color: '#ff5722',  // Orange-red
    pattern: 'rapid_bounce',
    movement: 'energetic_spiral',
    particles: ['⚡', '🔥', '💥'],
    glow: 'electric_orange',
    returnToHome: true,
    persistent: false,
    baseHeight: 40,
    basePulses: 12,
    baseParticles: 8
  },

  thinking: {
    color: '#9c27b0',  // Purple
    pattern: 'slow_orbit',
    movement: 'contemplative_circle',
    particles: ['💭', '🤔', '💡'],
    glow: 'thoughtful_purple',
    returnToHome: true,
    persistent: false,
    baseHeight: 10,
    basePulses: 3,
    baseParticles: 3
  },

  curious: {
    color: '#2196f3',  // Blue
    pattern: 'investigate',
    movement: 'scanning_pattern',
    movementWithTarget: 'move_to_investigate',
    particles: ['👀', '🔍', '❓'],
    glow: 'inquisitive_blue',
    returnToHome: true,
    persistent: false,
    baseHeight: 15,
    basePulses: 6,
    baseParticles: 4
  },

  surprised: {
    color: '#ffc107',  // Amber
    pattern: 'sudden_expand',
    movement: 'shock_recoil',
    particles: ['😲', '‼️', '⚠️'],
    glow: 'startled_yellow',
    returnToHome: true,
    persistent: false,
    baseHeight: 50,
    basePulses: 2,
    baseParticles: 3
  },

  // System States (usually persistent, don't return home)
  error: {
    color: '#f44336',  // Red
    pattern: 'urgent_pulse',
    movement: 'alert_shake',
    particles: ['❌', '⚠️', '🚨'],
    glow: 'danger_red',
    returnToHome: false,  // Stay visible for critical issues
    persistent: true,
    baseHeight: 15,
    basePulses: 6,
    baseParticles: 4
  },

  warning: {
    color: '#ff9800',  // Orange
    pattern: 'caution_pulse',
    movement: 'attention_bob',
    particles: ['⚠️', '⚡', '📢'],
    glow: 'warning_orange',
    returnToHome: false,  // Stay visible for warnings
    persistent: true,
    baseHeight: 10,
    basePulses: 4,
    baseParticles: 3
  },

  offline: {
    color: '#607d8b',  // Gray
    pattern: 'disconnected_fade',
    movement: 'lost_drift',
    particles: ['💔', '📵', '🌫️'],
    glow: 'dim_gray',
    returnToHome: false,  // Don't hide when offline
    persistent: true,
    baseHeight: 2,
    basePulses: 2,
    baseParticles: 2
  },

  connecting: {
    color: '#2196f3',  // Blue
    pattern: 'connection_search',
    movement: 'searching_orbit',
    particles: ['🔄', '📡', '⏳'],
    glow: 'connecting_blue',
    returnToHome: false,  // Show connection attempts
    persistent: true,
    baseHeight: 8,
    basePulses: 8,
    baseParticles: 3
  },

  processing: {
    color: '#9c27b0',  // Purple
    pattern: 'work_spin',
    movement: 'busy_rotation',
    particles: ['⚙️', '🔄', '💭'],
    glow: 'processing_purple',
    returnToHome: false,  // Show work in progress
    persistent: true,
    baseHeight: 5,
    basePulses: 10,
    baseParticles: 4
  },

  success: {
    color: '#4caf50',  // Green
    pattern: 'victory_burst',
    movement: 'celebration_bounce',
    particles: ['✅', '🎉', '⭐'],
    glow: 'success_green',
    returnToHome: true,  // Success celebrates then goes home
    persistent: false,   // Success is fleeting
    baseHeight: 20,
    basePulses: 3,
    baseParticles: 5
  }
};

/**
 * Get intensity multipliers for different emotion strengths
 */
const intensityMultipliers = {
  subtle: 0.5,
  medium: 1.0,
  strong: 1.5,
  overwhelming: 2.0
};

/**
 * Calculate emotion properties based on intensity and target
 */
function calculateEmotionProperties(emotion, intensity = 'medium', target = null) {
  const multiplier = intensityMultipliers[intensity] || 1.0;
  const config = emotionConfigs[emotion] || emotionConfigs.joy;
  
  // Determine movement behavior based on target
  let movement = config.movement;
  if (target && config.movementWithTarget) {
    movement = config.movementWithTarget;
  }
  
  return {
    ...config,
    movement,
    bounceHeight: (config.baseHeight || 20) * multiplier,
    pulseCount: config.persistent ? 999 : Math.floor((config.basePulses || 5) * multiplier),
    particleCount: Math.floor((config.baseParticles || 3) * multiplier),
    glowIntensity: Math.min(1.0, 0.6 * multiplier),
    trailDensity: Math.min(10, Math.floor(3 * multiplier)),
    intensity: multiplier
  };
}

/**
 * Determine if continuon should return home after emotion
 */
function shouldReturnHome(emotion) {
  const config = emotionConfigs[emotion] || emotionConfigs.joy;
  return config.returnToHome !== false; // Default to true unless explicitly false
}

/**
 * Check if emotion should persist until cleared
 */
function isPersistentEmotion(emotion) {
  const config = emotionConfigs[emotion] || emotionConfigs.joy;
  return config.persistent === true; // Default to false unless explicitly true
}

module.exports = {
  emotionConfigs,
  intensityMultipliers,
  calculateEmotionProperties,
  shouldReturnHome,
  isPersistentEmotion
};