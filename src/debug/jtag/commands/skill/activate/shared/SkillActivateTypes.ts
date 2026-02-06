/**
 * Skill Activate Command - Shared Types
 *
 * Activate a validated skill by registering it as a live command. The skill becomes available for use by the creator (personal) or all personas (team).
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Skill Activate Command Parameters
 */
export interface SkillActivateParams extends CommandParams {
  // ID of the SkillEntity to activate
  skillId: string;
}

/**
 * Factory function for creating SkillActivateParams
 */
export const createSkillActivateParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // ID of the SkillEntity to activate
    skillId: string;
  }
): SkillActivateParams => createPayload(context, sessionId, {

  ...data
});

/**
 * Skill Activate Command Result
 */
export interface SkillActivateResult extends CommandResult {
  success: boolean;
  // ID of the SkillEntity
  skillId: string;
  // Skill command name
  name: string;
  // Lifecycle status after activation
  status: string;
  // Timestamp when the skill was activated
  activatedAt: number;
  // Human-readable result message
  message: string;
  error?: JTAGError;
}

/**
 * Factory function for creating SkillActivateResult with defaults
 */
export const createSkillActivateResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // ID of the SkillEntity
    skillId?: string;
    // Skill command name
    name?: string;
    // Lifecycle status after activation
    status?: string;
    // Timestamp when the skill was activated
    activatedAt?: number;
    // Human-readable result message
    message?: string;
    error?: JTAGError;
  }
): SkillActivateResult => createPayload(context, sessionId, {
  skillId: data.skillId ?? '',
  name: data.name ?? '',
  status: data.status ?? '',
  activatedAt: data.activatedAt ?? 0,
  message: data.message ?? '',
  ...data
});

/**
 * Smart Skill Activate-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createSkillActivateResultFromParams = (
  params: SkillActivateParams,
  differences: Omit<SkillActivateResult, 'context' | 'sessionId'>
): SkillActivateResult => transformPayload(params, differences);

/**
 * Skill Activate — Type-safe command executor
 *
 * Usage:
 *   import { SkillActivate } from '...shared/SkillActivateTypes';
 *   const result = await SkillActivate.execute({ ... });
 */
export const SkillActivate = {
  execute(params: CommandInput<SkillActivateParams>): Promise<SkillActivateResult> {
    return Commands.execute<SkillActivateParams, SkillActivateResult>('skill/activate', params as Partial<SkillActivateParams>);
  },
  commandName: 'skill/activate' as const,
} as const;
