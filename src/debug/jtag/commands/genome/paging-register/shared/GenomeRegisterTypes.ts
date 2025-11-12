/**
 * Genome Register Command Types
 *
 * Register persona with genome daemon - must be called before activating adapters.
 */

import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';

export interface GenomeRegisterParams extends CommandParams {
  personaId: UUID;
  displayName: string;
  quotaMB?: number;
  priority?: number;
}

export interface GenomeRegisterResult extends CommandResult {
  success: boolean;
  registered: boolean;
  error?: string;
}

/**
 * Helper to create GenomeRegisterResult from params
 */
export const createGenomeRegisterResultFromParams = (
  params: GenomeRegisterParams,
  differences: Omit<Partial<GenomeRegisterResult>, 'context' | 'sessionId'>
): GenomeRegisterResult => transformPayload(params, {
  success: false,
  registered: false,
  ...differences
});
