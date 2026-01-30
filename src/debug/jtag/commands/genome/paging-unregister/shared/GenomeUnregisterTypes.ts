/**
 * Genome Unregister Command Types
 *
 * Unregister persona from genome daemon - unloads all adapters for persona.
 */

import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { CommandParams, CommandResult, CommandInput} from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import { Commands } from '../../../../system/core/shared/Commands';

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

/**
 * GenomeUnregister — Type-safe command executor
 *
 * Usage:
 *   import { GenomeUnregister } from '...shared/GenomeUnregisterTypes';
 *   const result = await GenomeUnregister.execute({ ... });
 */
export const GenomeUnregister = {
  execute(params: CommandInput<GenomeUnregisterParams>): Promise<GenomeUnregisterResult> {
    return Commands.execute<GenomeUnregisterParams, GenomeUnregisterResult>('genome/paging-unregister', params as Partial<GenomeUnregisterParams>);
  },
  commandName: 'genome/paging-unregister' as const,
} as const;
