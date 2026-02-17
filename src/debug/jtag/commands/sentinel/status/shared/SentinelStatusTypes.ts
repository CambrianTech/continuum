/**
 * Check the status of a running or completed sentinel by handle ID.
 */

import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';
import type { SentinelType } from '../../run/shared/SentinelRunTypes';

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
