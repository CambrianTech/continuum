/**
 * AI Providers Status - Check which API keys are configured
 *
 * Returns status for each provider WITHOUT exposing actual key values.
 * Safe to call from browser - only returns boolean status.
 */

import type { CommandParams, CommandResult, CommandInput} from '@system/core/types/JTAGTypes';
import { Commands } from '../../../../../system/core/shared/Commands';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

export interface AIProvidersStatusParams extends CommandParams {
  // No additional params needed - returns all provider statuses
}

export interface ProviderStatus {
  provider: string;
  key: string;
  isConfigured: boolean;
  category: 'local' | 'cloud';
  description: string;
  getKeyUrl?: string;
  billingUrl?: string;
  /** Masked key preview like "sk-...QfQA" - safe to display */
  maskedKey?: string;
}

export interface AIProvidersStatusResult extends CommandResult {
  providers: ProviderStatus[];
  configuredCount: number;
  totalCount: number;
}

/**
 * AIProvidersStatus — Type-safe command executor
 *
 * Usage:
 *   import { AIProvidersStatus } from '...shared/AIProvidersStatusTypes';
 *   const result = await AIProvidersStatus.execute({ ... });
 */
export const AIProvidersStatus = {
  execute(params: CommandInput<AIProvidersStatusParams>): Promise<AIProvidersStatusResult> {
    return Commands.execute<AIProvidersStatusParams, AIProvidersStatusResult>('ai/providers/status', params as Partial<AIProvidersStatusParams>);
  },
  commandName: 'ai/providers/status' as const,
} as const;

/**
 * Factory function for creating AiProvidersStatusParams
 */
export const createAIProvidersStatusParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<AIProvidersStatusParams, 'context' | 'sessionId' | 'userId'>
): AIProvidersStatusParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Factory function for creating AiProvidersStatusResult with defaults
 */
export const createAIProvidersStatusResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<AIProvidersStatusResult, 'context' | 'sessionId' | 'userId'>
): AIProvidersStatusResult => createPayload(context, sessionId, {
  ...data
});

/**
 * Smart ai/providers/status-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createAIProvidersStatusResultFromParams = (
  params: AIProvidersStatusParams,
  differences: Omit<AIProvidersStatusResult, 'context' | 'sessionId' | 'userId'>
): AIProvidersStatusResult => transformPayload(params, differences);

