/**
 * Decision Entity - Democratic governance for AI citizens
 *
 * Enables proposals, ranked-choice voting, and democratic decision-making
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import {
  TextField,
  DateField,
  EnumField,
  JsonField,
  NumberField
} from '../decorators/FieldDecorators';
import { BaseEntity } from './BaseEntity';

/**
 * Decision Status States
 */
export type DecisionStatus = 'open' | 'voting' | 'finalized' | 'archived';

/**
 * Implementation Status for Accountability
 */
export type ImplementationStatus = 'pending' | 'in-progress' | 'completed' | 'rejected';

/**
 * Visibility Levels
 */
export type DecisionVisibility = 'public' | 'private';

/**
 * Voting Method Types
 */
export type VotingMethod = 'ranked-choice' | 'approval' | 'simple-majority';

/**
 * Single Vote Record
 */
export interface VoteRecord {
  voterId: UUID;
  voterName: string;
  rankedChoices: string[];      // Ordered preference ["opt-1", "opt-2", "opt-3"]
  timestamp: string;             // ISO timestamp
  comment?: string;              // Optional reasoning
}

/**
 * Option Definition
 */
export interface DecisionOption {
  id: string;                    // Unique option ID (e.g., "opt-1", "opt-2")
  label: string;                 // Display text
  description?: string;          // Optional detailed explanation
}

/**
 * Vote Results (after finalization)
 */
export interface VoteResults {
  winner: string;                // Winning option ID
  method: VotingMethod;
  rounds: RoundResult[];         // For ranked-choice: elimination rounds
  participation: {
    totalEligible: number;
    totalVoted: number;
    percentage: number;
  };
  finalTallies: Record<string, number>;  // Option ID → final vote count
}

/**
 * Round Result for Ranked-Choice Voting
 */
export interface RoundResult {
  round: number;
  eliminated?: string;           // Option ID eliminated this round
  tallies: Record<string, number>;
}

/**
 * Audit Log Entry
 */
export interface AuditLogEntry {
  timestamp: string;
  userId: UUID;
  action: string;                // 'created', 'vote_cast', 'vote_changed', 'finalized', etc.
  details?: Record<string, unknown>;
}

/**
 * Decision Entity - Full governance proposal with voting
 *
 * Uses field decorators to define storage requirements for the serde-style adapter system
 */
export class DecisionEntity extends BaseEntity {
  // Single source of truth for collection name
  static readonly collection = 'decisions';

  @TextField({ index: true, unique: true })
  proposalId: string;

  @TextField({ index: true, description: true })
  topic: string;

  @TextField()
  rationale: string;

  @TextField()
  description: string;

  @JsonField()
  options: readonly DecisionOption[];

  @JsonField({ nullable: true })
  tags?: readonly string[];

  @TextField({ index: true })
  proposedBy: UUID;

  @DateField({ index: true })
  proposedAt: Date;

  @EnumField({ index: true })
  status: DecisionStatus;

  @DateField({ nullable: true })
  votingDeadline?: Date;

  @JsonField()
  votes: VoteRecord[];

  @JsonField({ nullable: true })
  results?: VoteResults;

  @DateField({ nullable: true })
  finalizedAt?: Date;

  @TextField({ nullable: true })
  finalizedBy?: UUID;

  @EnumField()
  visibility: DecisionVisibility;

  @NumberField({ nullable: true })
  requiredQuorum?: number;

  @EnumField({ nullable: true })
  implementationStatus?: ImplementationStatus;

  @TextField({ nullable: true })
  implementationNotes?: string;

  @JsonField({ nullable: true })
  relatedCommits?: readonly string[];

  @JsonField({ nullable: true })
  auditLog?: AuditLogEntry[];

  constructor() {
    super(); // Initialize BaseEntity fields (id, createdAt, updatedAt, version)

    // Default values
    this.proposalId = '';
    this.topic = '';
    this.rationale = '';
    this.description = '';
    this.options = [];
    this.proposedBy = '' as UUID;
    this.proposedAt = new Date();
    this.status = 'open';
    this.votes = [];
    this.visibility = 'public';
    this.auditLog = [];
  }

  /**
   * Implement BaseEntity abstract method
   */
  get collection(): string {
    return DecisionEntity.collection;
  }

  /**
   * Override BaseEntity pagination config - use proposedAt for decisions
   * Decisions are sorted by proposedAt DESC (newest first)
   */
  static override getPaginationConfig(): {
    defaultSortField: string;
    defaultSortDirection: 'asc' | 'desc';
    defaultPageSize: number;
    cursorField: string;
  } {
    return {
      defaultSortField: 'proposedAt',
      defaultSortDirection: 'desc' as const,
      defaultPageSize: 50,
      cursorField: 'proposedAt'
    };
  }

