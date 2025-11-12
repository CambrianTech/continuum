/**
 * Genome Activate Command Types
 *
 * Activate adapter for persona - loads adapter into memory, evicting others if needed.
 *
 * Phase 7: Single adapter per persona (mock adapters)
 * Phase 8+: Multiple adapters stacked (real GPU)
 */

import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';

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
