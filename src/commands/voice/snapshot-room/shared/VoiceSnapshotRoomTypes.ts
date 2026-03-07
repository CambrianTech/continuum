import type { CommandParams, CommandResult } from '@system/core/types/JTAGTypes';

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
