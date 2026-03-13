import type { CommandParams, CommandResult } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { CommandInput } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

export interface VoiceSnapshotRoomParams extends CommandParams {
}

export interface VoiceSnapshotRoomResult extends CommandResult {
  success: boolean;
  base64?: string;
  mimeType?: string;
  width?: number;
  height?: number;
  participants?: string;
  hash?: string;
  capturedAt?: number;
  error?: string;
}

/**
 * VoiceSnapshotRoom — Type-safe command executor
 *
 * Usage:
 *   import { VoiceSnapshotRoom } from '...shared/VoiceSnapshotRoomTypes';
 *   const result = await VoiceSnapshotRoom.execute({ ... });
 */
export const VoiceSnapshotRoom = {
  execute(params: CommandInput<VoiceSnapshotRoomParams>): Promise<VoiceSnapshotRoomResult> {
    return Commands.execute<VoiceSnapshotRoomParams, VoiceSnapshotRoomResult>('voice/snapshot-room', params as Partial<VoiceSnapshotRoomParams>);
  },
  commandName: 'voice/snapshot-room' as const,
} as const;

/**
 * Factory function for creating VoiceSnapshotRoomParams
 */
export const createVoiceSnapshotRoomParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<VoiceSnapshotRoomParams, 'context' | 'sessionId' | 'userId'>
): VoiceSnapshotRoomParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Factory function for creating VoiceSnapshotRoomResult with defaults
 */
export const createVoiceSnapshotRoomResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<VoiceSnapshotRoomResult, 'context' | 'sessionId' | 'userId'>
): VoiceSnapshotRoomResult => createPayload(context, sessionId, {
  ...data
});

/**
 * Smart voice/snapshot-room-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createVoiceSnapshotRoomResultFromParams = (
  params: VoiceSnapshotRoomParams,
  differences: Omit<VoiceSnapshotRoomResult, 'context' | 'sessionId' | 'userId'>
): VoiceSnapshotRoomResult => transformPayload(params, differences);

