/**
 * Live Leave Command Types
 *
 * Leave a live audio/video call.
 * Removes user from participants. Call ends when last person leaves.
 */

import type { CommandParams, CommandResult, CommandInput} from '@system/core/types/JTAGTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../../system/core/shared/Commands';

export interface LiveLeaveParams extends CommandParams {
  /**
   * Call to leave (UUID) - named sessionId for backwards compat but represents callId
   */
  sessionId: string;
}

export interface LiveLeaveResult extends CommandResult {
  success: boolean;
  message: string;

  /** Whether the call ended (last participant left) */
  sessionEnded: boolean;

  /** Remaining participant count */
  remainingParticipants: number;
}

/**
 * LiveLeave — Type-safe command executor
 *
 * Usage:
 *   import { LiveLeave } from '...shared/LiveLeaveTypes';
 *   const result = await LiveLeave.execute({ ... });
 */
export const LiveLeave = {
  execute(params: CommandInput<LiveLeaveParams>): Promise<LiveLeaveResult> {
    return Commands.execute<LiveLeaveParams, LiveLeaveResult>('collaboration/live/leave', params as Partial<LiveLeaveParams>);
  },
  commandName: 'collaboration/live/leave' as const,
} as const;
