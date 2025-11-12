/**
 * Genome Unregister Command Types
 *
 * Unregister persona from genome daemon - unloads all adapters for persona.
 */

import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';

export interface GenomeUnregisterParams extends CommandParams {
  personaId: UUID;
}

export interface GenomeUnregisterResult extends CommandResult {
  success: boolean;
  unregistered: boolean;
  error?: string;
}

/**
 * Helper to create GenomeUnregisterResult from params
 */
export const createGenomeUnregisterResultFromParams = (
  params: GenomeUnregisterParams,
  differences: Omit<Partial<GenomeUnregisterResult>, 'context' | 'sessionId'>
): GenomeUnregisterResult => transformPayload(params, {
  success: false,
  unregistered: false,
  ...differences
});
