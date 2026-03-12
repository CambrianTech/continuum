import type { CommandParams, CommandResult } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { CommandInput } from '@system/core/types/JTAGTypes';

export interface VoiceSnapshotParticipantParams extends CommandParams {
  identity: string;
}

export interface VoiceSnapshotParticipantResult extends CommandResult {
  success: boolean;
  base64?: string;
  mimeType?: string;
  width?: number;
  height?: number;
  identity?: string;
  displayName?: string;
  hash?: string;
  capturedAt?: number;
  error?: string;
}

/**
 * VoiceSnapshotParticipant — Type-safe command executor
 *
 * Usage:
 *   import { VoiceSnapshotParticipant } from '...shared/VoiceSnapshotParticipantTypes';
 *   const result = await VoiceSnapshotParticipant.execute({ ... });
 */
export const VoiceSnapshotParticipant = {
  execute(params: CommandInput<VoiceSnapshotParticipantParams>): Promise<VoiceSnapshotParticipantResult> {
    return Commands.execute<VoiceSnapshotParticipantParams, VoiceSnapshotParticipantResult>('voice/snapshot-participant', params as Partial<VoiceSnapshotParticipantParams>);
  },
  commandName: 'voice/snapshot-participant' as const,
} as const;
