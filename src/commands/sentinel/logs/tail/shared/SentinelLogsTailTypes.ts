/**
 * Get the last N lines of a sentinel log stream, like Unix tail.
 */

import type { CommandParams, CommandResult } from '../../../../../system/core/types/JTAGTypes';

/**
 * Get the last N lines of a sentinel log stream, like Unix tail.
 */
export interface SentinelLogsTailParams extends CommandParams {
  /** Sentinel handle (short ID or full ID) */
  handle: string;

  /** Stream name (e.g., "execution", "build-1", "stderr") */
  stream: string;

  /** Number of lines from the end (default: 20) */
  lines?: number;
}

/**
 * Tail result
 */
export interface SentinelLogsTailResult extends CommandResult {
  success: boolean;
  handle: string;
  stream: string;
  content: string;
  lineCount: number;
  error?: string;
}
