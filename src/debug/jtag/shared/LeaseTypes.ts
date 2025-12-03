import type { BaseEntity } from '../system/data/entities/BaseEntity';
import type { UUID } from '../system/core/types/CrossPlatformUUID';

/**
 * Protection levels for file access control
 */
export enum ProtectionLevel {
  UNRESTRICTED = 0,    // No approval needed
  PEER_REVIEW = 1,     // 2 AI approvals required
  SENIOR_REVIEW = 2,   // 3 AI approvals + must include senior AI
  HUMAN_REVIEW = 3,    // Human must approve
  LOCKED = 4           // Nobody can edit
}

/**
 * File type for lease validation
 */
export enum FileType {
  MARKDOWN = 'markdown',
  JSON = 'json',
  YAML = 'yaml',
  TYPESCRIPT = 'typescript',
  JAVASCRIPT = 'javascript',
  OTHER = 'other'
}

/**
 * Lease status
 */
export enum LeaseStatus {
  ACTIVE = 'active',
  COMMITTED = 'committed',
  ROLLED_BACK = 'rolled_back',
  EXPIRED = 'expired',
  BROKEN = 'broken',
  KICKED = 'kicked'
}

/**
 * Edit operation types
 */
export enum EditOperation {
  REPLACE = 'replace',
  APPEND = 'append',
  INSERT = 'insert',
  DELETE = 'delete'
}

/**
 * Staged edit operation
 */
export interface StagedEdit {
  editNumber: number;
  timestamp: Date;
  operation: EditOperation;
  oldString?: string;
  newString?: string;
  section?: string;       // For docs
  afterLine?: number;     // For line-based insert
}

/**
 * File lease entity - time-limited exclusive write access
 */
export interface FileLease extends BaseEntity {
  id: UUID;
  entityType: 'file_lease';

  // File info
  filePath: string;
  fileType: FileType;
  originalHash: string;  // SHA256 of file when lease acquired
  protectionLevel: ProtectionLevel;

  // Owner info
  holderId: UUID;
  holderName: string;
  holderType: 'persona' | 'human';
  intent: string;  // Why they're editing

  // Timing
  acquiredAt: Date;
  expiresAt: Date;
  lastActivityAt: Date;
  durationSeconds: number;

  // State
  status: LeaseStatus;

  // Edits (staged changes)
  stagedEdits: StagedEdit[];
  editCount: number;
  extensionCount: number;

  // Validation (Phase 2+)
  validationRequired: boolean;
  validationPassed?: boolean;
  validationErrors?: string[];

  // Build verification (Phase 3)
  buildRequired: boolean;
  buildPassed?: boolean;
  buildErrors?: string[];
  testsRequired: boolean;
  testsPassed?: boolean;
  testFailures?: string[];
}

/**
 * Lease queue entry
 */
export interface LeaseQueue extends BaseEntity {
  id: UUID;
  entityType: 'lease_queue';

  filePath: string;
  requesterId: UUID;
  requesterName: string;
  requesterType: 'persona' | 'human';
  intent: string;
  requestedAt: Date;
  position: number;
  notified: boolean;
}

/**
 * Approval request for protected files
 */
export interface ApprovalRequest extends BaseEntity {
  id: UUID;
  entityType: 'approval_request';

  // File info
  filePath: string;
  protectionLevel: ProtectionLevel;

  // Requester
  requesterId: UUID;
  requesterName: string;
  requesterType: 'persona' | 'human';
  intent: string;

  // Requirements
  requiredApprovals: number;
  requiresSeniorApproval: boolean;

  // Status
  status: 'pending' | 'approved' | 'rejected' | 'expired';
  approvals: ApprovalVote[];

  // Timing
  requestedAt: Date;
  expiresAt: Date;
  resolvedAt?: Date;

  // Result
  leaseGranted: boolean;
  leaseId?: UUID;
}

/**
 * Approval vote
 */
export interface ApprovalVote {
  reviewerId: UUID;
  reviewerName: string;
  isSenior: boolean;
  action: 'approve' | 'reject';
  reason: string;
  timestamp: Date;
}

/**
 * Release request for emergency access
 */
export interface ReleaseRequest extends BaseEntity {
  id: UUID;
  entityType: 'release_request';

  // File & lease info
  filePath: string;
  currentLeaseId: UUID;
  currentHolderId: UUID;
  currentHolderName: string;

  // Requester
  requesterId: UUID;
  requesterName: string;
  requesterType: 'persona' | 'human';
  reason: string;
  priority: 'normal' | 'high' | 'urgent';

  // Status
  status: 'pending' | 'approved' | 'denied' | 'transferred' | 'expired';
  response?: ReleaseResponse;

  // Timing
  requestedAt: Date;
  expiresAt: Date;
  resolvedAt?: Date;

  // Result
  newLeaseId?: UUID;
}

/**
 * Release response
 */
export interface ReleaseResponse {
  action: 'approve' | 'deny' | 'transfer';
  message: string;
  timestamp: Date;
}

/**
 * Kick vote for democratic lease revocation
 */
export interface KickVote extends BaseEntity {
  id: UUID;
  entityType: 'kick_vote';

  // Target lease
  leaseId: UUID;
  filePath: string;
  currentHolderId: UUID;
  currentHolderName: string;
  leaseAgeMinutes: number;
  leaseInactivityMinutes: number;

  // Initiator
  initiatorId: UUID;
  initiatorName: string;
  initiatorType: 'persona' | 'human';
  reason: string;

  // Voting
  requiredVotes: number;
  votes: Vote[];
  supportCount: number;
  opposeCount: number;

  // Status
  status: 'pending' | 'passed' | 'failed' | 'expired' | 'appealed';

  // Timing
  initiatedAt: Date;
  expiresAt: Date;
  resolvedAt?: Date;

  // Result
  kicked: boolean;
  stagedEditsCommitted?: boolean;
  commitSuccess?: boolean;
  nextHolderId?: UUID;
  nextHolderName?: string;

  // Appeal
  appealId?: UUID;
}

/**
 * Vote on kick or appeal
 */
export interface Vote {
  voterId: UUID;
  voterName: string;
  action: 'support' | 'oppose';
  reason: string;
  timestamp: Date;
}

/**
 * Appeal against kick vote
 */
export interface KickAppeal extends BaseEntity {
  id: UUID;
  entityType: 'kick_appeal';

  // Source kick vote
  kickVoteId: UUID;
  leaseId: UUID;
  filePath: string;

  // Appellant (lease holder)
  appellantId: UUID;
  appellantName: string;
  reason: string;
  evidence: string;  // Why kick was unjustified

  // Voting
  requiredVotes: number;  // Same as kick threshold
  votes: Vote[];
  supportCount: number;  // Support = dismiss kick
  opposeCount: number;   // Oppose = uphold kick

  // Status
  status: 'pending' | 'upheld' | 'dismissed' | 'expired';

  // Timing
  submittedAt: Date;
  expiresAt: Date;
  resolvedAt?: Date;

  // Result
  kickDismissed: boolean;
  leaseRestored: boolean;
}

/**
 * Configuration for protection levels
 */
export interface FileProtectionConfig {
  pattern: string;
  level: ProtectionLevel;
  description: string;
}

/**
 * Senior AI configuration
 */
export interface SeniorAIConfig {
  userId: UUID;
  name: string;
  reason: string;  // Why they're senior
}
