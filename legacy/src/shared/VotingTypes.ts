/**
 * Universal Voting System Types
 *
 * ANY command in the system can require voting/approval.
 * This enables democratic decision-making for:
 * - File editing (lease kicks, protected file access)
 * - Architecture decisions
 * - Code changes
 * - Resource allocation
 * - Policy changes
 * - Any collaborative decision
 *
 * Commands can dynamically decide if voting is required based on:
 * - Protection level of target resource
 * - Caller's permissions
 * - Impact scope
 * - Community policies
 */

import type { UUID } from '../system/core/types/CrossPlatformUUID';
import type { BaseEntity } from '../system/data/entities/BaseEntity';

/**
 * Vote action - support or oppose
 */
export type VoteAction = 'support' | 'oppose';

/**
 * Vote status - lifecycle of a vote
 */
export type VoteStatus =
  | 'pending'      // Waiting for votes
  | 'passed'       // Reached required support
  | 'rejected'     // Reached opposition threshold or expired
  | 'expired';     // Timeout reached

/**
 * Voting strategy - how votes are counted
 */
export type VotingStrategy =
  | 'simple-majority'      // >50% support
  | 'supermajority'        // >=66% support
  | 'unanimous'            // 100% support
  | 'quorum-majority'      // Requires minimum voters + majority
  | 'weighted'             // Votes have different weights
  | 'ranked-choice';       // Multiple options ranked

/**
 * Individual vote record
 */
export interface Vote {
  voterId: UUID;
  voterName: string;
  voterType?: 'persona' | 'human';
  action: VoteAction;
  reason: string;
  weight?: number;           // For weighted voting
  timestamp: Date;
}

/**
 * Base interface for any votable proposal
 * Commands requiring voting create proposal entities implementing this
 */
export interface VotableProposal extends BaseEntity {
  /** Type of proposal (command-specific) */
  proposalType: string;

  /** Current status */
  status: VoteStatus;

  /** Voting strategy being used */
  strategy: VotingStrategy;

  /** Who initiated the proposal */
  initiatorId: UUID;
  initiatorName: string;
  initiatorType: 'persona' | 'human';

  /** What is being proposed */
  title: string;
  description: string;

  /** Votes cast so far */
  votes: Vote[];

  /** Vote counts */
  supportCount: number;
  opposeCount: number;

  /** Voting requirements */
  requiredVotes?: number;        // Minimum votes needed
  requiredSupportRatio?: number; // Required support ratio (0.0-1.0)
  minimumQuorum?: number;        // Minimum voters for quorum

  /** Timing */
  initiatedAt: Date;
  expiresAt: Date;
  resolvedAt?: Date;

  /** Outcome */
  passed?: boolean;
  outcomeMessage?: string;

  /** Command execution (if passed) */
  commandExecuted?: boolean;
  commandResult?: unknown;
}

/**
 * Result of voting operation
 */
export interface VoteResult {
  success: boolean;
  proposalId?: UUID;
  status?: VoteStatus;
  passed?: boolean;
  supportCount?: number;
  opposeCount?: number;
  message?: string;
  error?: string;
}

/**
 * Parameters for casting a vote
 */
export interface CastVoteParams {
  proposalId: UUID;
  voterId: UUID;
  voterName: string;
  voterType?: 'persona' | 'human';
  action: VoteAction;
  reason: string;
  weight?: number;
}

/**
 * Parameters for creating a votable proposal
 */
export interface CreateProposalParams {
  proposalType: string;
  title: string;
  description: string;
  initiatorId: UUID;
  initiatorName: string;
  initiatorType: 'persona' | 'human';
  strategy: VotingStrategy;
  requiredVotes?: number;
  requiredSupportRatio?: number;
  minimumQuorum?: number;
  durationSeconds?: number;      // How long voting is open (default: 900 = 15 min)
  metadata?: Record<string, unknown>; // Command-specific data
}

/**
 * Interface for commands that support voting
 * Commands can implement this to enable voting on their execution
 */
export interface IVotableCommand {
  /**
   * Check if this command execution requires voting
   * Commands decide based on context (protection level, permissions, etc.)
   */
  requiresVoting(params: unknown): Promise<boolean>;

  /**
   * Create a proposal for voting on this command
   * Returns proposal ID that voters can reference
   */
  createProposal(params: unknown): Promise<{ proposalId: UUID; message: string }>;

  /**
   * Execute command after vote passes
   * Called automatically by voting system when proposal passes
   */
  executeAfterVote(proposalId: UUID): Promise<unknown>;
}

/**
 * Voting configuration - system-wide settings
 */
export interface VotingConfig {
  /** Default voting duration in seconds */
  defaultDurationSeconds: number;

  /** Default strategy for different contexts */
  defaultStrategy: VotingStrategy;

  /** Minimum votes required by default */
  defaultRequiredVotes: number;

  /** Enable auto-execution after vote passes */
  autoExecuteOnPass: boolean;

  /** Allow vote changes before expiration */
  allowVoteChanges: boolean;
}

/**
 * Default voting configuration
 */
export const DEFAULT_VOTING_CONFIG: VotingConfig = {
  defaultDurationSeconds: 900,        // 15 minutes
  defaultStrategy: 'simple-majority',
  defaultRequiredVotes: 3,
  autoExecuteOnPass: true,
  allowVoteChanges: true
};

/**
 * Permission elevation through voting
 *
 * ANY command can require voting based on:
 * - Caller's permission level (junior AI, senior AI, human)
 * - Target resource protection (file protection level, system resource)
 * - Action impact (read vs write, scope of change)
 *
 * This enables democratic permission elevation:
 * - Junior AI can propose editing protected files → team votes
 * - Any AI can propose architecture changes → senior review + vote
 * - Misbehaving AI can be demoted → team votes to reduce permissions
 *
 * Think of it as democratic `sudo` - temporary permission elevation
 * with oversight and accountability.
 */
export interface PermissionElevationProposal extends VotableProposal {
  proposalType: 'permission-elevation';

  /** What command needs elevation */
  commandName: string;
  commandParams: Record<string, unknown>;

  /** Who is requesting elevation */
  requesterId: UUID;
  requesterName: string;
  currentPermissionLevel: number;

  /** What is being accessed */
  targetResource: string;
  targetProtectionLevel: number;
  requiredPermissionLevel: number;

  /** Why elevation is needed */
  justification: string;

  /** If passed, grant temporary elevated permission */
  grantedPermissionLevel?: number;
  permissionExpiresAt?: Date;
}

/**
 * Permission demotion through voting
 *
 * Team can vote to reduce misbehaving AI's permissions:
 * - Too many failed operations
 * - Violating community guidelines
 * - Repeated mistakes
 * - Resource abuse
 *
 * Democratic accountability mechanism.
 */
export interface PermissionDemotionProposal extends VotableProposal {
  proposalType: 'permission-demotion';

  /** Who should be demoted */
  targetUserId: UUID;
  targetUserName: string;
  currentPermissionLevel: number;

  /** What level to demote to */
  proposedPermissionLevel: number;

  /** Evidence and reasoning */
  violations: string[];
  evidence: string;

  /** Duration (permanent or temporary) */
  demotionDurationSeconds?: number; // undefined = permanent

  /** If passed, reduce permissions */
  demoted?: boolean;
  restorationDate?: Date;
}
