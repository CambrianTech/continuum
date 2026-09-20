/**
 * Genome Academy Team Command - Shared Types
 *
 * Start a collaborative team training project. Decomposes a project description into roles, trains each student for their role, then orchestrates collaborative building. Teacher grades both the overall project and individual role performance. Students communicate via the academy chat room.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Genome Academy Team Command Parameters
 */
export interface GenomeAcademyTeamParams extends CommandParams {
  // What the team should build (e.g., 'side-scrolling game with mushroom people afraid of sunlight')
  projectDescription: string;
  // Skill domain (e.g., 'game-development', 'web-app', 'music-production')
  skill: string;
  // Explicit team members: [{ personaId, personaName, role, roleDescription }]. If omitted, teacher LLM decomposes project into roles and RecipeAssembler matches available personas.
  team?: object;
  // Recipe with pre-defined roles (e.g., 'coding'). Roles extracted from recipe, personas matched by RecipeAssembler.
  recipeId?: string;
  // Base model for student training (default: LOCAL_MODELS.DEFAULT)
  baseModel?: string;
  // Teacher LLM model (required — teacher must be a capable cloud model)
  model: string;
  // Teacher LLM provider (required — e.g., 'deepseek', 'anthropic')
  provider: string;
  // Training epochs per topic (default: 3)
  epochs?: number;
  // Number of build milestones (default: 3)
  buildMilestones?: number;
}

/**
 * Factory function for creating GenomeAcademyTeamParams
 */
export const createGenomeAcademyTeamParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // What the team should build (e.g., 'side-scrolling game with mushroom people afraid of sunlight')
    projectDescription: string;
    // Skill domain (e.g., 'game-development', 'web-app', 'music-production')
    skill: string;
    // Explicit team members: [{ personaId, personaName, role, roleDescription }]. If omitted, teacher LLM decomposes project into roles and RecipeAssembler matches available personas.
    team?: object;
    // Recipe with pre-defined roles (e.g., 'coding'). Roles extracted from recipe, personas matched by RecipeAssembler.
    recipeId?: string;
    // Base model for student training (default: LOCAL_MODELS.DEFAULT)
    baseModel?: string;
    // Teacher LLM model (required — teacher must be a capable cloud model)
    model: string;
    // Teacher LLM provider (required — e.g., 'deepseek', 'anthropic')
    provider: string;
    // Training epochs per topic (default: 3)
    epochs?: number;
    // Number of build milestones (default: 3)
    buildMilestones?: number;
  }
): GenomeAcademyTeamParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  team: data.team ?? {},
  recipeId: data.recipeId ?? '',
  baseModel: data.baseModel ?? '',
  epochs: data.epochs ?? 0,
  buildMilestones: data.buildMilestones ?? 0,
  ...data
});

/**
 * Genome Academy Team Command Result
 */
export interface GenomeAcademyTeamResult extends CommandResult {
  success: boolean;
  // The created team project entity ID
  teamProjectId: string;
  // Sentinel handle for the teacher pipeline
  teacherHandle: string;
  // Array of { personaId, personaName, role, studentHandle, sessionId } for each team member
  memberHandles: object;
  error?: JTAGError;
}

/**
 * Factory function for creating GenomeAcademyTeamResult with defaults
 */
export const createGenomeAcademyTeamResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // The created team project entity ID
    teamProjectId?: string;
    // Sentinel handle for the teacher pipeline
    teacherHandle?: string;
    // Array of { personaId, personaName, role, studentHandle, sessionId } for each team member
    memberHandles?: object;
    error?: JTAGError;
  }
): GenomeAcademyTeamResult => createPayload(context, sessionId, {
  teamProjectId: data.teamProjectId ?? '',
  teacherHandle: data.teacherHandle ?? '',
  memberHandles: data.memberHandles ?? {},
  ...data
});

/**
 * Smart Genome Academy Team-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createGenomeAcademyTeamResultFromParams = (
  params: GenomeAcademyTeamParams,
  differences: Omit<GenomeAcademyTeamResult, 'context' | 'sessionId' | 'userId'>
): GenomeAcademyTeamResult => transformPayload(params, differences);

/**
 * Genome Academy Team — Type-safe command executor
 *
 * Usage:
 *   import { GenomeAcademyTeam } from '...shared/GenomeAcademyTeamTypes';
 *   const result = await GenomeAcademyTeam.execute({ ... });
 */
export const GenomeAcademyTeam = {
  execute(params: CommandInput<GenomeAcademyTeamParams>): Promise<GenomeAcademyTeamResult> {
    return Commands.execute<GenomeAcademyTeamParams, GenomeAcademyTeamResult>('genome/academy-team', params as Partial<GenomeAcademyTeamParams>);
  },
  commandName: 'genome/academy-team' as const,
} as const;
