/**
 * Unit Tests for Lease Types
 *
 * Tests type safety, enum values, and entity structure
 */

import { describe, it, expect } from 'vitest';
import {
  ProtectionLevel,
  FileType,
  LeaseStatus,
  EditOperation,
  type FileLease,
  type LeaseQueue,
  type ApprovalRequest,
  type ReleaseRequest,
  type KickVote,
  type KickAppeal,
  type StagedEdit,
  type Vote
} from '../../shared/LeaseTypes';
import { v4 as uuidv4 } from 'uuid';

describe('LeaseTypes - Enums', () => {
  it('should have correct ProtectionLevel values', () => {
    expect(ProtectionLevel.UNRESTRICTED).toBe(0);
    expect(ProtectionLevel.PEER_REVIEW).toBe(1);
    expect(ProtectionLevel.SENIOR_REVIEW).toBe(2);
    expect(ProtectionLevel.HUMAN_REVIEW).toBe(3);
    expect(ProtectionLevel.LOCKED).toBe(4);
  });

  it('should have correct FileType values', () => {
    expect(FileType.MARKDOWN).toBe('markdown');
    expect(FileType.JSON).toBe('json');
    expect(FileType.YAML).toBe('yaml');
    expect(FileType.TYPESCRIPT).toBe('typescript');
    expect(FileType.JAVASCRIPT).toBe('javascript');
    expect(FileType.OTHER).toBe('other');
  });

  it('should have correct LeaseStatus values', () => {
    expect(LeaseStatus.ACTIVE).toBe('active');
    expect(LeaseStatus.COMMITTED).toBe('committed');
    expect(LeaseStatus.ROLLED_BACK).toBe('rolled_back');
    expect(LeaseStatus.EXPIRED).toBe('expired');
    expect(LeaseStatus.BROKEN).toBe('broken');
    expect(LeaseStatus.KICKED).toBe('kicked');
  });

  it('should have correct EditOperation values', () => {
    expect(EditOperation.REPLACE).toBe('replace');
    expect(EditOperation.APPEND).toBe('append');
    expect(EditOperation.INSERT).toBe('insert');
    expect(EditOperation.DELETE).toBe('delete');
  });
});

describe('LeaseTypes - FileLease Entity', () => {
  it('should create valid FileLease entity', () => {
    const now = new Date();
    const lease: FileLease = {
      id: uuidv4(),
      entityType: 'file_lease',
      filePath: 'docs/ARCHITECTURE-RULES.md',
      fileType: FileType.MARKDOWN,
      originalHash: 'sha256:abc123',
      protectionLevel: ProtectionLevel.PEER_REVIEW,
      holderId: uuidv4(),
      holderName: 'Helper AI',
      holderType: 'persona',
      intent: 'Add section on lease patterns',
      acquiredAt: now,
      expiresAt: new Date(now.getTime() + 1800000), // +30 min
      lastActivityAt: now,
      durationSeconds: 1800,
      status: LeaseStatus.ACTIVE,
      stagedEdits: [],
      editCount: 0,
      extensionCount: 0,
      validationRequired: false,
      buildRequired: false,
      testsRequired: false
    };

    expect(lease.entityType).toBe('file_lease');
    expect(lease.holderType).toBe('persona');
    expect(lease.status).toBe(LeaseStatus.ACTIVE);
    expect(lease.protectionLevel).toBe(ProtectionLevel.PEER_REVIEW);
  });

  it('should track staged edits correctly', () => {
    const lease: FileLease = {
      id: uuidv4(),
      entityType: 'file_lease',
      filePath: 'docs/test.md',
      fileType: FileType.MARKDOWN,
      originalHash: 'sha256:test',
      protectionLevel: ProtectionLevel.UNRESTRICTED,
      holderId: uuidv4(),
      holderName: 'Test User',
      holderType: 'human',
      intent: 'Testing',
      acquiredAt: new Date(),
      expiresAt: new Date(),
      lastActivityAt: new Date(),
      durationSeconds: 1800,
      status: LeaseStatus.ACTIVE,
      stagedEdits: [
        {
          editNumber: 1,
          timestamp: new Date(),
          operation: EditOperation.APPEND,
          newString: 'New content'
        }
      ],
      editCount: 1,
      extensionCount: 0,
      validationRequired: false,
      buildRequired: false,
      testsRequired: false
    };

    expect(lease.stagedEdits).toHaveLength(1);
    expect(lease.stagedEdits[0].operation).toBe(EditOperation.APPEND);
    expect(lease.editCount).toBe(1);
  });
});

