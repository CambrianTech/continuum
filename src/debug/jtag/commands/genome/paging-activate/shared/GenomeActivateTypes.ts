/**
 * Genome Activate Command Types
 *
 * Activate adapter for persona - loads adapter into memory, evicting others if needed.
 *
 * Phase 7: Single adapter per persona (mock adapters)
 * Phase 8+: Multiple adapters stacked (real GPU)
 */

import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { CommandParams, CommandResult, CommandInput} from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import { Commands } from '../../../../system/core/shared/Commands';

export interface GenomeActivateParams extends CommandParams {
  personaId: UUID;
  adapterId: UUID;
}

export interface GenomeActivateResult extends CommandResult {
  success: boolean;
  loaded: boolean;          // True if loaded, false if thrashing detected
  evictedAdapters?: UUID[]; // Adapters that were evicted
  loadTimeMs?: number;
  thrashingDetected?: boolean;
  error?: string;
}

/**
 * Helper to create GenomeActivateResult from params
 */
export const createGenomeActivateResultFromParams = (
  params: GenomeActivateParams,
  differences: Omit<Partial<GenomeActivateResult>, 'context' | 'sessionId'>
): GenomeActivateResult => transformPayload(params, {
  success: false,
  loaded: false,
  ...differences
});

/**
 * GenomeActivate — Type-safe command executor
 *
 * Usage:
 *   import { GenomeActivate } from '...shared/GenomeActivateTypes';
 *   const result = await GenomeActivate.execute({ ... });
 */
export const GenomeActivate = {
  execute(params: CommandInput<GenomeActivateParams>): Promise<GenomeActivateResult> {
    return Commands.execute<GenomeActivateParams, GenomeActivateResult>('genome/paging-activate', params as Partial<GenomeActivateParams>);
  },
  commandName: 'genome/paging-activate' as const,
} as const;
