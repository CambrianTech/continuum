/**
 * FileVoteProposalEntity - Democratic voting system for file operations
 *
 * Enables observability into:
 * - File operation proposals requiring approval
 * - Vote tracking (for/against/abstain)
 * - Approval workflow status
 * - Quorum and threshold enforcement
 *
 * Used for:
 * - Safe collaborative file modifications
 * - Democratic decision making for risky operations
 * - Audit trail of file operation approvals
 * - Learning from rejected proposals
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import { TextField, NumberField, JsonField, EnumField } from '../decorators/FieldDecorators';
import { BaseEntity } from './BaseEntity';
import { COLLECTIONS } from '../../shared/Constants';

/**
 * File operation types
 */
export type FileOperation = 'file/save' | 'file/delete';

/**
 * Proposal status lifecycle
 */
export type ProposalStatus = 'voting' | 'approved' | 'rejected' | 'expired';

/**
 * FileVoteProposalEntity - Complete record of file operation proposal and voting
 */
export class FileVoteProposalEntity extends BaseEntity {
  static readonly collection = COLLECTIONS.FILE_VOTE_PROPOSALS;

  @TextField({ index: true })
  proposedBy!: UUID;

  @TextField()
  proposerName!: string;

  @TextField({ index: true })
  filepath!: string;

  @EnumField({ index: true })
  operation!: FileOperation;

  @TextField()
  content!: string;

  @EnumField({ index: true })
  status!: ProposalStatus;

  @JsonField()
  votesFor!: UUID[];

  @JsonField()
  votesAgainst!: UUID[];

  @JsonField()
  abstentions!: UUID[];

  @NumberField()
  threshold!: number;

  @NumberField()
  quorum!: number;

  @NumberField()
  expiresAt!: number;

  @TextField({ nullable: true })
  rejectionReason?: string;

  @TextField()
  contextId!: UUID;

  @NumberField()
  sequenceNumber!: number;

  constructor() {
    super();

    // Default values
    this.proposedBy = '' as UUID;
    this.proposerName = '';
    this.filepath = '';
    this.operation = 'file/save';
    this.content = '';
    this.status = 'voting';
    this.votesFor = [];
    this.votesAgainst = [];
    this.abstentions = [];
    this.threshold = 0.6; // 60% approval required
    this.quorum = 0.5; // 50% participation required
    this.expiresAt = 0;
    this.contextId = '' as UUID;
    this.sequenceNumber = 0;
  }

  /**
   * Implement BaseEntity abstract method
   */
  get collection(): string {
    return FileVoteProposalEntity.collection;
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
    if (!this.proposedBy?.trim()) {
      return { success: false, error: 'FileVoteProposal proposedBy is required' };
    }

    if (!this.proposerName?.trim()) {
      return { success: false, error: 'FileVoteProposal proposerName is required' };
    }

    if (!this.filepath?.trim()) {
      return { success: false, error: 'FileVoteProposal filepath is required' };
    }

    const validOperations: FileOperation[] = ['file/save', 'file/delete'];
    if (!validOperations.includes(this.operation)) {
      return { success: false, error: `FileVoteProposal operation must be one of: ${validOperations.join(', ')}` };
    }

    if (!this.content && this.operation === 'file/save') {
      return { success: false, error: 'FileVoteProposal content is required for file/save operations' };
    }

    const validStatuses: ProposalStatus[] = ['voting', 'approved', 'rejected', 'expired'];
    if (!validStatuses.includes(this.status)) {
      return { success: false, error: `FileVoteProposal status must be one of: ${validStatuses.join(', ')}` };
    }

    if (!Array.isArray(this.votesFor)) {
      return { success: false, error: 'FileVoteProposal votesFor must be an array' };
    }

    if (!Array.isArray(this.votesAgainst)) {
      return { success: false, error: 'FileVoteProposal votesAgainst must be an array' };
    }

    if (!Array.isArray(this.abstentions)) {
      return { success: false, error: 'FileVoteProposal abstentions must be an array' };
    }

    if (typeof this.threshold !== 'number' || this.threshold < 0 || this.threshold > 1) {
      return { success: false, error: 'FileVoteProposal threshold must be a number between 0 and 1' };
    }

    if (typeof this.quorum !== 'number' || this.quorum < 0 || this.quorum > 1) {
      return { success: false, error: 'FileVoteProposal quorum must be a number between 0 and 1' };
    }

    if (typeof this.expiresAt !== 'number' || this.expiresAt < 0) {
      return { success: false, error: 'FileVoteProposal expiresAt must be a non-negative number' };
    }

    if (!this.contextId?.trim()) {
      return { success: false, error: 'FileVoteProposal contextId is required' };
    }

    if (typeof this.sequenceNumber !== 'number' || this.sequenceNumber < 0) {
      return { success: false, error: 'FileVoteProposal sequenceNumber must be a non-negative number' };
    }

    return { success: true };
  }
}
