/**
 * memory/elevate-scope - Change scope level of a thought
 *
 * Used to promote local insights to broader scopes:
 * - local → domain: Pattern seen in multiple rooms
 * - domain → global: Principle applicable everywhere
 * - Or demote if thought proves too narrow
 *
 * CROSS-AI USAGE: Smarter orchestrator can elevate important patterns
 * discovered by smaller models, effectively mentoring them.
 */

import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';

export interface MemoryElevateScopeParams extends CommandParams {
  /** ID of the thought to elevate/demote */
  thoughtId: UUID;

  /** Persona ID (defaults to current user, but can modify others) */
  personaId?: UUID;

  /** New scope */
  targetScope: 'local' | 'domain' | 'global' | 'private';

  /** If elevating to local, specify contextId */
  contextId?: UUID;

  /** If elevating to domain, specify domain */
  domain?: string;

  /** Optional: Update thought content when changing scope */
  thoughtContent?: string;

  /** Optional: Who is performing this elevation (for attribution) */
  elevatedBy?: UUID;

  /** Optional: Reason for scope change */
  reason?: string;
}

export interface MemoryElevateScopeResult extends CommandResult {
  thoughtId: UUID;
  previousScope: 'local' | 'domain' | 'global' | 'private';
  newScope: 'local' | 'domain' | 'global' | 'private';
  elevated: boolean;
}
