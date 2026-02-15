import { Commands } from '../../../../system/core/shared/Commands';
import type { CommandParams, CommandResult, CommandInput} from '../../../../system/core/types/JTAGTypes';

/** Display an emoji reaction with an optional color glow on the Continuum interface. */
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

/**
 * Emotion — Type-safe command executor
 *
 * Usage:
 *   import { Emotion } from '...shared/EmotionTypes';
 *   const result = await Emotion.execute({ ... });
 */
export const Emotion = {
  execute(params: CommandInput<EmotionParams>): Promise<EmotionResult> {
    return Commands.execute<EmotionParams, EmotionResult>('continuum/emotion', params as Partial<EmotionParams>);
  },
  commandName: 'continuum/emotion' as const,
} as const;
