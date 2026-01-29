/**
 * Decision List Command - Shared Types
 *
 * List all governance proposals with optional filtering
 */

import type { CommandParams, CommandResult, JTAGContext, CommandInput} from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { DecisionEntity } from '@system/data/entities/DecisionEntity';
import { Commands } from '../../../../../system/core/shared/Commands';

/**
 * Decision List Command Parameters
 */
export interface DecisionListParams extends CommandParams {
  // Filter by status: 'open', 'finalized', or 'archived' (optional)
  status?: string;
  // Filter by domain tag (optional)
  domain?: string;
  // Filter by proposer user ID (optional)
  proposedBy?: UUID;
  // Maximum number of proposals to return (default: 50)
  limit?: number;
  // Number of proposals to skip for pagination (default: 0)
  offset?: number;
}

/**
 * Factory function for creating DecisionListParams
 */
export const createDecisionListParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Filter by status: 'open', 'finalized', or 'archived' (optional)
    status?: string;
    // Filter by domain tag (optional)
    domain?: string;
    // Filter by proposer user ID (optional)
    proposedBy?: UUID;
    // Maximum number of proposals to return (default: 50)
    limit?: number;
    // Number of proposals to skip for pagination (default: 0)
    offset?: number;
  }
): DecisionListParams => createPayload(context, sessionId, {

  ...data
});

/**
 * Decision List Command Result
 */
export interface DecisionListResult extends CommandResult {
  // Whether the query succeeded
  success: boolean;
  // Array of matching proposals
  proposals: DecisionEntity[];
  // Total number of matching proposals (before pagination)
  total: number;
  // The limit that was applied
  limit: number;
  // The offset that was applied
  offset: number;
  error?: JTAGError;
}

/**
 * Factory function for creating DecisionListResult with defaults
 */
export const createDecisionListResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Array of matching proposals
    proposals?: DecisionEntity[];
    // Total number of matching proposals (before pagination)
    total?: number;
    // The limit that was applied
    limit?: number;
    // The offset that was applied
    offset?: number;
    error?: JTAGError;
  }
): DecisionListResult => createPayload(context, sessionId, {
  success: data.success ?? false,
  proposals: data.proposals ?? [],
  total: data.total ?? 0,
  limit: data.limit ?? 0,
  offset: data.offset ?? 0,
  error: data.error
});

/**
 * Smart Decision List-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createDecisionListResultFromParams = (
  params: DecisionListParams,
  differences: Omit<DecisionListResult, 'context' | 'sessionId'>
): DecisionListResult => transformPayload(params, differences);

/**
 * DecisionList — Type-safe command executor
 *
 * Usage:
 *   import { DecisionList } from '...shared/DecisionListTypes';
 *   const result = await DecisionList.execute({ ... });
 */
export const DecisionList = {
  execute(params: CommandInput<DecisionListParams>): Promise<DecisionListResult> {
    return Commands.execute<DecisionListParams, DecisionListResult>('collaboration/decision/list', params as Partial<DecisionListParams>);
  },
  commandName: 'collaboration/decision/list' as const,
} as const;
