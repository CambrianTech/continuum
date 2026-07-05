/**
 * Sentinel Resume Command - Shared Types
 *
 * Resume a pipeline from a durable checkpoint. Only works for pipelines in Interrupted, Paused, or BudgetExhausted status.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';

import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Sentinel Resume Command Parameters
 */
export interface SentinelResumeParams extends CommandParams {
  // Sentinel handle ID to resume from checkpoint
  handle: string;
}

/**
 * Factory function for creating SentinelResumeParams
 */
export const createSentinelResumeParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Sentinel handle ID to resume from checkpoint
    handle: string;
  }
): SentinelResumeParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,

  ...data
});

/**
 * Sentinel Resume Command Result
 */
export interface SentinelResumeResult extends CommandResult {
  success: boolean;
  // The sentinel handle that was resumed
  handle: string;
  // New status after resume (typically 'running')
  status: string;
  // Step index the pipeline resumed from
  resumedFromStep: number;
  error?: string;
}

/**
 * Factory function for creating SentinelResumeResult with defaults
 */
export const createSentinelResumeResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // The sentinel handle that was resumed
    handle?: string;
    // New status after resume (typically 'running')
    status?: string;
    // Step index the pipeline resumed from
    resumedFromStep?: number;
    error?: string;
  }
): SentinelResumeResult => createPayload(context, sessionId, {
  handle: data.handle ?? '',
  status: data.status ?? '',
  resumedFromStep: data.resumedFromStep ?? 0,
  ...data
});

/**
 * Smart Sentinel Resume-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createSentinelResumeResultFromParams = (
  params: SentinelResumeParams,
  differences: Omit<SentinelResumeResult, 'context' | 'sessionId' | 'userId'>
): SentinelResumeResult => transformPayload(params, differences);

/**
 * Sentinel Resume — Type-safe command executor
 *
 * Usage:
 *   import { SentinelResume } from '...shared/SentinelResumeTypes';
 *   const result = await SentinelResume.execute({ ... });
 */
export const SentinelResume = {
  execute(params: CommandInput<SentinelResumeParams>): Promise<SentinelResumeResult> {
    return Commands.execute<SentinelResumeParams, SentinelResumeResult>('sentinel/resume', params as Partial<SentinelResumeParams>);
  },
  commandName: 'sentinel/resume' as const,
} as const;
