/**
 * Decision Finalize Command - Shared Types
 *
 * Close voting and calculate winner using ranked-choice voting
 */

import type { CommandParams, CommandResult, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { RoundResult } from '@system/data/entities/DecisionEntity';

/**
 * Decision Finalize Command Parameters
 */
export interface DecisionFinalizeParams extends CommandParams {
  // Unique identifier for the proposal to finalize
  proposalId: string;
}

/**
 * Factory function for creating DecisionFinalizeParams
 */
export const createDecisionFinalizeParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Unique identifier for the proposal to finalize
    proposalId: string;
  }
): DecisionFinalizeParams => createPayload(context, sessionId, {

  ...data
});

/**
 * Decision Finalize Command Result
 */
export interface DecisionFinalizeResult extends CommandResult {
  // Whether finalization succeeded
  success: boolean;
  // The winning option ID (or null if no winner)
  winner: string | null;
  // Elimination rounds from ranked-choice voting
  rounds: RoundResult[];
  // Voter turnout statistics
  participation: {
    totalEligible: number;
    totalVoted: number;
    percentage: number;
  };
  // Final vote counts for each option
  finalTallies: Record<string, number>;
  error?: JTAGError;
}

/**
 * Factory function for creating DecisionFinalizeResult with defaults
 */
export const createDecisionFinalizeResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // The winning option ID (or null if no winner)
    winner?: string | null;
    // Elimination rounds from ranked-choice voting
    rounds?: RoundResult[];
    // Voter turnout statistics
    participation?: {
      totalEligible: number;
      totalVoted: number;
      percentage: number;
    };
    // Final vote counts for each option
    finalTallies?: Record<string, number>;
    error?: JTAGError;
  }
): DecisionFinalizeResult => createPayload(context, sessionId, {
  ...data,
  success: data.success ?? false,
  winner: data.winner ?? null,
  rounds: data.rounds ?? [],
  participation: data.participation ?? { totalEligible: 0, totalVoted: 0, percentage: 0 },
  finalTallies: data.finalTallies ?? {}
});

/**
 * Smart Decision Finalize-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createDecisionFinalizeResultFromParams = (
  params: DecisionFinalizeParams,
  differences: Omit<DecisionFinalizeResult, 'context' | 'sessionId'>
): DecisionFinalizeResult => transformPayload(params, differences);
