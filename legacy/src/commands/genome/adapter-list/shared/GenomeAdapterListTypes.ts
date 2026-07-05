/**
 * Genome Adapter List Command - Shared Types
 *
 * List all LoRA adapters in the genome with sizes, domains, last-used timestamps, and cascade scores.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { transformPayload } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';

/** Summary info for a single adapter in the list */
export interface AdapterListEntry {
  name: string;
  domain: string;
  sizeMB: number;
  baseModel: string;
  personaId: string;
  personaName: string;
  createdAt: string;
  hasWeights: boolean;
  isActive: boolean;
  rank: number;
  loss?: number;
  epochs?: number;
}

export interface GenomeAdapterListParams extends CommandParams {
  personaId?: string;
  domain?: string;
  sortBy?: string;
  includeMetrics?: boolean;
}

export interface GenomeAdapterListResult extends CommandResult {
  success: boolean;
  adapters: AdapterListEntry[];
  totalCount: number;
  totalSizeMB: number;
  activeCount: number;
  error?: JTAGError;
}

export const createGenomeAdapterListResultFromParams = (
  params: GenomeAdapterListParams,
  differences: Omit<GenomeAdapterListResult, 'context' | 'sessionId' | 'userId'>
): GenomeAdapterListResult => transformPayload(params, differences);

export const GenomeAdapterList = {
  execute(params: CommandInput<GenomeAdapterListParams>): Promise<GenomeAdapterListResult> {
    return Commands.execute<GenomeAdapterListParams, GenomeAdapterListResult>('genome/adapter-list', params as Partial<GenomeAdapterListParams>);
  },
  commandName: 'genome/adapter-list' as const,
} as const;
