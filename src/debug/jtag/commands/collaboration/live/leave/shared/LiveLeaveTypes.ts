/**
 * Live Leave Command Types
 *
 * Leave a live audio/video call.
 * Removes user from participants. Call ends when last person leaves.
 */

import type { CommandParams, CommandResult } from '@system/core/types/JTAGTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

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
