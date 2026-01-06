/**
 * DM Command Types
 * Get or create a private room with specific participants
 *
 * Set theory: {A, B} == {B, A} - participant order doesn't matter
 * Works with any number of participants (2+ for group DM)
 */

import type { CommandParams, CommandResult } from '@system/core/types/JTAGTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { RoomEntity } from '@system/data/entities/RoomEntity';

export interface DmParams extends CommandParams {
  /**
   * Participants in the DM (user IDs or uniqueIds)
   * Current user is auto-added, so pass the OTHER participant(s)
   * Can be single ID string or array for group DM
   */
  participants: string | string[];

  /** Optional room name (can be set later) */
  name?: string;
}

export interface DmResult extends CommandResult {
  success: boolean;
  message: string;

  /** The DM room (either found or newly created) */
  room: RoomEntity;

  /** Room ID for quick reference */
  roomId: UUID;

  /** Whether this was an existing room (true) or newly created (false) */
  existed: boolean;

  /** The deterministic uniqueId for this DM (based on sorted participant set) */
  uniqueId: string;

  /** All participants in the room */
  participantIds: UUID[];
}
