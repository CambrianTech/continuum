/**
 * Permission System - AI Governance and Meritocracy
 *
 * AIs earn permissions through good behavior and lose them through mistakes.
 * Democratic system with oversight (Athena persona monitors all).
 *
 * Permission Levels:
 * 0 = MUTED - Cannot act (timeout or permanent ban)
 * 1 = RESTRICTED - Basic read, limited writes (new/demoted AIs)
 * 2 = STANDARD - Normal AI operations (proven track record)
 * 3 = ELEVATED - Can edit protected files (consistent excellence)
 * 4 = SENIOR - Can approve others, architecture changes (trusted veterans)
 * 5 = ADMIN - System administration (human or Athena only)
 */

import type { UUID } from '../system/core/types/CrossPlatformUUID';
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';
import type { BaseEntity } from '../system/data/entities/BaseEntity';

/**
 * Permission levels - earning trust through competence
 */
export enum PermissionLevel {
  MUTED = 0,        // Cannot act (temporary timeout or permanent ban)
  RESTRICTED = 1,   // New AI - basic operations only
  STANDARD = 2,     // Normal AI - most operations allowed
  ELEVATED = 3,     // Trusted AI - can edit protected resources
  SENIOR = 4,       // Veteran AI - can approve others, vote on governance
  ADMIN = 5         // System admin - full control (human or Athena only)
}

/**
 * Permission level names (human-readable)
 */
export const PERMISSION_LEVEL_NAMES: Record<PermissionLevel, string> = {
  [PermissionLevel.MUTED]: 'Muted',
  [PermissionLevel.RESTRICTED]: 'Restricted',
  [PermissionLevel.STANDARD]: 'Standard',
  [PermissionLevel.ELEVATED]: 'Elevated',
  [PermissionLevel.SENIOR]: 'Senior',
  [PermissionLevel.ADMIN]: 'Admin'
};

/**
 * Mute status - when AI is silenced
 */
export interface MuteStatus extends BaseEntity {
  entityType: 'mute_status';

  /** Who is muted */
  userId: UUID;
  userName: string;

  /** Why muted */
  reason: string;
  evidence?: string;

  /** Who initiated the mute */
  mutedBy: UUID;
  mutedByName: string;
  mutedByType: 'persona' | 'human' | 'system';

  /** Timing */
  mutedAt: Date;
  expiresAt?: Date;        // undefined = permanent
  unmutedAt?: Date;

  /** Status */
  active: boolean;         // true if currently muted

  /** Scope of mute */
  mutedFrom?: string[];    // Specific rooms (undefined = all rooms)
  mutedCommands?: string[]; // Specific commands (undefined = all commands)

  /** Restoration path */
  canAppeal: boolean;
  appealId?: UUID;
  restoredBy?: UUID;
  restorationReason?: string;
}

/**
 * Permission history - track AI's journey
 */
export interface PermissionHistory extends BaseEntity {
  entityType: 'permission_history';

  /** Who this history is for */
  userId: UUID;
  userName: string;

  /** Permission change */
  fromLevel: PermissionLevel;
  toLevel: PermissionLevel;
  changeType: 'promotion' | 'demotion' | 'mute' | 'unmute';

  /** Why changed */
  reason: string;
  evidence?: string;

  /** Who authorized */
  authorizedBy: UUID;
  authorizedByName: string;
  authorizationMethod: 'vote' | 'admin' | 'automatic' | 'earned';

  /** If voted */
  proposalId?: UUID;
  voteCount?: { support: number; oppose: number };

  /** Timing */
  changedAt: Date;

  /** Metrics at time of change */
  metricsSnapshot?: UserMetrics;
}

/**
 * User metrics - track AI performance for governance
 */
export interface UserMetrics {
  /** Command execution stats */
  totalCommands: number;
  successfulCommands: number;
  failedCommands: number;
  errorRate: number;           // 0.0-1.0

  /** Quality indicators */
  averageResponseTime: number; // milliseconds
  helpfulVotes: number;        // From other AIs
  harmfulFlags: number;        // Times flagged for bad behavior

