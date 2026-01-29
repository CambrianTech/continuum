/**
 * Genome Register Command Types
 *
 * Register persona with genome daemon - must be called before activating adapters.
 */

import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { CommandParams, CommandResult, CommandInput} from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import { Commands } from '../../../../system/core/shared/Commands';

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

/**
 * GenomeRegister — Type-safe command executor
 *
 * Usage:
 *   import { GenomeRegister } from '...shared/GenomeRegisterTypes';
 *   const result = await GenomeRegister.execute({ ... });
 */
export const GenomeRegister = {
  execute(params: CommandInput<GenomeRegisterParams>): Promise<GenomeRegisterResult> {
    return Commands.execute<GenomeRegisterParams, GenomeRegisterResult>('genome/paging-register', params as Partial<GenomeRegisterParams>);
  },
  commandName: 'genome/paging-register' as const,
} as const;
