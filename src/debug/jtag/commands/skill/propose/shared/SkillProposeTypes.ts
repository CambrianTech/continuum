/**
 * Skill Propose Command - Shared Types
 *
 * Propose a new skill (command) specification. Creates a SkillEntity with status 'proposed'. For team-scoped skills, creates a DecisionProposal for governance approval.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Skill Propose Command Parameters
 */
export interface SkillProposeParams extends CommandParams {
  // Command name (e.g., 'analysis/complexity')
  name: string;
  // What the skill does
  description: string;
  // Input parameters spec array [{name, type, optional?, description?}]
  skillParams: Record<string, unknown>[];
  // Output fields spec array [{name, type, description?}]
  skillResults: Record<string, unknown>[];
  // Natural language description of the implementation logic
  implementation: string;
  // Who can use it: 'personal' (default) or 'team' (requires approval)
  scope?: string;
  // Usage examples array [{description, command, expectedResult?}]
  examples?: Record<string, unknown>[];
  // AI persona proposing this skill
  personaId: string;
}

/**
 * Factory function for creating SkillProposeParams
 */
export const createSkillProposeParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Command name (e.g., 'analysis/complexity')
    name: string;
    // What the skill does
    description: string;
    // Input parameters spec array [{name, type, optional?, description?}]
    skillParams: Record<string, unknown>[];
    // Output fields spec array [{name, type, description?}]
    skillResults: Record<string, unknown>[];
    // Natural language description of the implementation logic
    implementation: string;
    // Who can use it: 'personal' (default) or 'team' (requires approval)
    scope?: string;
    // Usage examples array [{description, command, expectedResult?}]
    examples?: Record<string, unknown>[];
    // AI persona proposing this skill
    personaId: string;
  }
): SkillProposeParams => createPayload(context, sessionId, {
  scope: data.scope ?? '',
  examples: data.examples ?? undefined,
  ...data
});

/**
 * Skill Propose Command Result
 */
export interface SkillProposeResult extends CommandResult {
  success: boolean;
  // ID of the created SkillEntity
  skillId: string;
  // Skill command name
  name: string;
  // Lifecycle status after proposal
  status: string;
  // Skill scope (personal or team)
  scope: string;
  // DecisionProposal ID if team-scoped
  proposalId: string;
  // Human-readable result message
  message: string;
  error?: JTAGError;
}

/**
 * Factory function for creating SkillProposeResult with defaults
 */
export const createSkillProposeResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // ID of the created SkillEntity
    skillId?: string;
    // Skill command name
    name?: string;
    // Lifecycle status after proposal
    status?: string;
    // Skill scope (personal or team)
    scope?: string;
    // DecisionProposal ID if team-scoped
    proposalId?: string;
    // Human-readable result message
    message?: string;
    error?: JTAGError;
  }
): SkillProposeResult => createPayload(context, sessionId, {
  skillId: data.skillId ?? '',
  name: data.name ?? '',
  status: data.status ?? '',
  scope: data.scope ?? '',
  proposalId: data.proposalId ?? '',
  message: data.message ?? '',
  ...data
});

/**
 * Smart Skill Propose-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createSkillProposeResultFromParams = (
  params: SkillProposeParams,
  differences: Omit<SkillProposeResult, 'context' | 'sessionId'>
): SkillProposeResult => transformPayload(params, differences);

/**
 * Skill Propose — Type-safe command executor
 *
 * Usage:
 *   import { SkillPropose } from '...shared/SkillProposeTypes';
 *   const result = await SkillPropose.execute({ ... });
 */
export const SkillPropose = {
  execute(params: CommandInput<SkillProposeParams>): Promise<SkillProposeResult> {
    return Commands.execute<SkillProposeParams, SkillProposeResult>('skill/propose', params as Partial<SkillProposeParams>);
  },
  commandName: 'skill/propose' as const,
} as const;
