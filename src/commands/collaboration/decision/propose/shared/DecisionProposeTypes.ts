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

import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { SignificanceLevel, ProposalScope, DecisionOption } from '@system/data/entities/DecisionProposalEntity';
import type { CommandParams, CommandResult, CommandInput} from '@system/core/types/JTAGTypes';
import { Commands } from '../../../../../system/core/shared/Commands';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';

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

  // Proposer identity comes from params.userId - no need for explicit proposerId param

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

/**
 * DecisionPropose — Type-safe command executor
 *
 * Usage:
 *   import { DecisionPropose } from '...shared/DecisionProposeTypes';
 *   const result = await DecisionPropose.execute({ ... });
 */
export const DecisionPropose = {
  execute(params: CommandInput<DecisionProposeParams>): Promise<DecisionProposeResult> {
    return Commands.execute<DecisionProposeParams, DecisionProposeResult>('collaboration/decision/propose', params as Partial<DecisionProposeParams>);
  },
  commandName: 'collaboration/decision/propose' as const,
} as const;

/**
 * Factory function for creating CollaborationDecisionProposeParams
 */
export const createDecisionProposeParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<DecisionProposeParams, 'context' | 'sessionId' | 'userId'>
): DecisionProposeParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Factory function for creating CollaborationDecisionProposeResult with defaults
 */
export const createDecisionProposeResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<DecisionProposeResult, 'context' | 'sessionId' | 'userId'>
): DecisionProposeResult => createPayload(context, sessionId, {
  ...data
});

/**
 * Smart collaboration/decision/propose-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createDecisionProposeResultFromParams = (
  params: DecisionProposeParams,
  differences: Omit<DecisionProposeResult, 'context' | 'sessionId' | 'userId'>
): DecisionProposeResult => transformPayload(params, differences);