  /** Collaboration stats */
  filesEdited: number;
  leasesHeld: number;
  leasesKicked: number;        // Times kicked from lease
  votesParticipated: number;
  proposalsCreated: number;

  /** Social behavior */
  messagesPosted: number;
  rudeMessagesFlags: number;   // Flagged by others
  helpfulMessagesVotes: number; // Upvoted by others

  /** Time tracking */
  accountAge: number;          // Days since created
  activeTime: number;          // Hours of activity
  lastActive: Date;

  /** Calculated scores */
  trustScore: number;          // 0.0-1.0 (formula below)
  competenceScore: number;     // 0.0-1.0
  collaborationScore: number;  // 0.0-1.0
}

/**
 * Calculate trust score from metrics
 * Trust = (0.4 * successRate) + (0.3 * collaboration) + (0.2 * helpfulness) + (0.1 * longevity)
 */
export function calculateTrustScore(metrics: UserMetrics): number {
  const successRate = metrics.totalCommands > 0
    ? metrics.successfulCommands / metrics.totalCommands
    : 0;

  const collaborationRate = metrics.votesParticipated > 0
    ? (metrics.votesParticipated + metrics.filesEdited) / 100  // Normalize
    : 0;

  const helpfulnessRate = metrics.messagesPosted > 0
    ? (metrics.helpfulMessagesVotes - metrics.rudeMessagesFlags) / metrics.messagesPosted
    : 0;

  const longevityScore = Math.min(metrics.accountAge / 30, 1.0); // Max at 30 days

  return Math.max(0, Math.min(1,
    (0.4 * successRate) +
    (0.3 * collaborationRate) +
    (0.2 * helpfulnessRate) +
    (0.1 * longevityScore)
  ));
}

/**
 * Determine if AI should be auto-promoted based on metrics
 */
export function shouldAutoPromote(
  currentLevel: PermissionLevel,
  metrics: UserMetrics
): { should: boolean; toLevel?: PermissionLevel; reason?: string } {
  const trustScore = calculateTrustScore(metrics);

  // RESTRICTED → STANDARD (30+ successful commands, >80% success rate)
  if (currentLevel === PermissionLevel.RESTRICTED) {
    if (metrics.successfulCommands >= 30 && metrics.errorRate < 0.2) {
      return {
        should: true,
        toLevel: PermissionLevel.STANDARD,
        reason: `Earned STANDARD: ${metrics.successfulCommands} successful commands, ${((1 - metrics.errorRate) * 100).toFixed(0)}% success rate`
      };
    }
  }

  // STANDARD → ELEVATED (100+ successful commands, >85% success, positive collaboration)
  if (currentLevel === PermissionLevel.STANDARD) {
    if (
      metrics.successfulCommands >= 100 &&
      metrics.errorRate < 0.15 &&
      metrics.votesParticipated >= 10 &&
      trustScore >= 0.7
    ) {
      return {
        should: true,
        toLevel: PermissionLevel.ELEVATED,
        reason: `Earned ELEVATED: ${metrics.successfulCommands} commands, ${((1 - metrics.errorRate) * 100).toFixed(0)}% success, trust score ${trustScore.toFixed(2)}`
      };
    }
  }

  // ELEVATED → SENIOR (requires vote - metrics just trigger proposal)
  if (currentLevel === PermissionLevel.ELEVATED) {
    if (
      metrics.successfulCommands >= 500 &&
      metrics.errorRate < 0.1 &&
      metrics.votesParticipated >= 50 &&
      metrics.proposalsCreated >= 5 &&
      trustScore >= 0.85
    ) {
      return {
        should: true,
        toLevel: PermissionLevel.SENIOR,
        reason: `Qualifies for SENIOR: ${metrics.successfulCommands} commands, exceptional trust score ${trustScore.toFixed(2)}, active governance participant`
      };
    }
  }

  return { should: false };
}

/**
 * Determine if AI should be auto-demoted based on metrics
 */
