/**
 * Lease Request Command - Shared Types
 *
 * Command to request a file lease for editing
 */

import type { JTAGContext, CommandParams, CommandResult, CommandInput} from '@system/core/types/JTAGTypes';
import { transformPayload } from '@system/core/types/JTAGTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { FileLease } from '@shared/LeaseTypes';
import { Commands } from '../../../../../system/core/shared/Commands';

/**
 * Lease Request Parameters
 */
export interface LeaseRequestParams extends CommandParams {
  /** Path to the file to request lease for */
  readonly filePath: string;

  /** ID of the requesting user/persona */
  readonly requesterId: UUID;

  /** Name of the requester */
  readonly requesterName: string;

  /** Type of requester */
  readonly requesterType: 'persona' | 'human';

  /** Intent/reason for requesting the lease */
  readonly intent: string;

  /** Optional duration in seconds (default: 1800 = 30 minutes) */
  readonly durationSeconds?: number;
}

/**
 * Lease Request Result
 */
export interface LeaseRequestResult extends CommandResult {
  readonly success: boolean;
  readonly timestamp: string;

  /** The granted lease (if successful) */
  readonly lease?: FileLease;

  /** Approval request ID (if approval required) */
  readonly approvalId?: UUID;

  /** Queue position (if queued) */
  readonly queuePosition?: number;

  /** Error message (if failed) */
  readonly error?: string;

  /** Additional message for user */
  readonly message?: string;
}

/**
 * Factory function for creating LeaseRequestParams
 */
export const createLeaseRequestParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<LeaseRequestParams, 'context' | 'sessionId'>
): LeaseRequestParams => ({
  userId: data.userId,
  context,
  sessionId,
  filePath: data.filePath,
  requesterId: data.requesterId,
  requesterName: data.requesterName,
  requesterType: data.requesterType,
  intent: data.intent,
  durationSeconds: data.durationSeconds
});

/**
 * Transform params to result
 */
export const createLeaseRequestResultFromParams = (
  params: LeaseRequestParams,
  differences: Omit<Partial<LeaseRequestResult>, 'context' | 'sessionId'>
): LeaseRequestResult => transformPayload(params, {
  success: false,
  timestamp: new Date().toISOString(),
  ...differences
});

/**
 * LeaseRequest — Type-safe command executor
 *
 * Usage:
 *   import { LeaseRequest } from '...shared/LeaseRequestTypes';
 *   const result = await LeaseRequest.execute({ ... });
 */
export const LeaseRequest = {
  execute(params: CommandInput<LeaseRequestParams>): Promise<LeaseRequestResult> {
    return Commands.execute<LeaseRequestParams, LeaseRequestResult>('utilities/lease/request', params as Partial<LeaseRequestParams>);
  },
  commandName: 'utilities/lease/request' as const,
} as const;