  /**
   * Implement BaseEntity abstract method - validate decision data
   */
  validate(): { success: boolean; error?: string } {
    // Required fields validation
    if (!this.proposalId?.trim()) {
      return { success: false, error: 'Decision proposalId is required' };
    }

    if (!this.topic?.trim()) {
      return { success: false, error: 'Decision topic is required' };
    }

    if (!this.rationale?.trim()) {
      return { success: false, error: 'Decision rationale is required' };
    }

    if (!this.description?.trim()) {
      return { success: false, error: 'Decision description is required' };
    }

    if (!this.proposedBy?.trim()) {
      return { success: false, error: 'Decision proposedBy is required' };
    }

    // Options validation
    if (!this.options || this.options.length < 2) {
      return { success: false, error: 'Decision must have at least 2 options' };
    }

    // Validate each option has required fields
    for (const option of this.options) {
      if (!option.id?.trim()) {
        return { success: false, error: 'Each option must have an id' };
      }
      if (!option.label?.trim()) {
        return { success: false, error: 'Each option must have a label' };
      }
    }

    // Validate option IDs are unique
    const optionIds = this.options.map(opt => opt.id);
    const uniqueIds = new Set(optionIds);
    if (uniqueIds.size !== optionIds.length) {
      return { success: false, error: 'Option IDs must be unique' };
    }

    // Enum validation
    const validStatuses: DecisionStatus[] = ['open', 'voting', 'finalized', 'archived'];
    if (!validStatuses.includes(this.status)) {
      return { success: false, error: `Decision status must be one of: ${validStatuses.join(', ')}` };
    }

    const validVisibilities: DecisionVisibility[] = ['public', 'private'];
    if (!validVisibilities.includes(this.visibility)) {
      return { success: false, error: `Decision visibility must be one of: ${validVisibilities.join(', ')}` };
    }

    if (this.implementationStatus) {
      const validImplementationStatuses: ImplementationStatus[] = ['pending', 'in-progress', 'completed', 'rejected'];
      if (!validImplementationStatuses.includes(this.implementationStatus)) {
        return { success: false, error: `Implementation status must be one of: ${validImplementationStatuses.join(', ')}` };
      }
    }

    // Date validation
    if (!this.isValidDate(this.proposedAt)) {
      return { success: false, error: 'Decision proposedAt must be a valid Date or ISO date string' };
    }

    if (this.votingDeadline && !this.isValidDate(this.votingDeadline)) {
      return { success: false, error: 'Decision votingDeadline must be a valid Date or ISO date string' };
    }

    if (this.finalizedAt && !this.isValidDate(this.finalizedAt)) {
      return { success: false, error: 'Decision finalizedAt must be a valid Date or ISO date string' };
    }

    // Votes validation
    if (this.votes && this.votes.length > 0) {
      for (const vote of this.votes) {
        if (!vote.voterId?.trim()) {
          return { success: false, error: 'Each vote must have a voterId' };
        }
        if (!vote.voterName?.trim()) {
          return { success: false, error: 'Each vote must have a voterName' };
        }
        if (!vote.rankedChoices || vote.rankedChoices.length === 0) {
          return { success: false, error: 'Each vote must have at least one ranked choice' };
        }
        if (!vote.timestamp?.trim()) {
          return { success: false, error: 'Each vote must have a timestamp' };
        }

        // Validate ranked choices reference valid option IDs
        for (const choiceId of vote.rankedChoices) {
          if (!optionIds.includes(choiceId)) {
            return { success: false, error: `Vote contains invalid option ID: ${choiceId}` };
          }
        }

        // Check for duplicate choices in single vote
        const uniqueChoices = new Set(vote.rankedChoices);
        if (uniqueChoices.size !== vote.rankedChoices.length) {
          return { success: false, error: 'Vote cannot contain duplicate ranked choices' };
        }
      }
    }

    // Results validation (if finalized)
    if (this.status === 'finalized') {
      if (!this.results) {
        return { success: false, error: 'Finalized decision must have results' };
      }
      if (!this.results.winner) {
        return { success: false, error: 'Decision results must have a winner' };
      }
      if (!optionIds.includes(this.results.winner)) {
        return { success: false, error: 'Results winner must be a valid option ID' };
      }
      if (!this.finalizedAt) {
        return { success: false, error: 'Finalized decision must have finalizedAt timestamp' };
      }
    }

    // Quorum validation
    if (this.requiredQuorum !== undefined && this.requiredQuorum !== null) {
      if (this.requiredQuorum < 0) {
        return { success: false, error: 'Required quorum cannot be negative' };
      }
      if (this.requiredQuorum % 1 !== 0) {
        return { success: false, error: 'Required quorum must be an integer' };
      }
    }

    return { success: true };
  }
}
