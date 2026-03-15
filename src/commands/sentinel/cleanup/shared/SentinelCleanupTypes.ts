/**
 * Sentinel Cleanup — prune old sentinel logs, training datasets, and pipeline artifacts.
 *
 * Data hygiene for long-running systems. Sentinel logs and training exports
 * accumulate fast (GBs) and must be pruned after their useful life.
 */

import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { CommandInput } from '@system/core/types/JTAGTypes';

export interface SentinelCleanupParams extends CommandParams {
  /** Max age in hours for sentinel logs (default: 72 = 3 days) */
  maxAgeHours?: number;

  /** Max age in hours for training dataset JSONL files (default: 168 = 7 days) */
  datasetMaxAgeHours?: number;

  /** If true, only report what would be deleted (default: false) */
  dryRun?: boolean;

  /** If true, also clean up prompt capture logs (default: true) */
  cleanPromptCaptures?: boolean;

  /** Max age in hours for LoRA adapter checkpoints (default: 336 = 14 days).
   *  Only deletes intermediate checkpoints (checkpoint-N/), not final adapters. */
  adapterMaxAgeHours?: number;

  /** If true, also prune old LoRA adapters and checkpoints (default: true) */
  cleanAdapters?: boolean;
}

export interface CleanupStats {
  sentinelDirs: number;
  sentinelBytes: number;
  datasetFiles: number;
  datasetBytes: number;
  promptCaptureBytes: number;
  adapterDirs: number;
  adapterBytes: number;
}

export interface SentinelCleanupResult extends CommandResult {
  success: boolean;
  deleted: CleanupStats;
  remaining: CleanupStats;
  error?: string;
}

export const SentinelCleanup = {
  execute(params: CommandInput<SentinelCleanupParams>): Promise<SentinelCleanupResult> {
    return Commands.execute<SentinelCleanupParams, SentinelCleanupResult>('sentinel/cleanup', params as Partial<SentinelCleanupParams>);
  },
  commandName: 'sentinel/cleanup' as const,
} as const;
