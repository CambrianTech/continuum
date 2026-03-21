/**
 * Ai Key Save Command - Shared Types
 *
 * Save an API key for a cloud AI provider. Persists to ~/.continuum/config.env, sets process.env, and emits system:config:key-added event to trigger persona creation.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Ai Key Save Command Parameters
 */
export interface AiKeySaveParams extends CommandParams {
  // The config key name (e.g., 'ANTHROPIC_API_KEY', 'DEEPSEEK_API_KEY')
  provider: string;
  // The API key value to save
  value: string;
}

/**
 * Factory function for creating AiKeySaveParams
 */
export const createAiKeySaveParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // The config key name (e.g., 'ANTHROPIC_API_KEY', 'DEEPSEEK_API_KEY')
    provider: string;
    // The API key value to save
    value: string;
  }
): AiKeySaveParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,

  ...data
});

/**
 * Ai Key Save Command Result
 */
export interface AiKeySaveResult extends CommandResult {
  success: boolean;
  // Whether the key was saved successfully
  saved: boolean;
  // The config key name that was saved
  provider: string;
  error?: JTAGError;
}

/**
 * Factory function for creating AiKeySaveResult with defaults
 */
export const createAiKeySaveResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Whether the key was saved successfully
    saved?: boolean;
    // The config key name that was saved
    provider?: string;
    error?: JTAGError;
  }
): AiKeySaveResult => createPayload(context, sessionId, {
  saved: data.saved ?? false,
  provider: data.provider ?? '',
  ...data
});

/**
 * Smart Ai Key Save-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createAiKeySaveResultFromParams = (
  params: AiKeySaveParams,
  differences: Omit<AiKeySaveResult, 'context' | 'sessionId' | 'userId'>
): AiKeySaveResult => transformPayload(params, differences);

/**
 * Ai Key Save — Type-safe command executor
 *
 * Usage:
 *   import { AiKeySave } from '...shared/AiKeySaveTypes';
 *   const result = await AiKeySave.execute({ ... });
 */
export const AiKeySave = {
  execute(params: CommandInput<AiKeySaveParams>): Promise<AiKeySaveResult> {
    return Commands.execute<AiKeySaveParams, AiKeySaveResult>('ai/key/save', params as Partial<AiKeySaveParams>);
  },
  commandName: 'ai/key/save' as const,
} as const;
