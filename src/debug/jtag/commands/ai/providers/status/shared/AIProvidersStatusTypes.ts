/**
 * AI Providers Status - Check which API keys are configured
 *
 * Returns status for each provider WITHOUT exposing actual key values.
 * Safe to call from browser - only returns boolean status.
 */

import type { CommandParams, CommandResult, CommandInput} from '@system/core/types/JTAGTypes';
import { Commands } from '../../../../../system/core/shared/Commands';

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
