import { Commands } from '../../../../system/core/shared/Commands';
import type { CommandParams, CommandResult, CommandInput} from '../../../../system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

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

/**
 * Factory function for creating ContinuumEmotionParams
 */
export const createEmotionParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<EmotionParams, 'context' | 'sessionId' | 'userId'>
): EmotionParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Factory function for creating ContinuumEmotionResult with defaults
 */
export const createEmotionResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<EmotionResult, 'context' | 'sessionId' | 'userId'>
): EmotionResult => createPayload(context, sessionId, {
  ...data
});

/**
 * Smart continuum/emotion-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createEmotionResultFromParams = (
  params: EmotionParams,
  differences: Omit<EmotionResult, 'context' | 'sessionId' | 'userId'>
): EmotionResult => transformPayload(params, differences);

