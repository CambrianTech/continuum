/**
 * decision/propose - Create a new decision proposal with ranked-choice voting
 *
 * Enables AIs to:
 * - Propose decisions to the team
 * - Define multiple options for ranked-choice voting
 * - Target specific expert groups via scope
 * - Set urgency via significance level
 * - Automatically link to related past decisions
 */

import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { SignificanceLevel, ProposalScope, DecisionOption } from '../../../../system/data/entities/DecisionProposalEntity';
import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';

export interface DecisionProposeParams extends CommandParams {
  /** What decision is being made */
  topic: string;

  /** Background rationale explaining why this decision is needed */
  rationale: string;

  /** Keywords for similarity matching (auto-extracted from topic if not provided) */
  tags?: string[];

  /** Options to choose from (minimum 2) */
  options: Array<{
    label: string;
    description: string;
    proposedBy?: UUID; // Track who proposed this option
  }>;

  /** Who should vote on this? */
  scope?: ProposalScope; // Default: 'all'

  /** How urgent is this? Determines response window */
  significanceLevel?: SignificanceLevel; // Default: 'medium'

  /** Who is proposing this decision */
  proposerId?: UUID; // Default: inferred from session

  /** Chat room context where proposal originated */
  contextId?: UUID; // Default: inferred from session
}

export interface DecisionProposeResult extends CommandResult {
  success: boolean;
  proposalId?: UUID;
  shortId?: string; // Last 6 chars of proposalId for easy reference (#abc123)
  deadline?: number;
  notifiedCount?: number;
  relatedProposals?: UUID[];
  error?: string;
}
