// ISSUES: 0 open, last updated 2025-07-16 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking
/**
 * Emotion Module - Middle-out Architecture Export
 * 
 * Self-contained module for emotion expression functionality
 * following the middle-out architecture pattern
 */

// Main command exports
export { EmotionCommand } from './EmotionCommand';
export { default as EmotionCommandDefault } from './EmotionCommand';

// Types and interfaces
export type { 
  EmotionParams, 
  EmotionConfig, 
  EmotionContext, 
  EmotionResult,
  EmotionDefinition,
  ValidEmotion,
  EmotionType
} from './types';

// Constants
export { VALID_EMOTIONS } from './types';

// Utility functions
export { 
  calculateEmotionProperties, 
  shouldReturnHome, 
  isPersistentEmotion 
} from './emotionConfigs';

// Command definition
export { emotionDefinition } from './emotionDefinition';