/**
 * Sentinel Logs Read Command - Types
 *
 * Read a log stream for a sentinel.
 */

import type { CommandParams, CommandResult } from '../../../../../system/core/types/JTAGTypes';

/**
 * Read params
 */
export interface SentinelLogsReadParams extends CommandParams {
  /** Sentinel handle (short ID or full ID) */
  handle: string;

  /** Stream name (e.g., "execution", "build-1", "stderr") */
  stream: string;

  /** Start from line number (0-indexed, default: 0) */
  offset?: number;

  /** Maximum lines to return (default: all) */
  limit?: number;
}

/**
 * Read result
 */
export interface SentinelLogsReadResult extends CommandResult {
  success: boolean;
  handle: string;
  stream: string;
  content: string;
  lineCount: number;
  totalLines: number;
  truncated: boolean;
  error?: string;
}
