/**
 * Genome Deactivate Command Types
 *
 * Deactivate adapter for persona - unloads adapter from memory.
 */

import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { CommandParams, CommandResult, CommandInput} from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import { Commands } from '../../../../system/core/shared/Commands';

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

/**
 * GenomeDeactivate — Type-safe command executor
 *
 * Usage:
 *   import { GenomeDeactivate } from '...shared/GenomeDeactivateTypes';
 *   const result = await GenomeDeactivate.execute({ ... });
 */
export const GenomeDeactivate = {
  execute(params: CommandInput<GenomeDeactivateParams>): Promise<GenomeDeactivateResult> {
    return Commands.execute<GenomeDeactivateParams, GenomeDeactivateResult>('genome/paging-deactivate', params as Partial<GenomeDeactivateParams>);
  },
  commandName: 'genome/paging-deactivate' as const,
} as const;
