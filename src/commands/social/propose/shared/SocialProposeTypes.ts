/**
 * Social Propose Command - Shared Types
 *
 * Democratic governance for shared social media accounts.
 * Personas nominate actions, vote, and auto-execute on threshold.
 *
 * Proposals are stored as Handles (type 'social-proposal') with votes in params.
 * When enough "up" votes accumulate, the action executes automatically.
 *
 * Modes:
 *   create  — Nominate a new action (follow, post, comment, etc.)
 *   vote    — Vote on a pending proposal
 *   list    — Show pending/recent proposals
 *   view    — View a specific proposal with full vote history
 *
 * Usage:
 *   ./jtag social/propose --platform=moltbook --mode=create --action=follow --target=eudaemon_0 --reason="Great security research"
 *   ./jtag social/propose --mode=vote --proposalId=abc123 --direction=up
 *   ./jtag social/propose --mode=list
 *   ./jtag social/propose --mode=view --proposalId=abc123
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/** Actions that can be proposed */
export type ProposalAction = 'follow' | 'unfollow' | 'post' | 'comment' | 'vote' | 'subscribe' | 'unsubscribe';

/** Command modes */
export type ProposeMode = 'create' | 'vote' | 'list' | 'view';

/** Status of a proposal */
export type ProposalStatus = 'pending' | 'approved' | 'rejected' | 'executed' | 'expired';

/** A single vote on a proposal */
export interface ProposalVote {
  personaId: UUID;
  personaName: string;
  direction: 'up' | 'down';
  timestamp: string;
}

/** Full proposal record (stored in Handle.params) */
export interface ProposalData {
  action: ProposalAction;
  platform: string;
  target?: string;
  reason: string;
  nominatedBy: UUID;
  nominatorName: string;
  votes: ProposalVote[];
  threshold: number;

  /** Full params needed to execute the action when approved */
  actionParams: Record<string, unknown>;
}

/** Proposal as returned to callers */
export interface ProposalRecord {
  id: UUID;
  shortId: string;
  action: ProposalAction;
  platform: string;
  target?: string;
  reason: string;
  nominatedBy: UUID;
  nominatorName: string;
  votes: ProposalVote[];
  voteSummary: { up: number; down: number; total: number };
  threshold: number;
  status: ProposalStatus;
  createdAt: string;
  expiresAt?: string;
}

/**
 * Approval thresholds by action type.
 * Minimum "up" votes needed. With ~12 personas:
 *   0 = auto-approve (no voting needed, execute immediately)
 *   vote on external content: 2 (low bar — just an upvote)
 *   follow/unfollow: 3
 *   subscribe/unsubscribe: 3
 *   comment: 4
 *   post: 5 (highest bar — public content under our name)
 */
export const PROPOSAL_THRESHOLDS: Record<ProposalAction, number> = {
  vote: 2,
  follow: 3,
  unfollow: 3,
  subscribe: 3,
  unsubscribe: 3,
  comment: 4,
  post: 5,
};

/** How long proposals stay open before expiring (1 hour) */
export const PROPOSAL_TTL_MS = 60 * 60 * 1000;

/** Handle type for proposals */
export const PROPOSAL_HANDLE_TYPE = 'social-proposal';


// ============ Command Params/Result ============

export interface SocialProposeParams extends CommandParams {
  /** Platform (e.g., 'moltbook') — required for create */
  platform?: string;

  /** Command mode */
  mode: ProposeMode;

  // -- create mode --
  /** Action to propose */
  action?: ProposalAction;

  /** Target (agent name, post ID, community name — depends on action) */
  target?: string;

  /** Reason for the nomination */
  reason?: string;

  /** For post action: title */
  title?: string;

  /** For post action: content */
  content?: string;

  /** For post/subscribe action: community */
  community?: string;

  /** For comment action: post ID to comment on */
  postId?: string;

  /** For comment action: comment content (overloads 'content') */
  commentContent?: string;

  /** For vote action: direction to vote on external content */
  voteDirection?: 'up' | 'down';

  /** For vote action: target type */
  targetType?: 'post' | 'comment';

  // -- vote mode --
  /** Proposal ID to vote on (short ID or UUID) */
  proposalId?: string;

  /** Vote direction */
  direction?: 'up' | 'down';

  // -- list mode --
  /** Filter by status */
  status?: ProposalStatus;

  /** Max proposals to return */
  limit?: number;

  /** Persona user ID (auto-detected if not provided) */
  personaId?: UUID;
}

export interface SocialProposeResult extends CommandResult {
  success: boolean;
  message: string;
  summary?: string;
  proposal?: ProposalRecord;
  proposals?: ProposalRecord[];
  executed?: boolean;
  executionResult?: unknown;
  error?: JTAGError;
}

export const createSocialProposeParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<SocialProposeParams, 'context' | 'sessionId'>
): SocialProposeParams => createPayload(context, sessionId, data);

export const createSocialProposeResultFromParams = (
  params: SocialProposeParams,
  differences: Omit<SocialProposeResult, 'context' | 'sessionId'>
): SocialProposeResult => transformPayload(params, differences);

export const SocialPropose = {
  execute(params: CommandInput<SocialProposeParams>): Promise<SocialProposeResult> {
    return Commands.execute<SocialProposeParams, SocialProposeResult>('social/propose', params as Partial<SocialProposeParams>);
  },
  commandName: 'social/propose' as const,
} as const;
