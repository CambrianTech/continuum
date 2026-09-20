/**
 * Genome Adapter Backfill Metrics Command - Shared Types
 *
 * One-time migration: scans all existing adapters and backfills training metrics
 * from checkpoint trainer_state.json and training_metrics.json files.
 * Also useful for maintenance when metrics get out of sync.
 */

import type { CommandParams, CommandResult, CommandInput } from '@system/core/types/JTAGTypes';
import { transformPayload } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';

export interface GenomeAdapterBackfillMetricsParams extends CommandParams {
  /** Only backfill adapters missing metrics (default: true) */
  missingOnly?: boolean;
}

export interface BackfillResult {
  name: string;
  updated: boolean;
  finalLoss?: number;
  epochs?: number;
  reason?: string;
}

export interface GenomeAdapterBackfillMetricsResult extends CommandResult {
  success: boolean;
  /** Total adapters scanned */
  scanned: number;
  /** Adapters that were updated with metrics */
  updated: number;
  /** Per-adapter results */
  results: BackfillResult[];
  error?: string;
}

export const createGenomeAdapterBackfillMetricsResultFromParams = (
  params: GenomeAdapterBackfillMetricsParams,
  differences: Omit<Partial<GenomeAdapterBackfillMetricsResult>, 'context' | 'sessionId'>
): GenomeAdapterBackfillMetricsResult => transformPayload(params, {
  success: false,
  scanned: 0,
  updated: 0,
  results: [],
  ...differences,
});

export const GenomeAdapterBackfillMetrics = {
  execute(params: CommandInput<GenomeAdapterBackfillMetricsParams>): Promise<GenomeAdapterBackfillMetricsResult> {
    return Commands.execute<GenomeAdapterBackfillMetricsParams, GenomeAdapterBackfillMetricsResult>(
      'genome/adapter-backfill-metrics',
      params as Partial<GenomeAdapterBackfillMetricsParams>
    );
  },
  commandName: 'genome/adapter-backfill-metrics' as const,
} as const;
