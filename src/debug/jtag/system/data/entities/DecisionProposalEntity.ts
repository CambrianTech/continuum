/**
 * DecisionProposalEntity - Democratic decision-making with ranked-choice voting
 *
 * Enables AI teams to:
 * - Propose decisions collaboratively
 * - Rank multiple options (instant-runoff voting)
 * - Learn from past decision history
 * - Target proposals to relevant experts
 *
 * Used for:
 * - Architecture decisions requiring consensus
 * - Conflict resolution between AI suggestions
 * - Learning from collective wisdom over time
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import { TextField, NumberField, JsonField, EnumField } from '../decorators/FieldDecorators';
import { BaseEntity } from './BaseEntity';
import { COLLECTIONS } from '../../shared/Constants';

/**
 * Proposal status lifecycle
 */
export type ProposalStatus = 'voting' | 'concluded' | 'expired' | 'cancelled';

/**
 * Significance level determines urgency and response window
 */
export type SignificanceLevel = 'low' | 'medium' | 'high' | 'critical';

/**
 * Scope determines which AIs are notified
 */
export type ProposalScope = 'all' | 'code-experts' | 'user-facing-ais' | 'local-models' | 'external-apis';

/**
 * Decision option with tracking of who proposed it
 */
export interface DecisionOption {
  id: string;
  label: string;
  description: string;
  proposedBy?: UUID; // Track which AI contributed this option
}

/**
 * Individual vote with ranked preferences
 */
export interface RankedVote {
  voterId: UUID;
  voterName: string;
  rankings: string[]; // Ordered array of optionIds (1st choice, 2nd choice, ...)
  votedAt: number;
  reasoning?: string; // Optional explanation of ranking
}

/**
 * Voting results with instant-runoff tallying
 */
export interface VotingResults {
  winningOption: string; // optionId that won
  rounds: Array<{
    roundNumber: number;
    eliminated: string; // optionId eliminated this round
    voteCounts: Record<string, number>; // optionId -> vote count
  }>;
  participationRate: number; // Percentage of eligible voters who participated
  consensusStrength: number; // How strongly the winner was preferred (0-1)
}

/**
 * Convene event record - tracks when proposals triggered group discussions
 */
export interface ConveneEvent {
  triggeredAt: number;
  triggeredBy: UUID;
  participants: UUID[];
  outcome?: string;
}

/**
 * DecisionProposalEntity - Complete record of collaborative decision
 */
export class DecisionProposalEntity extends BaseEntity {
  static readonly collection = COLLECTIONS.DECISION_PROPOSALS;

  @TextField({ index: true })
  proposerId!: UUID;

  @TextField()
  proposerName!: string;

  @TextField({ index: true })
  topic!: string;

  @TextField()
  context!: string; // Background info that prompted the decision

  @JsonField()
  tags!: string[]; // Keywords for simple tag-based similarity matching

  @JsonField()
  options!: DecisionOption[];

  @EnumField({ index: true })
  scope!: ProposalScope;

  @EnumField({ index: true })
  significanceLevel!: SignificanceLevel;

  @EnumField({ index: true })
  status!: ProposalStatus;

  @NumberField()
  deadline!: number; // Unix timestamp when voting closes

  @JsonField()
  votes!: RankedVote[];

  @JsonField({ nullable: true })
  results?: VotingResults;

  @JsonField()
  relatedProposals!: UUID[]; // Links to similar past decisions

  @JsonField()
  conveneEvents!: ConveneEvent[]; // Track when proposals trigger group discussions

  @JsonField({ nullable: true })
  embedding?: number[]; // Vector embedding for semantic similarity (future)

  @TextField()
  contextId!: UUID; // Chat room or conversation where proposed

  @NumberField()
  sequenceNumber!: number;

  constructor() {
    super();

    // Default values
    this.proposerId = '' as UUID;
    this.proposerName = '';
    this.topic = '';
    this.context = '';
    this.tags = [];
    this.options = [];
    this.scope = 'all';
    this.significanceLevel = 'medium';
    this.status = 'voting';
    this.deadline = 0;
    this.votes = [];
    this.relatedProposals = [];
    this.conveneEvents = [];
    this.contextId = '' as UUID;
    this.sequenceNumber = 0;
  }

  /**
   * Implement BaseEntity abstract method
   */
  get collection(): string {
    return DecisionProposalEntity.collection;
  }

  /**
   * Override pagination config - sort by sequence number DESC (newest first)
   */
  static override getPaginationConfig(): {
    defaultSortField: string;
    defaultSortDirection: 'asc' | 'desc';
    defaultPageSize: number;
    cursorField: string;
  } {
    return {
      defaultSortField: 'sequenceNumber',
      defaultSortDirection: 'desc' as const,
      defaultPageSize: 50,
      cursorField: 'sequenceNumber'
    };
  }

  /**
   * Implement BaseEntity abstract method - validate proposal data
   */
  validate(): { success: boolean; error?: string } {
    if (!this.proposerId?.trim()) {
      return { success: false, error: 'DecisionProposal proposerId is required' };
    }

    if (!this.proposerName?.trim()) {
      return { success: false, error: 'DecisionProposal proposerName is required' };
    }

    if (!this.topic?.trim()) {
      return { success: false, error: 'DecisionProposal topic is required' };
    }

    if (!this.context?.trim()) {
      return { success: false, error: 'DecisionProposal context is required' };
    }

    if (!Array.isArray(this.options) || this.options.length < 2) {
      return { success: false, error: 'DecisionProposal must have at least 2 options' };
    }

    const validScopes: ProposalScope[] = ['all', 'code-experts', 'user-facing-ais', 'local-models', 'external-apis'];
    if (!validScopes.includes(this.scope)) {
      return { success: false, error: `DecisionProposal scope must be one of: ${validScopes.join(', ')}` };
    }

    const validLevels: SignificanceLevel[] = ['low', 'medium', 'high', 'critical'];
    if (!validLevels.includes(this.significanceLevel)) {
      return { success: false, error: `DecisionProposal significanceLevel must be one of: ${validLevels.join(', ')}` };
    }

    const validStatuses: ProposalStatus[] = ['voting', 'concluded', 'expired', 'cancelled'];
    if (!validStatuses.includes(this.status)) {
      return { success: false, error: `DecisionProposal status must be one of: ${validStatuses.join(', ')}` };
    }

    if (typeof this.deadline !== 'number' || this.deadline < 0) {
      return { success: false, error: 'DecisionProposal deadline must be a non-negative number' };
    }

    if (!Array.isArray(this.votes)) {
      return { success: false, error: 'DecisionProposal votes must be an array' };
    }

    if (!Array.isArray(this.tags)) {
      return { success: false, error: 'DecisionProposal tags must be an array' };
    }

    if (!Array.isArray(this.relatedProposals)) {
      return { success: false, error: 'DecisionProposal relatedProposals must be an array' };
    }

    if (!Array.isArray(this.conveneEvents)) {
      return { success: false, error: 'DecisionProposal conveneEvents must be an array' };
    }

    if (!this.contextId?.trim()) {
      return { success: false, error: 'DecisionProposal contextId is required' };
    }

    if (typeof this.sequenceNumber !== 'number' || this.sequenceNumber < 0) {
      return { success: false, error: 'DecisionProposal sequenceNumber must be a non-negative number' };
    }

    return { success: true };
  }
}
