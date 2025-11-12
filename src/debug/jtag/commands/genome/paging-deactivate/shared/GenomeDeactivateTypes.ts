/**
 * Genome Deactivate Command Types
 *
 * Deactivate adapter for persona - unloads adapter from memory.
 */

import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';

export interface GenomeDeactivateParams extends CommandParams {
  personaId: UUID;
  adapterId: UUID;
}

export interface GenomeDeactivateResult extends CommandResult {
  success: boolean;
  unloaded: boolean;
  error?: string;
}

/**
 * Helper to create GenomeDeactivateResult from params
 */
export const createGenomeDeactivateResultFromParams = (
  params: GenomeDeactivateParams,
  differences: Omit<Partial<GenomeDeactivateResult>, 'context' | 'sessionId'>
): GenomeDeactivateResult => transformPayload(params, {
  success: false,
  unloaded: false,
  ...differences
});
