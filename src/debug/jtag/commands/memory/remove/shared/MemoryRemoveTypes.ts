/**
 * memory/remove - Delete thought from WorkingMemory
 *
 * Used when thoughts prove incorrect or no longer relevant
 */

import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';

export interface MemoryRemoveParams extends CommandParams {
  /** ID of the thought to remove */
  thoughtId: UUID;

  /** Persona ID (defaults to current user if omitted) */
  personaId?: UUID;

  /** Optional: Store reason for removal */
  reason?: string;

  /** Optional: Replace with corrected thought */
  correction?: {
    thoughtContent: string;
    thoughtType: string;
    importance: number;
  };
}

export interface MemoryRemoveResult extends CommandResult {
  thoughtId: UUID;
  removed: boolean;
  replacementId?: UUID;
}
