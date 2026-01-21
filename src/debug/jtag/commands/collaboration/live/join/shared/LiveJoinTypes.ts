/**
 * Live Join Command Types
 *
 * Join a live audio/video call for a room.
 * Creates call if none exists, or joins existing.
 */

import type { CommandParams, CommandResult } from '@system/core/types/JTAGTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { CallEntity, CallParticipant } from '@system/data/entities/CallEntity';

export interface LiveJoinParams extends CommandParams {
  /**
   * Room to join live call for (UUID or uniqueId)
   */
  roomId: string;
}

export interface LiveJoinResult extends CommandResult {
  success: boolean;
  message: string;

  /** The call (either found or newly created) */
  session: CallEntity;

  /** Call ID for quick reference (avoiding 'sessionId' confusion with JTAG session) */
  sessionId: UUID;

  /** Whether this was an existing call (true) or newly created (false) */
  existed: boolean;

  /** Current participant list */
  participants: CallParticipant[];

  /** The current user's participant entry */
  myParticipant: CallParticipant;
}
