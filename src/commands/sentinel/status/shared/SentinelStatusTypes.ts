/**
 * Check the status of a running or completed sentinel by handle ID.
 */

import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';
import type { SentinelType } from '../../run/shared/SentinelRunTypes';
import { Commands } from '@system/core/shared/Commands';
import type { CommandInput } from '@system/core/types/JTAGTypes';

/**
 * Check the status of a running or completed sentinel by handle ID.
 */
export interface SentinelStatusParams extends CommandParams {
  /** Handle ID to check */
  handle: string;
}

export interface SentinelStatusResult extends CommandResult {
  handle: string;
  status: 'running' | 'completed' | 'failed' | 'cancelled' | 'not_found';
  progress?: number;
  exitCode?: number;
  workingDir?: string;
  logsDir?: string;
  error?: string;
}

/**
 * SentinelStatus — Type-safe command executor
 *
 * Usage:
 *   import { SentinelStatus } from '...shared/SentinelStatusTypes';
 *   const result = await SentinelStatus.execute({ ... });
 */
export const SentinelStatus = {
  execute(params: CommandInput<SentinelStatusParams>): Promise<SentinelStatusResult> {
    return Commands.execute<SentinelStatusParams, SentinelStatusResult>('sentinel/status', params as Partial<SentinelStatusParams>);
  },
  commandName: 'sentinel/status' as const,
} as const;
