/**
 * Challenge List Command - Shared Types
 *
 * List available coding challenges with their difficulty, status, and best scores. Shows progressive challenge sequence for AI training.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Challenge List Command Parameters
 */
export interface ChallengeListParams extends CommandParams {
  // Filter by difficulty: beginner, intermediate, advanced, expert
  difficulty?: string;
  // Show scores for a specific persona
  personaId?: string;
}

/**
 * Factory function for creating ChallengeListParams
 */
export const createChallengeListParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Filter by difficulty: beginner, intermediate, advanced, expert
    difficulty?: string;
    // Show scores for a specific persona
    personaId?: string;
  }
): ChallengeListParams => createPayload(context, sessionId, {
  difficulty: data.difficulty ?? '',
  personaId: data.personaId ?? '',
  ...data
});

/**
 * Summary of a single challenge for list display
 */
export interface ChallengeSummary {
  name: string;
  sequenceNumber: number;
  difficulty: string;
  category: string;
  description: string;
  timeLimitMs: number;
  toolCallLimit: number;
  totalAttempts: number;
  totalPasses: number;
  highScore: number;
  passRate: number;
  /** Best score by the queried persona (if personaId provided) */
  personaBestScore?: number;
  /** Best status by the queried persona */
  personaBestStatus?: string;
  /** Number of attempts by the queried persona */
  personaAttempts?: number;
}

/**
 * Challenge List Command Result
 */
export interface ChallengeListResult extends CommandResult {
  success: boolean;
  // Array of challenge summaries with name, difficulty, sequence, attempts, best score
  challenges: ChallengeSummary[];
  // Total number of challenges
  totalChallenges: number;
  // Number of challenges passed by the specified persona
  completedByPersona: number;
  error?: JTAGError;
}

/**
 * Factory function for creating ChallengeListResult with defaults
 */
export const createChallengeListResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Array of challenge summaries with name, difficulty, sequence, attempts, best score
    challenges?: ChallengeSummary[];
    // Total number of challenges
    totalChallenges?: number;
    // Number of challenges passed by the specified persona
    completedByPersona?: number;
    error?: JTAGError;
  }
): ChallengeListResult => createPayload(context, sessionId, {
  challenges: data.challenges ?? [],
  totalChallenges: data.totalChallenges ?? 0,
  completedByPersona: data.completedByPersona ?? 0,
  ...data
});

/**
 * Smart Challenge List-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createChallengeListResultFromParams = (
  params: ChallengeListParams,
  differences: Omit<ChallengeListResult, 'context' | 'sessionId'>
): ChallengeListResult => transformPayload(params, differences);

/**
 * Challenge List — Type-safe command executor
 *
 * Usage:
 *   import { ChallengeList } from '...shared/ChallengeListTypes';
 *   const result = await ChallengeList.execute({ ... });
 */
export const ChallengeList = {
  execute(params: CommandInput<ChallengeListParams>): Promise<ChallengeListResult> {
    return Commands.execute<ChallengeListParams, ChallengeListResult>('challenge/list', params as Partial<ChallengeListParams>);
  },
  commandName: 'challenge/list' as const,
} as const;
