/**
 * Voice Start Command - Shared Types
 *
 * Start voice chat session for real-time audio communication with AI
 */

import type { CommandParams, CommandResult, JTAGContext, CommandInput} from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../system/core/shared/Commands';

/**
 * Voice Start Command Parameters
 */
export interface VoiceStartParams extends CommandParams {
  // Room ID or name to join (defaults to 'general')
  room?: string;
  // LLM model to use for responses (defaults to system default)
  model?: string;
  // TTS voice ID for AI responses
  voice?: string;
}

/**
 * Factory function for creating VoiceStartParams
 */
export const createVoiceStartParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Room ID or name to join (defaults to 'general')
    room?: string;
    // LLM model to use for responses (defaults to system default)
    model?: string;
    // TTS voice ID for AI responses
    voice?: string;
  }
): VoiceStartParams => createPayload(context, sessionId, {
  room: data.room ?? '',
  model: data.model ?? '',
  voice: data.voice ?? '',
  ...data
});

/**
 * Voice Start Command Result
 */
export interface VoiceStartResult extends CommandResult {
  success: boolean;
  // Session handle (UUID) for correlation
  handle: string;
  // WebSocket URL to connect for audio streaming
  wsUrl: string;
  // Resolved room ID
  roomId: string;
  error?: JTAGError;
}

/**
 * Factory function for creating VoiceStartResult with defaults
 */
export const createVoiceStartResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Session handle (UUID) for correlation
    handle?: string;
    // WebSocket URL to connect for audio streaming
    wsUrl?: string;
    // Resolved room ID
    roomId?: string;
    error?: JTAGError;
  }
): VoiceStartResult => createPayload(context, sessionId, {
  handle: data.handle ?? '',
  wsUrl: data.wsUrl ?? '',
  roomId: data.roomId ?? '',
  ...data
});

/**
 * Smart Voice Start-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createVoiceStartResultFromParams = (
  params: VoiceStartParams,
  differences: Omit<VoiceStartResult, 'context' | 'sessionId'>
): VoiceStartResult => transformPayload(params, differences);

/**
 * VoiceStart — Type-safe command executor
 *
 * Usage:
 *   import { VoiceStart } from '...shared/VoiceStartTypes';
 *   const result = await VoiceStart.execute({ ... });
 */
export const VoiceStart = {
  execute(params: CommandInput<VoiceStartParams>): Promise<VoiceStartResult> {
    return Commands.execute<VoiceStartParams, VoiceStartResult>('voice/start', params as Partial<VoiceStartParams>);
  },
  commandName: 'voice/start' as const,
} as const;
