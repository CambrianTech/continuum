/**
 * Sentinel Approve Command - Shared Types
 *
 * Approve or reject a pending pipeline approval step. Resolves the blocking approval gate in the Rust executor.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';

import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Sentinel Approve Command Parameters
 */
export interface SentinelApproveParams extends CommandParams {
  // Sentinel handle ID with pending approval
  handle: string;
  // Whether to approve (true) or reject (false) the pending step
  approved: boolean;
  // Human-readable reason for the approval decision
  reason?: string;
  // UUID of the user or persona approving/rejecting
  approverId?: string;
}

/**
 * Factory function for creating SentinelApproveParams
 */
export const createSentinelApproveParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Sentinel handle ID with pending approval
    handle: string;
    // Whether to approve (true) or reject (false) the pending step
    approved: boolean;
    // Human-readable reason for the approval decision
    reason?: string;
    // UUID of the user or persona approving/rejecting
    approverId?: string;
  }
): SentinelApproveParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  reason: data.reason ?? '',
  approverId: data.approverId ?? '',
  ...data
});

/**
 * Sentinel Approve Command Result
 */
export interface SentinelApproveResult extends CommandResult {
  success: boolean;
  // The sentinel handle that was approved/rejected
  handle: string;
  // Whether the approval was granted
  approved: boolean;
  error?: string;
}

/**
 * Factory function for creating SentinelApproveResult with defaults
 */
export const createSentinelApproveResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // The sentinel handle that was approved/rejected
    handle?: string;
    // Whether the approval was granted
    approved?: boolean;
    error?: string;
  }
): SentinelApproveResult => createPayload(context, sessionId, {
  handle: data.handle ?? '',
  approved: data.approved ?? false,
  ...data
});

/**
 * Smart Sentinel Approve-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createSentinelApproveResultFromParams = (
  params: SentinelApproveParams,
  differences: Omit<SentinelApproveResult, 'context' | 'sessionId' | 'userId'>
): SentinelApproveResult => transformPayload(params, differences);

/**
 * Sentinel Approve — Type-safe command executor
 *
 * Usage:
 *   import { SentinelApprove } from '...shared/SentinelApproveTypes';
 *   const result = await SentinelApprove.execute({ ... });
 */
export const SentinelApprove = {
  execute(params: CommandInput<SentinelApproveParams>): Promise<SentinelApproveResult> {
    return Commands.execute<SentinelApproveParams, SentinelApproveResult>('sentinel/approve', params as Partial<SentinelApproveParams>);
  },
  commandName: 'sentinel/approve' as const,
} as const;
