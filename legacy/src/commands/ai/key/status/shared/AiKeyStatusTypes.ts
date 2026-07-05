/**
 * Ai Key Status Command - Shared Types
 *
 * Report redacted API-key availability and fingerprints without exposing raw or masked secret values.
 */

import type { CommandInput, CommandParams, JTAGContext } from '@system/core/types/JTAGTypes';
import { transformPayload } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import {
  type AiKeyParams,
  type AiKeyResult,
  createAiKeyParams,
  createAiKeyResult
} from '../../common/AiKeyBase';
import type { AiKeyCategory } from '../../common/AiKeyProviders';

/**
 * Ai Key Status Command Parameters
 */
export interface AiKeyStatusParams extends CommandParams, AiKeyParams {
  // Optional provider name or config key. Omit to list all known keys.
  provider?: string;
}

/**
 * Factory function for creating AiKeyStatusParams
 */
export const createAiKeyStatusParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Optional provider name or config key. Omit to list all known keys.
    provider?: string;
  },
): AiKeyStatusParams => createAiKeyParams(context, sessionId, data);

export interface AiKeyStatusEntry {
  provider: string;
  key: string;
  category: AiKeyCategory;
  configured: boolean;
  empty: boolean;
  fingerprint?: string;
  source: 'continuum-home' | 'process-env' | 'missing';
  description: string;
}

/**
 * Ai Key Status Command Result
 */
export interface AiKeyStatusResult extends AiKeyResult {
  // Redacted key status entries containing provider names, config key names, booleans, source, and short fingerprints only.
  entries: AiKeyStatusEntry[];
  // Number of configured keys.
  configuredCount: number;
  // Number of checked keys.
  totalCount: number;
  error?: JTAGError;
}

/**
 * Factory function for creating AiKeyStatusResult with defaults
 */
export const createAiKeyStatusResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Redacted key status entries containing provider names, config key names, booleans, source, and short fingerprints only.
    entries?: AiKeyStatusEntry[];
    // Number of configured keys.
    configuredCount?: number;
    // Number of checked keys.
    totalCount?: number;
    error?: JTAGError;
  }
): AiKeyStatusResult => createAiKeyResult(context, sessionId, {
  entries: data.entries ?? [],
  configuredCount: data.configuredCount ?? 0,
  totalCount: data.totalCount ?? 0,
  ...data
});

/**
 * Smart Ai Key Status-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createAiKeyStatusResultFromParams = (
  params: AiKeyStatusParams,
  differences: Omit<AiKeyStatusResult, 'context' | 'sessionId' | 'userId'>
): AiKeyStatusResult => transformPayload(params, differences);

/**
 * Ai Key Status — Type-safe command executor
 *
 * Usage:
 *   import { AiKeyStatus } from '...shared/AiKeyStatusTypes';
 *   const result = await AiKeyStatus.execute({ ... });
 */
export const AiKeyStatus = {
  execute(params: CommandInput<AiKeyStatusParams>): Promise<AiKeyStatusResult> {
    return Commands.execute<AiKeyStatusParams, AiKeyStatusResult>('ai/key/status', params as Partial<AiKeyStatusParams>);
  },
  commandName: 'ai/key/status' as const,
} as const;
