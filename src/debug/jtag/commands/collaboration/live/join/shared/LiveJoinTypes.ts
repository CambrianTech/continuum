/**
 * Live Join Command Types
 *
 * Join a live audio/video call for a room.
 * Creates call if none exists, or joins existing.
 */

import type { CommandParams, CommandResult, CommandInput} from '@system/core/types/JTAGTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { CallEntity, CallParticipant } from '@system/data/entities/CallEntity';
import { Commands } from '../../../../../system/core/shared/Commands';

export interface LiveJoinParams extends CommandParams {
  /**
   * Entity (room/activity) to join live call for (UUID or uniqueId)
   */
  entityId: string;
}

export interface LiveJoinResult extends CommandResult {
  success: boolean;
  message: string;

  /** The call (either found or newly created) */
  session: CallEntity;

  /** Call ID for audio/voice connection */
  callId: UUID;

  /** Whether this was an existing call (true) or newly created (false) */
  existed: boolean;

  /** Current participant list */
  participants: CallParticipant[];

  /** The current user's participant entry */
  myParticipant: CallParticipant;
}

/**
 * LiveJoin — Type-safe command executor
 *
 * Usage:
 *   import { LiveJoin } from '...shared/LiveJoinTypes';
 *   const result = await LiveJoin.execute({ ... });
 */
export const LiveJoin = {
  execute(params: CommandInput<LiveJoinParams>): Promise<LiveJoinResult> {
    return Commands.execute<LiveJoinParams, LiveJoinResult>('collaboration/live/join', params as Partial<LiveJoinParams>);
  },
  commandName: 'collaboration/live/join' as const,
} as const;
