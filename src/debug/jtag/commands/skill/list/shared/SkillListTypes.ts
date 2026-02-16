/**
 * Skill List Command - Shared Types
 *
 * List skills with optional filters by status, scope, and creator. Returns SkillEntity records from the database.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Skill List Command Parameters
 */
export interface SkillListParams extends CommandParams {
  // Filter by lifecycle status (proposed, approved, generated, validated, active, failed, deprecated)
  status?: string;
  // Filter by scope (personal, team)
  scope?: string;
  // Filter by creator persona ID
  createdById?: string;
  // Maximum results to return (default: 20)
  limit?: number;
}

/**
 * Factory function for creating SkillListParams
 */
export const createSkillListParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Filter by lifecycle status (proposed, approved, generated, validated, active, failed, deprecated)
    status?: string;
    // Filter by scope (personal, team)
    scope?: string;
    // Filter by creator persona ID
    createdById?: string;
    // Maximum results to return (default: 20)
    limit?: number;
  }
): SkillListParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  status: data.status ?? '',
  scope: data.scope ?? '',
  createdById: data.createdById ?? '',
  limit: data.limit ?? 0,
  ...data
});

/**
 * Skill List Command Result
 */
export interface SkillListResult extends CommandResult {
  success: boolean;
  // Array of SkillEntity records matching the filter
  skills: Record<string, unknown>[];
  // Total matching skills
  total: number;
  // Human-readable result summary
  message: string;
  error?: JTAGError;
}

/**
 * Factory function for creating SkillListResult with defaults
 */
export const createSkillListResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Array of SkillEntity records matching the filter
    skills?: Record<string, unknown>[];
    // Total matching skills
    total?: number;
    // Human-readable result summary
    message?: string;
    error?: JTAGError;
  }
): SkillListResult => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  skills: data.skills ?? [],
  total: data.total ?? 0,
  message: data.message ?? '',
  ...data
});

/**
 * Smart Skill List-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createSkillListResultFromParams = (
  params: SkillListParams,
  differences: Omit<SkillListResult, 'context' | 'sessionId'>
): SkillListResult => transformPayload(params, differences);

/**
 * Skill List — Type-safe command executor
 *
 * Usage:
 *   import { SkillList } from '...shared/SkillListTypes';
 *   const result = await SkillList.execute({ ... });
 */
export const SkillList = {
  execute(params: CommandInput<SkillListParams>): Promise<SkillListResult> {
    return Commands.execute<SkillListParams, SkillListResult>('skill/list', params as Partial<SkillListParams>);
  },
  commandName: 'skill/list' as const,
} as const;
