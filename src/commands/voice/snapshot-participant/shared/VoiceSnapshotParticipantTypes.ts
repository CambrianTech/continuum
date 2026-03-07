import type { CommandParams, CommandResult } from '@system/core/types/JTAGTypes';

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
