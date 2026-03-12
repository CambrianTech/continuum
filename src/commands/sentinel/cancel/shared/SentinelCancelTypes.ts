/**
 * Cancel running sentinels by handle or filter.
 *
 * Supports three modes:
 * - Direct: provide a `handle` to cancel one sentinel
 * - Filtered: provide `type` and/or `status` to cancel matching sentinels
 * - Default: no params cancels all running sentinels
 */

import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { CommandInput } from '@system/core/types/JTAGTypes';

export interface SentinelCancelParams extends CommandParams {
  /** Specific handle to cancel */
  handle?: string;

  /** Filter by sentinel type (e.g., 'pipeline', 'build') */
  type?: string;

  /** Filter by status (default: 'running') */
  status?: 'running' | 'completed' | 'failed' | 'cancelled';
}

export interface CancelledSentinel {
  handle: string;
  type: string;
  previousStatus: string;
  cancelled: boolean;
  error?: string;
}

export interface SentinelCancelResult extends CommandResult {
  cancelled: CancelledSentinel[];
  totalCancelled: number;
  totalAttempted: number;
}

/**
 * SentinelCancel — Type-safe command executor
 *
 * Usage:
 *   import { SentinelCancel } from '...shared/SentinelCancelTypes';
 *   const result = await SentinelCancel.execute({ ... });
 */
export const SentinelCancel = {
  execute(params: CommandInput<SentinelCancelParams>): Promise<SentinelCancelResult> {
    return Commands.execute<SentinelCancelParams, SentinelCancelResult>('sentinel/cancel', params as Partial<SentinelCancelParams>);
  },
  commandName: 'sentinel/cancel' as const,
} as const;
