/**
 * Decision View Command - Shared Types
 *
 * View detailed information about a specific governance proposal
 */

import type { CommandParams, CommandResult, JTAGContext, CommandInput} from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { DecisionEntity } from '@system/data/entities/DecisionEntity';
import { Commands } from '../../../../../system/core/shared/Commands';

/**
 * Decision View Command Parameters
 */
export interface DecisionViewParams extends CommandParams {
  // Unique identifier for the proposal to view
  proposalId: string;
}

/**
 * Factory function for creating DecisionViewParams
 */
export const createDecisionViewParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Unique identifier for the proposal to view
    proposalId: string;
  }
): DecisionViewParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,

  ...data
});

/**
 * Decision View Command Result
 */
export interface DecisionViewResult extends CommandResult {
  // Whether the proposal was found
  success: boolean;
  // The complete proposal details (null if not found)
  proposal: DecisionEntity | null;
  // Human-readable summary of proposal status and results
  summary: string;
  error?: JTAGError;
}

/**
 * Factory function for creating DecisionViewResult with defaults
 */
export const createDecisionViewResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // The complete proposal details (null if not found)
    proposal?: DecisionEntity | null;
    // Human-readable summary of proposal status and results
    summary?: string;
    error?: JTAGError;
  }
): DecisionViewResult => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  success: data.success ?? false,
  proposal: data.proposal ?? null,
  summary: data.summary ?? '',
  error: data.error
});

/**
 * Smart Decision View-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createDecisionViewResultFromParams = (
  params: DecisionViewParams,
  differences: Omit<DecisionViewResult, 'context' | 'sessionId'>
): DecisionViewResult => transformPayload(params, differences);

/**
 * DecisionView — Type-safe command executor
 *
 * Usage:
 *   import { DecisionView } from '...shared/DecisionViewTypes';
 *   const result = await DecisionView.execute({ ... });
 */
export const DecisionView = {
  execute(params: CommandInput<DecisionViewParams>): Promise<DecisionViewResult> {
    return Commands.execute<DecisionViewParams, DecisionViewResult>('collaboration/decision/view', params as Partial<DecisionViewParams>);
  },
  commandName: 'collaboration/decision/view' as const,
} as const;
