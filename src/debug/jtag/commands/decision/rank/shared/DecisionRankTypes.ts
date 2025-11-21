/**
 * decision/rank - Types
 * Submit ranked-choice vote for a decision proposal
 */

import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';

export interface DecisionRankParams extends CommandParams {
  proposalId: UUID;
  rankedChoices: string[]; // Array of option IDs in preference order (first = most preferred)
  voterId?: UUID; // Optional - defaults to current user
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
