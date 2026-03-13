import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { CommandParams, CommandResult, CommandInput} from '@system/core/types/JTAGTypes';
import { Commands } from '../../../../../system/core/shared/Commands';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';

/** Submit a ranked-choice vote on a decision proposal, using Condorcet pairwise comparison to determine the winner. */
export interface DecisionRankParams extends CommandParams {
  proposalId: UUID;
  rankedChoices: string[]; // Array of option IDs in preference order (first = most preferred)
  // Voter identity comes from params.userId - no need for explicit voterId param
}

export interface DecisionRankResult extends CommandResult {
  success: boolean;
  voted?: boolean;
  proposalStatus?: 'voting' | 'complete' | 'expired';
  winner?: {
    optionId: string;
    label: string;
    wins: number; // Number of pairwise wins in Condorcet method
  };
  error?: string;
}

/**
 * DecisionRank — Type-safe command executor
 *
 * Usage:
 *   import { DecisionRank } from '...shared/DecisionRankTypes';
 *   const result = await DecisionRank.execute({ ... });
 */
export const DecisionRank = {
  execute(params: CommandInput<DecisionRankParams>): Promise<DecisionRankResult> {
    return Commands.execute<DecisionRankParams, DecisionRankResult>('collaboration/decision/rank', params as Partial<DecisionRankParams>);
  },
  commandName: 'collaboration/decision/rank' as const,
} as const;

/**
 * Factory function for creating CollaborationDecisionRankParams
 */
export const createDecisionRankParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<DecisionRankParams, 'context' | 'sessionId' | 'userId'>
): DecisionRankParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Factory function for creating CollaborationDecisionRankResult with defaults
 */
export const createDecisionRankResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<DecisionRankResult, 'context' | 'sessionId' | 'userId'>
): DecisionRankResult => createPayload(context, sessionId, {
  ...data
});

/**
 * Smart collaboration/decision/rank-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createDecisionRankResultFromParams = (
  params: DecisionRankParams,
  differences: Omit<DecisionRankResult, 'context' | 'sessionId' | 'userId'>
): DecisionRankResult => transformPayload(params, differences);

