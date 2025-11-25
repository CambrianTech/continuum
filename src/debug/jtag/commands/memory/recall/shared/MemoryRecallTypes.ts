/**
 * memory/recall - Query thoughts from WorkingMemory
 *
 * Supports scope-aware queries:
 * - Specific context: domain + contextId
 * - Domain-wide: domain + contextId=null
 * - Global: domain=null + contextId=null
 * - Wildcard: contextId='*' for all contexts in domain
 */

import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';

export interface MemoryRecallParams extends CommandParams {
  /** Persona ID (defaults to current user if omitted) */
  personaId?: UUID;

  /** Domain scope (e.g., 'chat', 'code', 'web') or null for global */
  domain?: string | null;

  /** Context ID (e.g., roomId), null for domain-wide, '*' for all */
  contextId?: UUID | null | '*';

  /** Filter by thought types */
  thoughtTypes?: string[];

  /** Minimum importance threshold (0.0-1.0) */
  minImportance?: number;

  /** Maximum number of thoughts to return */
  limit?: number;

  /** Sort order: 'recent' | 'important' | 'relevance' */
  sortBy?: 'recent' | 'important' | 'relevance';

  /** Include private thoughts (default: false) */
  includePrivate?: boolean;
}

export interface WorkingMemoryThought {
  id: UUID;
  personaId: UUID;
  domain: string | null;
  contextId: UUID | null;
  thoughtType: string;
  thoughtContent: string;
  importance: number;
  shareable: boolean;
  metadata?: Record<string, unknown>;
  createdAt: number;
  lastAccessedAt: number;
  scope: 'local' | 'domain' | 'global' | 'private';
}

export interface MemoryRecallResult extends CommandResult {
  thoughts: WorkingMemoryThought[];
  count: number;
  query: {
    domain?: string | null;
    contextId?: UUID | null | '*';
    thoughtTypes?: string[];
    minImportance?: number;
  };
}