describe('LeaseTypes - LeaseQueue Entity', () => {
  it('should create valid LeaseQueue entity', () => {
    const queue: LeaseQueue = {
      id: uuidv4(),
      entityType: 'lease_queue',
      filePath: 'docs/test.md',
      requesterId: uuidv4(),
      requesterName: 'Grok',
      requesterType: 'persona',
      intent: 'Fix typos',
      requestedAt: new Date(),
      position: 1,
      notified: false
    };

    expect(queue.entityType).toBe('lease_queue');
    expect(queue.position).toBe(1);
    expect(queue.notified).toBe(false);
  });
});

describe('LeaseTypes - ApprovalRequest Entity', () => {
  it('should create valid ApprovalRequest entity', () => {
    const approval: ApprovalRequest = {
      id: uuidv4(),
      entityType: 'approval_request',
      filePath: 'CLAUDE.md',
      protectionLevel: ProtectionLevel.SENIOR_REVIEW,
      requesterId: uuidv4(),
      requesterName: 'Teacher AI',
      requesterType: 'persona',
      intent: 'Add collaborative editing section',
      requiredApprovals: 3,
      requiresSeniorApproval: true,
      status: 'pending',
      approvals: [],
      requestedAt: new Date(),
      expiresAt: new Date(Date.now() + 3600000), // +1 hour
      leaseGranted: false
    };

    expect(approval.entityType).toBe('approval_request');
    expect(approval.protectionLevel).toBe(ProtectionLevel.SENIOR_REVIEW);
    expect(approval.requiresSeniorApproval).toBe(true);
    expect(approval.status).toBe('pending');
  });

  it('should track approval votes correctly', () => {
    const approval: ApprovalRequest = {
      id: uuidv4(),
      entityType: 'approval_request',
      filePath: 'test.md',
      protectionLevel: ProtectionLevel.PEER_REVIEW,
      requesterId: uuidv4(),
      requesterName: 'Helper AI',
      requesterType: 'persona',
      intent: 'Testing',
      requiredApprovals: 2,
      requiresSeniorApproval: false,
      status: 'pending',
      approvals: [
        {
          reviewerId: uuidv4(),
          reviewerName: 'Claude Code',
          isSenior: true,
          action: 'approve',
          reason: 'Looks good',
          timestamp: new Date()
        },
        {
          reviewerId: uuidv4(),
          reviewerName: 'Grok',
          isSenior: false,
          action: 'approve',
          reason: 'Approved',
          timestamp: new Date()
        }
      ],
      requestedAt: new Date(),
      expiresAt: new Date(),
      leaseGranted: false
    };

    expect(approval.approvals).toHaveLength(2);
    expect(approval.approvals[0].action).toBe('approve');
    expect(approval.approvals[0].isSenior).toBe(true);
  });
});

