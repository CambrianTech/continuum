/**
 * Ai Key Remove Command - Shared Types
 *
 * Remove an API key for a cloud AI provider. Removes from ~/.continuum/config.env, clears process.env, and emits system:config:key-removed event to deactivate personas.
 */

import type { CommandInput, CommandParams, JTAGContext } from '@system/core/types/JTAGTypes';
import { transformPayload } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import {
  type AiKeyParams,
  type AiKeyResult,
  type AiKeySyncMode,
  createAiKeyParams,
  createAiKeyResult
} from '../../common/AiKeyBase';

/**
 * Ai Key Remove Command Parameters
 */
export interface AiKeyRemoveParams extends CommandParams, AiKeyParams {
  // The config key name (e.g., 'ANTHROPIC_API_KEY', 'DEEPSEEK_API_KEY')
  provider: string;
  // Request immediate sync after local remove
  sync?: AiKeySyncMode;
}

/**
 * Factory function for creating AiKeyRemoveParams
 */
export const createAiKeyRemoveParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // The config key name (e.g., 'ANTHROPIC_API_KEY', 'DEEPSEEK_API_KEY')
    provider: string;
    sync?: AiKeySyncMode;
    targetNodes?: string[];
    dryRun?: boolean;
  }
): AiKeyRemoveParams => createAiKeyParams(context, sessionId, {
  ...data
});

/**
 * Ai Key Remove Command Result
 */
export interface AiKeyRemoveResult extends AiKeyResult {
  // Whether the key was removed successfully
  removed: boolean;
  // The config key name that was removed
  provider: string;
  synced?: boolean;
  syncMode?: AiKeySyncMode;
  targetNodes?: string[];
  error?: JTAGError;
}

/**
 * Factory function for creating AiKeyRemoveResult with defaults
 */
export const createAiKeyRemoveResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Whether the key was removed successfully
    removed?: boolean;
    // The config key name that was removed
    provider?: string;
    synced?: boolean;
    syncMode?: AiKeySyncMode;
    targetNodes?: string[];
    mergePlanId?: string;
    error?: JTAGError;
  }
): AiKeyRemoveResult => createAiKeyResult(context, sessionId, {
  removed: data.removed ?? false,
  provider: data.provider ?? '',
  ...data
});

/**
 * Smart Ai Key Remove-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createAiKeyRemoveResultFromParams = (
  params: AiKeyRemoveParams,
  differences: Omit<AiKeyRemoveResult, 'context' | 'sessionId' | 'userId'>
): AiKeyRemoveResult => transformPayload(params, differences);

/**
 * Ai Key Remove — Type-safe command executor
 *
 * Usage:
 *   import { AiKeyRemove } from '...shared/AiKeyRemoveTypes';
 *   const result = await AiKeyRemove.execute({ ... });
 */
export const AiKeyRemove = {
  execute(params: CommandInput<AiKeyRemoveParams>): Promise<AiKeyRemoveResult> {
    return Commands.execute<AiKeyRemoveParams, AiKeyRemoveResult>('ai/key/remove', params as Partial<AiKeyRemoveParams>);
  },
  commandName: 'ai/key/remove' as const,
} as const;
