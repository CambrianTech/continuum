/**
 * Genome Training Overview Command - Shared Types
 *
 * Aggregate all training data across local and grid nodes in one call. Returns adapters with loss histories, academy sessions, and per-node stats. Used by the training dashboard to avoid sequential grid/send chains from the browser.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Genome Training Overview Command Parameters
 */
export interface GenomeTrainingOverviewParams extends CommandParams {
  // Include data from remote grid nodes (default: true)
  includeGrid?: boolean;
  // Filter to a specific persona UUID
  personaId?: string;
}

/**
 * Factory function for creating GenomeTrainingOverviewParams
 */
export const createGenomeTrainingOverviewParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Include data from remote grid nodes (default: true)
    includeGrid?: boolean;
    // Filter to a specific persona UUID
    personaId?: string;
  }
): GenomeTrainingOverviewParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  includeGrid: data.includeGrid ?? false,
  personaId: data.personaId ?? '',
  ...data
});

/**
 * Genome Training Overview Command Result
 */
export interface GenomeTrainingOverviewResult extends CommandResult {
  success: boolean;
  // All adapters with training metrics, loss histories, and node info
  adapters: object[];
  // All academy sessions (active and recent completed)
  sessions: object[];
  // Grid node summary (name, GPU, adapter count)
  nodes: object[];
  // Aggregate stats: total adapters, total sessions, best loss, avg maturity
  summary: object;
  error?: JTAGError;
}

/**
 * Factory function for creating GenomeTrainingOverviewResult with defaults
 */
export const createGenomeTrainingOverviewResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // All adapters with training metrics, loss histories, and node info
    adapters?: object[];
    // All academy sessions (active and recent completed)
    sessions?: object[];
    // Grid node summary (name, GPU, adapter count)
    nodes?: object[];
    // Aggregate stats: total adapters, total sessions, best loss, avg maturity
    summary?: object;
    error?: JTAGError;
  }
): GenomeTrainingOverviewResult => createPayload(context, sessionId, {
  adapters: data.adapters ?? [],
  sessions: data.sessions ?? [],
  nodes: data.nodes ?? [],
  summary: data.summary ?? {},
  ...data
});

/**
 * Smart Genome Training Overview-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createGenomeTrainingOverviewResultFromParams = (
  params: GenomeTrainingOverviewParams,
  differences: Omit<GenomeTrainingOverviewResult, 'context' | 'sessionId' | 'userId'>
): GenomeTrainingOverviewResult => transformPayload(params, differences);

/**
 * Genome Training Overview — Type-safe command executor
 *
 * Usage:
 *   import { GenomeTrainingOverview } from '...shared/GenomeTrainingOverviewTypes';
 *   const result = await GenomeTrainingOverview.execute({ ... });
 */
export const GenomeTrainingOverview = {
  execute(params: CommandInput<GenomeTrainingOverviewParams>): Promise<GenomeTrainingOverviewResult> {
    return Commands.execute<GenomeTrainingOverviewParams, GenomeTrainingOverviewResult>('genome/training-overview', params as Partial<GenomeTrainingOverviewParams>);
  },
  commandName: 'genome/training-overview' as const,
} as const;
