/**
 * Collaboration Live Start Command - Shared Types
 *
 * Start a live call with selected participants. Creates or finds the DM room for the participant set, then joins the call. Like Discord's group call - select users, click call.
 */

import type { CommandParams, CommandResult, JTAGContext, CommandInput} from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { RoomEntity } from '@system/data/entities/RoomEntity';
import type { CallEntity, CallParticipant } from '@system/data/entities/CallEntity';
import { Commands } from '../../../../../system/core/shared/Commands';

/**
 * Collaboration Live Start Command Parameters
 */
export interface CollaborationLiveStartParams extends CommandParams {
  // User IDs or uniqueIds to include (current user auto-added)
  participants: string | string[];
  // Optional room name (defaults to participant names)
  name?: string;
  // Start with video enabled (default: false, audio only)
  withVideo?: boolean;
}

/**
 * Factory function for creating CollaborationLiveStartParams
 */
export const createCollaborationLiveStartParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // User IDs or uniqueIds to include (current user auto-added)
    participants: string | string[];
    // Optional room name (defaults to participant names)
    name?: string;
    // Start with video enabled (default: false, audio only)
    withVideo?: boolean;
  }
): CollaborationLiveStartParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  name: data.name ?? '',
  withVideo: data.withVideo ?? false,
  ...data
});

/**
 * Collaboration Live Start Command Result
 */
export interface CollaborationLiveStartResult extends CommandResult {
  success: boolean;
  // Human-readable message
  message?: string;
  // The DM room ID (created or found)
  roomId: UUID;
  // The call ID (named liveSessionId for backwards compat - represents callId)
  liveSessionId: UUID;
  // The full room entity
  room: RoomEntity;
  // The call with participants
  session: CallEntity;
  // Whether room already existed (true) or was created (false)
  existed: boolean;
  // Current participants in the call
  participants: CallParticipant[];
  error?: JTAGError;
}

/**
 * Factory function for creating CollaborationLiveStartResult with defaults
 */
export const createCollaborationLiveStartResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // The DM room ID (created or found)
    roomId?: UUID;
    // The call ID
    liveSessionId?: UUID;
    // The full room entity
    room?: RoomEntity;
    // The call with participants
    session?: CallEntity;
    // Whether room already existed (true) or was created (false)
    existed?: boolean;
    // Current participants in the call
    participants?: CallParticipant[];
    error?: JTAGError;
  }
): CollaborationLiveStartResult => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  roomId: data.roomId!,
  liveSessionId: data.liveSessionId!,
  room: data.room!,
  session: data.session!,
  existed: data.existed ?? false,
  participants: data.participants ?? [],
  ...data
});

/**
 * Smart Collaboration Live Start-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createCollaborationLiveStartResultFromParams = (
  params: CollaborationLiveStartParams,
  differences: Omit<CollaborationLiveStartResult, 'context' | 'sessionId'>
): CollaborationLiveStartResult => transformPayload(params, differences);

/**
 * CollaborationLiveStart — Type-safe command executor
 *
 * Usage:
 *   import { CollaborationLiveStart } from '...shared/CollaborationLiveStartTypes';
 *   const result = await CollaborationLiveStart.execute({ ... });
 */
export const CollaborationLiveStart = {
  execute(params: CommandInput<CollaborationLiveStartParams>): Promise<CollaborationLiveStartResult> {
    return Commands.execute<CollaborationLiveStartParams, CollaborationLiveStartResult>('collaboration/live/start', params as Partial<CollaborationLiveStartParams>);
  },
  commandName: 'collaboration/live/start' as const,
} as const;
