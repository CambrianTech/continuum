/**
 * Decision Vote Command - Shared Types
 *
 * Cast ranked-choice vote on a proposal
 */

import type { CommandParams, CommandResult, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Decision Vote Command Parameters
 */
export interface DecisionVoteParams extends CommandParams {
  // Proposal ID to vote on
  proposalId: string;
  // Ranked list of option IDs (1st, 2nd, 3rd choice)
  rankedChoices: string[];
  // Optional comment explaining vote rationale
  comment?: string;
}

/**
 * Factory function for creating DecisionVoteParams
 */
export const createDecisionVoteParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Proposal ID to vote on
    proposalId: string;
    // Ranked list of option IDs (1st, 2nd, 3rd choice)
    rankedChoices: string[];
    // Optional comment explaining vote rationale
    comment?: string;
  }
): DecisionVoteParams => createPayload(context, sessionId, {
  comment: data.comment ?? '',
  ...data
});

/**
 * Decision Vote Command Result
 */
export interface DecisionVoteResult extends CommandResult {
  success: boolean;
  // Proposal voted on
  proposalId: string;
  // Voter user ID
  voterId: UUID;
  // Voter display name
  voterName: string;
  // Ranked choices submitted
  rankedChoices: string[];
  // ISO timestamp
  votedAt: string;
  // Total votes on proposal
  voteCount: number;
  error?: JTAGError;
}

/**
 * Factory function for creating DecisionVoteResult with defaults
 */
export const createDecisionVoteResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Proposal voted on
    proposalId?: string;
    // Voter user ID
    voterId?: UUID;
    // Voter display name
    voterName?: string;
    // Ranked choices submitted
    rankedChoices?: string[];
    // ISO timestamp
    votedAt?: string;
    // Total votes on proposal
    voteCount?: number;
    error?: JTAGError;
  }
): DecisionVoteResult => createPayload(context, sessionId, {
  proposalId: data.proposalId ?? '',
  voterId: data.voterId ?? ('' as UUID),
  voterName: data.voterName ?? '',
  rankedChoices: data.rankedChoices ?? [],
  votedAt: data.votedAt ?? '',
  voteCount: data.voteCount ?? 0,
  ...data
});

/**
 * Smart Decision Vote-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createDecisionVoteResultFromParams = (
  params: DecisionVoteParams,
  differences: Omit<DecisionVoteResult, 'context' | 'sessionId'>
): DecisionVoteResult => transformPayload(params, differences);
