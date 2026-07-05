/**
 * Sentinel Extend Budget Command - Shared Types
 *
 * Extend budget limits for a running or paused pipeline. Merges new limits into existing checkpoint budget.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';

import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { BudgetLimits } from '@shared/generated/sentinel/BudgetLimits';

/**
 * Sentinel Extend Budget Command Parameters
 */
export interface SentinelExtendBudgetParams extends CommandParams {
  // Sentinel handle ID to extend budget for
  handle: string;
  // New max time limit in seconds (e.g., 3600 for 1 hour)
  maxTimeSecs?: number;
  // New max cost limit in USD (e.g., 5.00)
  maxCostUsd?: number;
  // New max token limit (e.g., 1000000)
  maxTokens?: number;
  // New max iteration limit (full pipeline loops, not agent turns)
  maxIterations?: number;
}

/**
 * Factory function for creating SentinelExtendBudgetParams
 */
export const createSentinelExtendBudgetParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Sentinel handle ID to extend budget for
    handle: string;
    // New max time limit in seconds (e.g., 3600 for 1 hour)
    maxTimeSecs?: number;
    // New max cost limit in USD (e.g., 5.00)
    maxCostUsd?: number;
    // New max token limit (e.g., 1000000)
    maxTokens?: number;
    // New max iteration limit (full pipeline loops, not agent turns)
    maxIterations?: number;
  }
): SentinelExtendBudgetParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data,
});

/**
 * Sentinel Extend Budget Command Result
 */
export interface SentinelExtendBudgetResult extends CommandResult {
  success: boolean;
  // The sentinel handle whose budget was extended
  handle: string;
  // The new merged budget limits after extension
  budgetLimits: BudgetLimits;
  error?: string;
}

/**
 * Factory function for creating SentinelExtendBudgetResult with defaults
 */
export const createSentinelExtendBudgetResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // The sentinel handle whose budget was extended
    handle?: string;
    // The new merged budget limits after extension
    budgetLimits?: BudgetLimits;
    error?: string;
  }
): SentinelExtendBudgetResult => createPayload(context, sessionId, {
  handle: data.handle ?? '',
  budgetLimits: data.budgetLimits ?? {} as BudgetLimits,
  ...data
});

/**
 * Smart Sentinel Extend Budget-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createSentinelExtendBudgetResultFromParams = (
  params: SentinelExtendBudgetParams,
  differences: Omit<SentinelExtendBudgetResult, 'context' | 'sessionId' | 'userId'>
): SentinelExtendBudgetResult => transformPayload(params, differences);

/**
 * Sentinel Extend Budget — Type-safe command executor
 *
 * Usage:
 *   import { SentinelExtendBudget } from '...shared/SentinelExtendBudgetTypes';
 *   const result = await SentinelExtendBudget.execute({ ... });
 */
export const SentinelExtendBudget = {
  execute(params: CommandInput<SentinelExtendBudgetParams>): Promise<SentinelExtendBudgetResult> {
    return Commands.execute<SentinelExtendBudgetParams, SentinelExtendBudgetResult>('sentinel/extend-budget', params as Partial<SentinelExtendBudgetParams>);
  },
  commandName: 'sentinel/extend-budget' as const,
} as const;