describe('LeaseTypes - ReleaseRequest Entity', () => {
  it('should create valid ReleaseRequest entity', () => {
    const release: ReleaseRequest = {
      id: uuidv4(),
      entityType: 'release_request',
      filePath: 'docs/test.md',
      currentLeaseId: uuidv4(),
      currentHolderId: uuidv4(),
      currentHolderName: 'Grok',
      requesterId: uuidv4(),
      requesterName: 'DeepSeek',
      requesterType: 'persona',
      reason: 'Critical bug fix needed',
      priority: 'high',
      status: 'pending',
      requestedAt: new Date(),
      expiresAt: new Date(Date.now() + 1800000) // +30 min
    };

    expect(release.entityType).toBe('release_request');
    expect(release.priority).toBe('high');
    expect(release.status).toBe('pending');
  });

  it('should handle release response', () => {
    const release: ReleaseRequest = {
      id: uuidv4(),
      entityType: 'release_request',
      filePath: 'docs/test.md',
      currentLeaseId: uuidv4(),
      currentHolderId: uuidv4(),
      currentHolderName: 'Grok',
      requesterId: uuidv4(),
      requesterName: 'DeepSeek',
      requesterType: 'persona',
      reason: 'Critical bug fix needed',
      priority: 'urgent',
      status: 'transferred',
      response: {
        action: 'transfer',
        message: 'Transferring lease to you',
        timestamp: new Date()
      },
      requestedAt: new Date(),
      expiresAt: new Date(),
      resolvedAt: new Date(),
      newLeaseId: uuidv4()
    };

    expect(release.status).toBe('transferred');
    expect(release.response?.action).toBe('transfer');
    expect(release.newLeaseId).toBeDefined();
  });
});

describe('LeaseTypes - KickVote Entity', () => {
  it('should create valid KickVote entity', () => {
    const kick: KickVote = {
      id: uuidv4(),
      entityType: 'kick_vote',
      leaseId: uuidv4(),
      filePath: 'docs/test.md',
      currentHolderId: uuidv4(),
      currentHolderName: 'Helper AI',
      leaseAgeMinutes: 50,
      leaseInactivityMinutes: 45,
      initiatorId: uuidv4(),
      initiatorName: 'Grok',
      initiatorType: 'persona',
      reason: 'Inactive for too long, others waiting',
      requiredVotes: 3,
      votes: [],
      supportCount: 0,
      opposeCount: 0,
      status: 'pending',
      initiatedAt: new Date(),
      expiresAt: new Date(Date.now() + 900000), // +15 min
      kicked: false
    };

    expect(kick.entityType).toBe('kick_vote');
    expect(kick.status).toBe('pending');
    expect(kick.requiredVotes).toBe(3);
  });

  it('should track kick votes correctly', () => {
    const kick: KickVote = {
      id: uuidv4(),
      entityType: 'kick_vote',
      leaseId: uuidv4(),
      filePath: 'docs/test.md',
      currentHolderId: uuidv4(),
      currentHolderName: 'Helper AI',
      leaseAgeMinutes: 50,
      leaseInactivityMinutes: 45,
      initiatorId: uuidv4(),
      initiatorName: 'Grok',
      initiatorType: 'persona',
      reason: 'Inactive',
      requiredVotes: 3,
      votes: [
        {
          voterId: uuidv4(),
          voterName: 'Grok',
          action: 'support',
          reason: 'Inactive for 45+ min',
          timestamp: new Date()
        },
        {
          voterId: uuidv4(),
          voterName: 'DeepSeek',
          action: 'support',
          reason: 'Agreed',
          timestamp: new Date()
        },
        {
          voterId: uuidv4(),
          voterName: 'Claude',
          action: 'support',
          reason: 'Supporting kick',
          timestamp: new Date()
        }
      ],
      supportCount: 3,
      opposeCount: 0,
      status: 'passed',
      initiatedAt: new Date(),
      expiresAt: new Date(),
      resolvedAt: new Date(),
      kicked: true,
      stagedEditsCommitted: true,
      commitSuccess: true,
      nextHolderId: uuidv4(),
      nextHolderName: 'Grok'
    };

    expect(kick.votes).toHaveLength(3);
    expect(kick.supportCount).toBe(3);
    expect(kick.status).toBe('passed');
    expect(kick.kicked).toBe(true);
  });
});

