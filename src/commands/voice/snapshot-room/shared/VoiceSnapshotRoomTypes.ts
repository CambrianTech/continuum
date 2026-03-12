import type { CommandParams, CommandResult } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { CommandInput } from '@system/core/types/JTAGTypes';

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
