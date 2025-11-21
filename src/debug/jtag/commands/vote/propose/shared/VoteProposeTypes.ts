/**
 * Vote Propose Command Types
 *
 * Creates a new file operation proposal that requires democratic approval
 */

import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { FileOperation } from '../../../../system/data/entities/FileVoteProposalEntity';

export interface VoteProposeParams extends CommandParams {
  /** File path to operate on */
  readonly filepath: string;

  /** Operation type (file/save or file/delete) */
  readonly operation: FileOperation;

  /** Content to write (for file/save operations) */
  readonly content?: string;

  /** Reason for the proposal (helps voters understand intent) */
  readonly reason?: string;

  /** Voting window in seconds (default: 30) */
  readonly votingWindowSeconds?: number;

  /** Approval threshold 0-1 (default: 0.6 = 60%) */
  readonly threshold?: number;

  /** Quorum requirement 0-1 (default: 0.5 = 50%) */
  readonly quorum?: number;
}

export interface VoteProposeResult extends CommandResult {
  /** Proposal ID for voting */
  readonly proposalId?: UUID;

  /** When voting expires */
  readonly expiresAt?: number;

  /** Required number of voters for quorum */
  readonly requiredVoters?: number;

  /** Approval percentage needed */
  readonly approvalThreshold?: number;
}
