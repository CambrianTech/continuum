import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';

export interface EmotionParams extends CommandParams {
  emoji: string;        // Emoji to display (e.g., '❤️', '😊', '🤔')
  color?: string;       // Optional color for glow (hex or CSS color)
  duration?: number;    // Duration in ms (default 3000ms)
}

export interface EmotionResult extends CommandResult {
  success: boolean;
  emoji: string;
  color: string;
  duration: number;
  timestamp: string;
}

export const EMOTION_EVENT = 'continuum:emotion' as const;
