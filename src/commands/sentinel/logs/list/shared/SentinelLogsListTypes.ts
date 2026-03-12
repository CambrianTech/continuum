/**
 * List available log streams for a sentinel by handle.
 */

import type { CommandParams, CommandResult } from '../../../../../system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { CommandInput } from '@system/core/types/JTAGTypes';

/**
 * List available log streams for a sentinel by handle.
 */
export interface SentinelLogsListParams extends CommandParams {
  /** Sentinel handle (short ID or full ID) */
  handle: string;
}

/**
 * Log stream info
 */
export interface LogStreamInfo {
  /** Stream name (e.g., "execution", "build-1", "stderr") */
  name: string;

  /** File path */
  path: string;

  /** Size in bytes */
  size: number;

  /** Last modified timestamp */
  modifiedAt: string;
}

/**
 * List result
 */
export interface SentinelLogsListResult extends CommandResult {
  success: boolean;
  handle: string;
  logsDir: string;
  streams: LogStreamInfo[];
  error?: string;
}

/**
 * SentinelLogsList — Type-safe command executor
 *
 * Usage:
 *   import { SentinelLogsList } from '...shared/SentinelLogsListTypes';
 *   const result = await SentinelLogsList.execute({ ... });
 */
export const SentinelLogsList = {
  execute(params: CommandInput<SentinelLogsListParams>): Promise<SentinelLogsListResult> {
    return Commands.execute<SentinelLogsListParams, SentinelLogsListResult>('sentinel/logs/list', params as Partial<SentinelLogsListParams>);
  },
  commandName: 'sentinel/logs/list' as const,
} as const;
