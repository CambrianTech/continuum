/**
 * Sentinel Status Command - Types
 */

import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';
import type { SentinelType } from '../../run/shared/SentinelRunTypes';

export interface SentinelStatusParams extends CommandParams {
  /** Handle ID to check */
  handle: string;
}

export interface SentinelStatusResult extends CommandResult {
  handle: string;
  type?: SentinelType;
  status: 'running' | 'completed' | 'failed' | 'not_found';
  progress?: number;
  duration?: number;
  data?: {
    success: boolean;
    summary?: string;
    filesCreated?: string[];
    errors?: string[];
    screenshot?: string;
  };
  error?: string;
}
