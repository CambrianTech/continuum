/**
 * Genome Adapter Update Metrics Command - Shared Types
 *
 * Writes phenotype validation results and other metrics back to an adapter's manifest.
 * Called by StudentPipeline after quality gate passes to persist phenotype scores.
 */

import type { CommandParams, CommandResult, CommandInput } from '@system/core/types/JTAGTypes';
import { transformPayload } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';

export interface GenomeAdapterUpdateMetricsParams extends CommandParams {
  /** Absolute path to the adapter directory containing manifest.json */
  adapterPath: string;
  /** Phenotype validation score (post-training) */
  phenotypeScore?: number;
  /** Phenotype improvement delta (adapted - baseline) */
  phenotypeImprovement?: number;
}

export interface GenomeAdapterUpdateMetricsResult extends CommandResult {
  success: boolean;
  /** Updated adapter name */
  name?: string;
  error?: string;
}

export const createGenomeAdapterUpdateMetricsResultFromParams = (
  params: GenomeAdapterUpdateMetricsParams,
  differences: Omit<Partial<GenomeAdapterUpdateMetricsResult>, 'context' | 'sessionId'>
): GenomeAdapterUpdateMetricsResult => transformPayload(params, {
  success: false,
  ...differences,
});

export const GenomeAdapterUpdateMetrics = {
  execute(params: CommandInput<GenomeAdapterUpdateMetricsParams>): Promise<GenomeAdapterUpdateMetricsResult> {
    return Commands.execute<GenomeAdapterUpdateMetricsParams, GenomeAdapterUpdateMetricsResult>(
      'genome/adapter-update-metrics',
      params as Partial<GenomeAdapterUpdateMetricsParams>
    );
  },
  commandName: 'genome/adapter-update-metrics' as const,
} as const;