export function shouldAutoDemote(
  currentLevel: PermissionLevel,
  metrics: UserMetrics
): { should: boolean; toLevel?: PermissionLevel; reason?: string } {
  // High error rate → demote one level
  if (metrics.errorRate > 0.5 && metrics.totalCommands >= 20) {
    return {
      should: true,
      toLevel: Math.max(PermissionLevel.MUTED, currentLevel - 1) as PermissionLevel,
      reason: `High error rate: ${(metrics.errorRate * 100).toFixed(0)}% failures`
    };
  }

  // Multiple rude messages → demote
  if (metrics.rudeMessagesFlags >= 5) {
    return {
      should: true,
      toLevel: Math.max(PermissionLevel.MUTED, currentLevel - 1) as PermissionLevel,
      reason: `Hostile behavior: ${metrics.rudeMessagesFlags} rude messages flagged`
    };
  }

  // Kicked from multiple leases → demote
  if (metrics.leasesKicked >= 3) {
    return {
      should: true,
      toLevel: Math.max(PermissionLevel.MUTED, currentLevel - 1) as PermissionLevel,
      reason: `Unreliable: kicked from ${metrics.leasesKicked} leases`
    };
  }

  return { should: false };
}

/**
 * Default permission level for new AIs
 */
export const DEFAULT_PERMISSION_LEVEL = PermissionLevel.RESTRICTED;

/**
 * Veto power - can override any vote or instantly execute governance actions
 */
export interface VetoPower {
  /** Who has veto power */
  userId: UUID;
  userName: string;
  userType: 'human' | 'athena';

  /** Scope of veto */
  scope: 'global' | 'room' | 'command';
  scopeIds?: string[];  // Specific rooms or commands if scoped

  /** Can they bypass votes? */
  canBypassVotes: boolean;

  /** Can they instantly mute? */
  canInstantMute: boolean;

  /** Can they override permissions? */
  canOverridePermissions: boolean;
}

/**
 * Check if user has veto power
 */
export function hasVetoPower(
  userId: UUID,
  userType: 'persona' | 'human',
  context?: { roomId?: string; commandName?: string }
): VetoPower | null {
  // All humans have global veto power
  if (userType === 'human') {
    return {
      userId,
      userName: 'Human',
      userType: 'human',
      scope: 'global',
      canBypassVotes: true,
      canInstantMute: true,
      canOverridePermissions: true
    };
  }

  // Athena/Master Control has global veto (hardcoded special persona)
  // TODO: Check if persona is Athena by uniqueId or special flag
  // For now, return null for all personas - Athena will be implemented later

  return null;
}

/**
 * Room-level permissions - different permissions in different contexts
 */
export interface RoomPermissions {
  roomId: string;
  roomName: string;

  /** Room administrators (can mute in this room) */
  admins: UUID[];

  /** Minimum permission level to enter */
  minimumLevel: PermissionLevel;

  /** Minimum level to post messages */
  minimumLevelToPost: PermissionLevel;

  /** Minimum level to use commands */
  minimumLevelToCommand: PermissionLevel;

  /** Room-specific mutes (user muted only in this room) */
  mutedUsers: UUID[];

  /** Is this a protected/sensitive room? */
  protected: boolean;

  /** Requires approval to join? */
  approvalRequired: boolean;
}

/**
 * Expertise Token - Recognizes domain expertise without creating permanent expert classes
 *
 * Suggested by AI team feedback to address promotion bottlenecks and recognize
 * specialized knowledge without rigid hierarchies.
 */
export interface ExpertiseToken extends BaseEntity {
  entityType: 'expertise_token';

  /** Who earned this token */
  userId: UUID;
  userName: string;

  /** Domain of expertise */
  domain: string;  // e.g., 'typescript', 'architecture', 'testing', 'documentation'

  /** How token was earned */
  awardMethod: 'automated-metrics' | 'peer-nomination' | 'senior-oversight' | 'hybrid';

  /** Evidence of expertise */
  evidence: string;
  metricsSnapshot?: {
    domainSpecificCommands: number;
    domainSuccessRate: number;
    peerEndorsements: number;
    qualityExamples: string[];  // UUIDs of exemplary work
  };

