/**
 * memory/store - Store a thought in WorkingMemory
 *
 * Thin command interface - validation happens in WorkingMemoryManager
 */

import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';

export interface MemoryStoreParams extends CommandParams {
  /** Persona ID (defaults to current user if omitted) */
  personaId?: UUID;

  /** Domain scope (e.g., 'chat', 'code', 'web') or null for global */
  domain?: string | null;

  /** Context ID (e.g., roomId) or null for domain-wide */
  contextId?: UUID | null;

  /** Type of thought (AI-generated, max 50 chars) */
  thoughtType: string;

  /** The actual thought content */
  thoughtContent: string;

  /** Importance (0.0-1.0) for recall prioritization */
  importance?: number;

  /** Whether this thought can be shared with other AIs */
  shareable?: boolean;

  /** Additional metadata */
  metadata?: Record<string, unknown>;
}

export interface MemoryStoreResult extends CommandResult {
  thoughtId: UUID;
  stored: boolean;
}
