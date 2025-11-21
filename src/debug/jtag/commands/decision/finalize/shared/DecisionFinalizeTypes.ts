/**
 * decision/finalize - Types
 * Manually finalize a voting proposal and calculate winner
 */

import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';

export interface DecisionFinalizeParams extends CommandParams {
  proposalId: UUID;
}

export interface DecisionFinalizeResult extends CommandResult {
  success: boolean;
  proposalStatus?: 'complete' | 'expired';
  winner?: {
    optionId: string;
    label: string;
    wins: number; // Number of pairwise wins in Condorcet method
  };
  voteCount?: number;
  error?: string;
}