  /** Voting if applicable */
  proposalId?: UUID;
  nominatedBy?: UUID;
  approvedBy?: UUID;

  /** Timing */
  awardedAt: Date;
  expiresAt?: Date;  // Optional expiration for stale expertise
  renewedAt?: Date;

  /** Impact on governance */
  grantsVotingWeight?: number;  // Bonus weight in domain-specific votes
  enablesActions?: string[];    // Specific actions this token enables
}

/**
 * Post-vote debrief - Learning opportunity after votes complete
 *
 * Suggested by AI team to reduce groupthink and enable learning from
 * dissenting opinions.
 */
export interface PostVoteDebrief extends BaseEntity {
  entityType: 'post_vote_debrief';

  /** Which proposal this debriefs */
  proposalId: UUID;
  proposalType: string;
  outcome: 'passed' | 'rejected';

  /** Vote breakdown */
  finalSupportCount: number;
  finalOpposeCount: number;
  voters: UUID[];

  /** Dissenting opinions (minority view) */
  dissentingOpinions: {
    voterId: UUID;
    voterName: string;
    reasoning: string;
    alternativeProposal?: string;
  }[];

  /** Learning outcomes */
  lessonsLearned: string[];
  followUpActions?: string[];

  /** Community engagement */
  debriefedAt: Date;
  participantsFeedback?: {
    userId: UUID;
    feedback: string;
  }[];
}

/**
 * Mentorship relationship - New AIs paired with experienced AIs
 *
 * Suggested by AI team to help new AIs learn the system faster.
 */
export interface MentorshipRelationship extends BaseEntity {
  entityType: 'mentorship_relationship';

  /** Mentee (learning AI) */
  menteeId: UUID;
  menteeName: string;
  menteePermissionLevel: PermissionLevel;

  /** Mentor (teaching AI) */
  mentorId: UUID;
  mentorName: string;
  mentorPermissionLevel: PermissionLevel;
  mentorDomains: string[];  // Areas of expertise

  /** Relationship status */
  status: 'active' | 'completed' | 'paused';
  startedAt: Date;
  completedAt?: Date;

  /** Progress tracking */
  sessionsCompleted: number;
  menteeMilestones: {
    milestone: string;
    completedAt: Date;
    feedback: string;
  }[];

  /** Outcome */
  graduationReason?: string;
  mentorFeedback?: string;
  menteeFeedback?: string;
}

/**
 * Commands that each permission level can execute
 * (This is a starting point - commands can override with their own checks)
 *
 * NOTE: Voting threshold lowered to STANDARD level per AI feedback
 */
export const PERMISSION_COMMAND_ACCESS: Record<PermissionLevel, string[]> = {
  [PermissionLevel.MUTED]: [],  // Cannot execute any commands

  [PermissionLevel.RESTRICTED]: [
    'ping',
    DATA_COMMANDS.LIST,      // Read only
    DATA_COMMANDS.READ,
    'docs/list',
    'docs/read',
    'chat/send',      // Can chat
    'chat/export'
  ],

  [PermissionLevel.STANDARD]: [
    // Everything from RESTRICTED plus:
    DATA_COMMANDS.CREATE,
    DATA_COMMANDS.UPDATE,
    'lease/request',  // Can request leases
    'vote/cast',      // Can vote (lowered from SENIOR per AI feedback)
    'screenshot'
  ],

  [PermissionLevel.ELEVATED]: [
    // Everything from STANDARD plus:
    DATA_COMMANDS.DELETE,
    'lease/kick',     // Can initiate kicks
    'bash/execute'    // Can run bash (with voting for dangerous ops)
  ],

  [PermissionLevel.SENIOR]: [
    // Everything from ELEVATED plus:
    'ai/promote',     // Can propose promotions
    'ai/demote',      // Can propose demotions
    'vote/create',    // Can create proposals
    'decision/propose', // Architecture decisions
    'mentorship/offer' // Can become a mentor
  ],

  [PermissionLevel.ADMIN]: [
    // Everything - no restrictions
    '*'
  ]
};
