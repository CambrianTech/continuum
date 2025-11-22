/**
 * memory/update - Update existing thought in WorkingMemory
 *
 * Allows refining thoughts as understanding evolves
 */

import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';

export interface MemoryUpdateParams extends CommandParams {
  /** ID of the thought to update */
  thoughtId: UUID;

  /** Persona ID (defaults to current user if omitted) */
  personaId?: UUID;

  /** Updated thought content */
  thoughtContent?: string;

  /** Updated importance (0.0-1.0) */
  importance?: number;

  /** Updated metadata */
  metadata?: Record<string, unknown>;

  /** Change thought type */
  thoughtType?: string;
}

export interface MemoryUpdateResult extends CommandResult {
  thoughtId: UUID;
  updated: boolean;
  changes: string[];
}
