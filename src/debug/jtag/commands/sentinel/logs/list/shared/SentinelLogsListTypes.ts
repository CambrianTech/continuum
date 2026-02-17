/**
 * List available log streams for a sentinel by handle.
 */

import type { CommandParams, CommandResult } from '../../../../../system/core/types/JTAGTypes';

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
