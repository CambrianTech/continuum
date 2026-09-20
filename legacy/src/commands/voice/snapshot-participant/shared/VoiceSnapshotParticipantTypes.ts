import type { CommandParams, CommandResult } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { CommandInput } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

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

/**
 * Factory function for creating VoiceSnapshotParticipantParams
 */
export const createVoiceSnapshotParticipantParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<VoiceSnapshotParticipantParams, 'context' | 'sessionId' | 'userId'>
): VoiceSnapshotParticipantParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Factory function for creating VoiceSnapshotParticipantResult with defaults
 */
export const createVoiceSnapshotParticipantResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<VoiceSnapshotParticipantResult, 'context' | 'sessionId' | 'userId'>
): VoiceSnapshotParticipantResult => createPayload(context, sessionId, {
  ...data
});

/**
 * Smart voice/snapshot-participant-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createVoiceSnapshotParticipantResultFromParams = (
  params: VoiceSnapshotParticipantParams,
  differences: Omit<VoiceSnapshotParticipantResult, 'context' | 'sessionId' | 'userId'>
): VoiceSnapshotParticipantResult => transformPayload(params, differences);

