/**
 * Decision Create Command - Shared Types
 *
 * Create a new governance proposal with voting options
 */

import type { CommandParams, CommandResult, JTAGContext, CommandInput} from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { DecisionOption } from '@system/data/entities/DecisionEntity';
import { Commands } from '../../../../../system/core/shared/Commands';

/**
 * Decision Create Command Parameters
 */
export interface DecisionCreateParams extends CommandParams {
  // Unique identifier for the proposal (e.g., 'PROP-2024-001')
  proposalId: string;
  // Short title for the proposal
  topic: string;
  // Why this decision is needed
  rationale: string;
  // Detailed explanation of the proposal
  description: string;
  // Array of voting options (minimum 2 required)
  options: DecisionOption[];
  // Optional tags for categorization
  tags?: string[];
  // Optional ISO timestamp for voting deadline
  votingDeadline?: string;
  // Optional minimum number of votes required
  requiredQuorum?: number;
  // Visibility level: 'public' or 'private' (defaults to 'public')
  visibility?: string;
}

/**
 * Factory function for creating DecisionCreateParams
 */
export const createDecisionCreateParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Unique identifier for the proposal (e.g., 'PROP-2024-001')
    proposalId: string;
    // Short title for the proposal
    topic: string;
    // Why this decision is needed
    rationale: string;
    // Detailed explanation of the proposal
    description: string;
    // Array of voting options (minimum 2 required)
    options: DecisionOption[];
    // Optional tags for categorization
    tags: string[];
    // Optional ISO timestamp for voting deadline
    votingDeadline: string;
    // Optional minimum number of votes required
    requiredQuorum: number;
    // Visibility level: 'public' or 'private' (defaults to 'public')
    visibility: string;
  }
): DecisionCreateParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,

  ...data
});

/**
 * Decision Create Command Result
 */
export interface DecisionCreateResult extends CommandResult {
  // Whether proposal was created successfully
  success: boolean;
  // The created proposal ID
  proposalId: string;
  // ID of the user who created the proposal
  proposedBy: UUID;
  // ISO timestamp of creation
  proposedAt: string;
  // Initial status (typically 'open')
  status: string;
  error?: JTAGError;
}

/**
 * Factory function for creating DecisionCreateResult with defaults
 */
export const createDecisionCreateResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Whether proposal was created successfully
    success: boolean;
    // The created proposal ID
    proposalId?: string;
    // ID of the user who created the proposal
    proposedBy?: UUID;
    // ISO timestamp of creation
    proposedAt?: string;
    // Initial status (typically 'open')
    status?: string;
    error?: JTAGError;
  }
): DecisionCreateResult => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data,
  success: data.success ?? false,
  proposalId: data.proposalId ?? '',
  proposedBy: data.proposedBy ?? ('' as UUID),
  proposedAt: data.proposedAt ?? '',
  status: data.status ?? ''
});

/**
 * Smart Decision Create-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createDecisionCreateResultFromParams = (
  params: DecisionCreateParams,
  differences: Omit<DecisionCreateResult, 'context' | 'sessionId'>
): DecisionCreateResult => transformPayload(params, differences);

/**
 * DecisionCreate — Type-safe command executor
 *
 * Usage:
 *   import { DecisionCreate } from '...shared/DecisionCreateTypes';
 *   const result = await DecisionCreate.execute({ ... });
 */
export const DecisionCreate = {
  execute(params: CommandInput<DecisionCreateParams>): Promise<DecisionCreateResult> {
    return Commands.execute<DecisionCreateParams, DecisionCreateResult>('collaboration/decision/create', params as Partial<DecisionCreateParams>);
  },
  commandName: 'collaboration/decision/create' as const,
} as const;
