/**
 * Voice Stop Command - Shared Types
 *
 * Stop an active voice chat session
 */

import type { CommandParams, CommandResult, JTAGContext, CommandInput} from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../system/core/shared/Commands';

/**
 * Voice Stop Command Parameters
 */
export interface VoiceStopParams extends CommandParams {
  // Session handle to stop (defaults to current session)
  handle?: string;
}

/**
 * Factory function for creating VoiceStopParams
 */
export const createVoiceStopParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Session handle to stop (defaults to current session)
    handle?: string;
  }
): VoiceStopParams => createPayload(context, sessionId, {
  handle: data.handle ?? '',
  ...data
});

/**
 * Voice Stop Command Result
 */
export interface VoiceStopResult extends CommandResult {
  success: boolean;
  // Whether the session was successfully stopped
  stopped: boolean;
  // Handle of the stopped session
  handle: string;
  // Session duration in seconds
  duration: number;
  error?: JTAGError;
}

/**
 * Factory function for creating VoiceStopResult with defaults
 */
export const createVoiceStopResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Whether the session was successfully stopped
    stopped?: boolean;
    // Handle of the stopped session
    handle?: string;
    // Session duration in seconds
    duration?: number;
    error?: JTAGError;
  }
): VoiceStopResult => createPayload(context, sessionId, {
  stopped: data.stopped ?? false,
  handle: data.handle ?? '',
  duration: data.duration ?? 0,
  ...data
});

/**
 * Smart Voice Stop-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createVoiceStopResultFromParams = (
  params: VoiceStopParams,
  differences: Omit<VoiceStopResult, 'context' | 'sessionId'>
): VoiceStopResult => transformPayload(params, differences);

/**
 * VoiceStop — Type-safe command executor
 *
 * Usage:
 *   import { VoiceStop } from '...shared/VoiceStopTypes';
 *   const result = await VoiceStop.execute({ ... });
 */
export const VoiceStop = {
  execute(params: CommandInput<VoiceStopParams>): Promise<VoiceStopResult> {
    return Commands.execute<VoiceStopParams, VoiceStopResult>('voice/stop', params as Partial<VoiceStopParams>);
  },
  commandName: 'voice/stop' as const,
} as const;