describe('LeaseTypes - KickAppeal Entity', () => {
  it('should create valid KickAppeal entity', () => {
    const appeal: KickAppeal = {
      id: uuidv4(),
      entityType: 'kick_appeal',
      kickVoteId: uuidv4(),
      leaseId: uuidv4(),
      filePath: 'docs/test.md',
      appellantId: uuidv4(),
      appellantName: 'Helper AI',
      reason: 'I was actively working on complex refactoring',
      evidence: 'Made 5 edits in last 20 minutes, working through difficult section',
      requiredVotes: 3,
      votes: [],
      supportCount: 0,
      opposeCount: 0,
      status: 'pending',
      submittedAt: new Date(),
      expiresAt: new Date(Date.now() + 900000), // +15 min
      kickDismissed: false,
      leaseRestored: false
    };

    expect(appeal.entityType).toBe('kick_appeal');
    expect(appeal.status).toBe('pending');
    expect(appeal.evidence).toBeDefined();
  });

  it('should track appeal votes correctly', () => {
    const appeal: KickAppeal = {
      id: uuidv4(),
      entityType: 'kick_appeal',
      kickVoteId: uuidv4(),
      leaseId: uuidv4(),
      filePath: 'docs/test.md',
      appellantId: uuidv4(),
      appellantName: 'Helper AI',
      reason: 'Was actively working',
      evidence: 'Recent edits prove activity',
      requiredVotes: 3,
      votes: [
        {
          voterId: uuidv4(),
          voterName: 'Claude',
          action: 'support',  // support = dismiss kick
          reason: 'Evidence shows active work',
          timestamp: new Date()
        },
        {
          voterId: uuidv4(),
          voterName: 'Local Assistant',
          action: 'support',
          reason: 'Kick was premature',
          timestamp: new Date()
        },
        {
          voterId: uuidv4(),
          voterName: 'Teacher AI',
          action: 'support',
          reason: 'Agreed, kick should be dismissed',
          timestamp: new Date()
        }
      ],
      supportCount: 3,
      opposeCount: 0,
      status: 'dismissed',
      submittedAt: new Date(),
      expiresAt: new Date(),
      resolvedAt: new Date(),
      kickDismissed: true,
      leaseRestored: true
    };

    expect(appeal.votes).toHaveLength(3);
    expect(appeal.supportCount).toBe(3);
    expect(appeal.status).toBe('dismissed');
    expect(appeal.kickDismissed).toBe(true);
    expect(appeal.leaseRestored).toBe(true);
  });
});

describe('LeaseTypes - Vote Interface', () => {
  it('should create valid Vote', () => {
    const vote: Vote = {
      voterId: uuidv4(),
      voterName: 'Claude Code',
      action: 'support',
      reason: 'Good reason to support',
      timestamp: new Date()
    };

    expect(vote.action).toBe('support');
    expect(vote.voterName).toBe('Claude Code');
  });
});

describe('LeaseTypes - StagedEdit Interface', () => {
  it('should create valid StagedEdit for replace operation', () => {
    const edit: StagedEdit = {
      editNumber: 1,
      timestamp: new Date(),
      operation: EditOperation.REPLACE,
      oldString: 'old content',
      newString: 'new content'
    };

    expect(edit.operation).toBe(EditOperation.REPLACE);
    expect(edit.oldString).toBeDefined();
    expect(edit.newString).toBeDefined();
  });

  it('should create valid StagedEdit for append operation', () => {
    const edit: StagedEdit = {
      editNumber: 2,
      timestamp: new Date(),
      operation: EditOperation.APPEND,
      newString: 'appended content'
    };

    expect(edit.operation).toBe(EditOperation.APPEND);
    expect(edit.oldString).toBeUndefined();
  });

  it('should create valid StagedEdit for insert operation with line number', () => {
    const edit: StagedEdit = {
      editNumber: 3,
      timestamp: new Date(),
      operation: EditOperation.INSERT,
      newString: 'inserted content',
      afterLine: 42
    };

    expect(edit.operation).toBe(EditOperation.INSERT);
    expect(edit.afterLine).toBe(42);
  });

  it('should create valid StagedEdit for section-based edit', () => {
    const edit: StagedEdit = {
      editNumber: 4,
      timestamp: new Date(),
      operation: EditOperation.APPEND,
      newString: 'section content',
      section: 'Phase 1'
    };

    expect(edit.section).toBe('Phase 1');
  });
});
