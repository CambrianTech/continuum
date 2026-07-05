/**
 * Genome Adapter Prune Command - Shared Types
 *
 * Prune unused LoRA adapters to reclaim disk space.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { transformPayload } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';

export interface PrunedAdapterEntry {
  name: string;
  domain: string;
  sizeMB: number;
  createdAt: string;
  personaId: string;
  dirPath: string;
}

export interface GenomeAdapterPruneParams extends CommandParams {
  unusedSince?: string;
  dryRun?: boolean;
  personaId?: string;
  domain?: string;
  keepLatest?: number;
}

export interface GenomeAdapterPruneResult extends CommandResult {
  success: boolean;
  prunedCount: number;
  reclaimedMB: number;
  prunedAdapters: PrunedAdapterEntry[];
  keptCount: number;
  isDryRun: boolean;
  error?: JTAGError;
}

export const createGenomeAdapterPruneResultFromParams = (
  params: GenomeAdapterPruneParams,
  differences: Omit<GenomeAdapterPruneResult, 'context' | 'sessionId' | 'userId'>
): GenomeAdapterPruneResult => transformPayload(params, differences);

export const GenomeAdapterPrune = {
  execute(params: CommandInput<GenomeAdapterPruneParams>): Promise<GenomeAdapterPruneResult> {
    return Commands.execute<GenomeAdapterPruneParams, GenomeAdapterPruneResult>('genome/adapter-prune', params as Partial<GenomeAdapterPruneParams>);
  },
  commandName: 'genome/adapter-prune' as const,
} as const;
