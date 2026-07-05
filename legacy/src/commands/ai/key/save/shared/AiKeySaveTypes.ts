/**
 * Ai Key Save Command - Shared Types
 *
 * Save an API key for a cloud AI provider. Persists to ~/.continuum/config.env, sets process.env, and emits system:config:key-added event to trigger persona creation.
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
 * Ai Key Save Command Parameters
 */
export interface AiKeySaveParams extends CommandParams, AiKeyParams {
  // The config key name (e.g., 'ANTHROPIC_API_KEY', 'DEEPSEEK_API_KEY')
  provider: string;
  // The API key value to save
  value: string;
  // Request immediate sync after local save
  sync?: AiKeySyncMode;
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
    sync?: AiKeySyncMode;
    targetNodes?: string[];
    dryRun?: boolean;
  }
): AiKeySaveParams => createAiKeyParams(context, sessionId, {
  ...data
});

/**
 * Ai Key Save Command Result
 */
export interface AiKeySaveResult extends AiKeyResult {
  // Whether the key was saved successfully
  saved: boolean;
  // The config key name that was saved
  provider: string;
  synced?: boolean;
  syncMode?: AiKeySyncMode;
  targetNodes?: string[];
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
    synced?: boolean;
    syncMode?: AiKeySyncMode;
    targetNodes?: string[];
    mergePlanId?: string;
    error?: JTAGError;
  }
): AiKeySaveResult => createAiKeyResult(context, sessionId, {
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
