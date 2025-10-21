/**
 * Emotion Configurations - TypeScript
 * Defines visual patterns, colors, and behaviors for different emotional states
 */

import { ValidEmotion, EmotionConfig } from './types';

// Simple emotion configurations for testing
export function calculateEmotionProperties(emotion: ValidEmotion, intensity: string, target?: string): EmotionConfig {
  const baseConfig = {
    emoji: getEmotionEmoji(emotion),
    animation: getEmotionAnimation(emotion),
    color: getEmotionColor(emotion),
    intensity: intensity as 'low' | 'medium' | 'high',
    target: target || 'main_display',
    persistent: false,
    returnToHome: true,
    duration: 3000
  };

  return baseConfig;
}

export function shouldReturnHome(emotion: ValidEmotion): boolean {
  // Most emotions should return to home state
  const persistentEmotions = ['error', 'warning', 'processing'];
  return !persistentEmotions.includes(emotion);
}

export function isPersistentEmotion(emotion: ValidEmotion): boolean {
  const persistentEmotions = ['error', 'warning', 'processing'];
  return persistentEmotions.includes(emotion);
}

function getEmotionEmoji(emotion: ValidEmotion): string {
  const emojiMap: { [key in ValidEmotion]: string } = {
    love: '💖',
    joy: '😊',
    excitement: '🎉',
    surprised: '😲',
    grateful: '🙏',
    playful: '😄',
    amazed: '🤩',
    thinking: '🤔',
    curious: '🧐',
    focused: '🎯',
    confused: '😕',
    sleepy: '😴',
    error: '❌',
    warning: '⚠️',
    offline: '📴',
    connecting: '🔗',
    processing: '⚙️',
    success: '✅'
  };
  
  return emojiMap[emotion] || '💚';
}

function getEmotionAnimation(emotion: ValidEmotion): string {
  const animationMap: { [key in ValidEmotion]: string } = {
    love: 'heart_pulse',
    joy: 'bounce_happy',
    excitement: 'rapid_bounce',
    surprised: 'quick_scale',
    grateful: 'gentle_bow',
    playful: 'bounce_fun',
    amazed: 'sparkle_burst',
    thinking: 'slow_orbit',
    curious: 'tilt_investigate',
    focused: 'steady_glow',
    confused: 'wobble_uncertain',
    sleepy: 'gentle_sway',
    error: 'alert_flash',
    warning: 'caution_blink',
    offline: 'fade_dim',
    connecting: 'pulse_blue',
    processing: 'spin_loading',
    success: 'celebration_burst'
  };
  
  return animationMap[emotion] || 'gentle_pulse';
}

function getEmotionColor(emotion: ValidEmotion): string {
  const colorMap: { [key in ValidEmotion]: string } = {
    love: '#ff69b4',
    joy: '#ffeb3b',
    excitement: '#ff5722',
    surprised: '#ffc107',
    grateful: '#8bc34a',
    playful: '#e91e63',
    amazed: '#9c27b0',
    thinking: '#9c27b0',
    curious: '#00bcd4',
    focused: '#3f51b5',
    confused: '#9e9e9e',
    sleepy: '#90a4ae',
    error: '#f44336',
    warning: '#ff9800',
    offline: '#424242',
    connecting: '#2196f3',
    processing: '#2196f3',
    success: '#4caf50'
  };
  
  return colorMap[emotion] || '#00ff00';
}