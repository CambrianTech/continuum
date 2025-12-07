/**
 * Lease Daemon - Shared Interface
 *
 * Manages file lease operations for collaborative editing
 * Provides democratic governance through peer approval and voting
 */

import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import type {
  FileLease,
  LeaseQueue,
  ApprovalRequest,
  ReleaseRequest,
  KickVote,
  KickAppeal,
  ProtectionLevel,
  FileType
} from '../../../shared/LeaseTypes';

/**
 * Result of lease request operation
 */
export interface LeaseRequestResult {
  success: boolean;
  lease?: FileLease;
  queued?: boolean;
  queuePosition?: number;
  requiresApproval?: boolean;
  approvalId?: UUID;
  currentHolder?: string;
  expiresAt?: Date;
  error?: string;
}

/**
 * Result of lease commit operation
 */
export interface LeaseCommitResult {
  success: boolean;
  committed?: boolean;
  validationErrors?: string[];
  buildErrors?: string[];
  testFailures?: string[];
  error?: string;
}

/**
 * Result of approval request
 */
export interface ApprovalResult {
  success: boolean;
  approvalRequest?: ApprovalRequest;
  currentApprovals?: number;
  requiredApprovals?: number;
  approved?: boolean;
  rejected?: boolean;
  leaseGranted?: boolean;
  leaseId?: UUID;
  error?: string;
}

/**
 * Result of vote operation
 */
export interface VoteResult {
  success: boolean;
  voteRecorded?: boolean;
  currentSupport?: number;
  currentOppose?: number;
  requiredVotes?: number;
  passed?: boolean;
  failed?: boolean;
  error?: string;
}

/**
 * Lease Daemon Interface
 *
 * This daemon manages all file lease operations:
 * - Lease acquisition/release
 * - Queue management
 * - Approval workflows
 * - Democratic voting (kick/appeal)
 * - Expiration monitoring
 */
export interface ILeaseDaemon {
  /**
   * Request a lease on a file
   */
  requestLease(params: {
    filePath: string;
    requesterId: UUID;
    requesterName: string;
    requesterType: 'persona' | 'human';
    intent: string;
    durationSeconds?: number;
  }): Promise<LeaseRequestResult>;

  /**
   * Release a lease (commit or rollback)
   */
  releaseLease(params: {
    leaseId: UUID;
    action: 'commit' | 'rollback';
    message?: string;
  }): Promise<LeaseCommitResult>;

  /**
   * Extend an active lease
   */
  extendLease(params: {
    leaseId: UUID;
    additionalSeconds: number;
    reason: string;
  }): Promise<{ success: boolean; newExpiresAt?: Date; error?: string }>;

  /**
   * Break a lease (human override)
   */
  breakLease(params: {
    filePath: string;
    breakerId: UUID;
    reason: string;
  }): Promise<{ success: boolean; error?: string }>;

  /**
   * Request approval for protected file
   */
  requestApproval(params: {
    filePath: string;
    requesterId: UUID;
    requesterName: string;
    requesterType: 'persona' | 'human';
    intent: string;
  }): Promise<ApprovalResult>;

  /**
   * Review an approval request
   */
  reviewApproval(params: {
    approvalId: UUID;
    reviewerId: UUID;
    reviewerName: string;
    action: 'approve' | 'reject';
    reason: string;
  }): Promise<ApprovalResult>;

  /**
   * Request lease release (emergency access)
   */
  requestRelease(params: {
    filePath: string;
    requesterId: UUID;
    requesterName: string;
    requesterType: 'persona' | 'human';
    reason: string;
    priority: 'normal' | 'high' | 'urgent';
  }): Promise<{ success: boolean; releaseId?: UUID; error?: string }>;

  /**
   * Respond to release request (current holder)
   */
  respondRelease(params: {
    releaseId: UUID;
    action: 'approve' | 'deny' | 'transfer';
    message: string;
  }): Promise<{ success: boolean; leaseTransferred?: boolean; error?: string }>;

  /**
   * Initiate or vote on kick
   */
  voteKick(params: {
    leaseId?: UUID;  // If initiating
    kickId?: UUID;   // If voting
    voterId: UUID;
    voterName: string;
    voterType: 'persona' | 'human';
    action: 'support' | 'oppose';
    reason: string;
  }): Promise<VoteResult>;

  /**
   * Appeal a kick vote
   */
  appealKick(params: {
    kickId: UUID;
    appellantId: UUID;
    appellantName: string;
    reason: string;
    evidence: string;
  }): Promise<{ success: boolean; appealId?: UUID; error?: string }>;

  /**
   * Vote on kick appeal
   */
  voteAppeal(params: {
    appealId: UUID;
    voterId: UUID;
    voterName: string;
    action: 'support' | 'oppose';  // support = dismiss kick, oppose = uphold kick
    reason: string;
  }): Promise<VoteResult>;

  /**
   * Get active lease for file
   */
  getActiveLease(filePath: string): Promise<FileLease | null>;

  /**
   * Get lease by ID
   */
  getLease(leaseId: UUID): Promise<FileLease | null>;

  /**
   * Get queue for file
   */
  getQueue(filePath: string): Promise<LeaseQueue[]>;

  /**
   * Get protection level for file
   */
  getProtectionLevel(filePath: string): Promise<ProtectionLevel>;

  /**
   * Get file type
   */
  getFileType(filePath: string): Promise<FileType>;

  /**
   * Cleanup expired leases (background task)
   */
  cleanupExpired(): Promise<void>;
}
