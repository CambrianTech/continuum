/**
 * Avatar Snapshot Command - Shared Types
 *
 * Capture a Bevy 3D avatar snapshot as PNG for profile pictures. Allocates a temporary render slot, loads the persona's VRM model, waits for a clean frame, encodes as PNG, and saves to ~/.continuum/avatars/. Cached on disk — subsequent calls return immediately unless force=true.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Avatar Snapshot Command Parameters
 */
export interface AvatarSnapshotParams extends CommandParams {
  // Persona identity string (e.g., 'helper', 'teacher'). Used to select avatar model and name the output file.
  identity: string;
  // Render width in pixels (default: 480)
  width?: number;
  // Render height in pixels (default: 480)
  height?: number;
  // Force re-capture even if cached snapshot exists (default: false)
  force?: boolean;
}

/**
 * Factory function for creating AvatarSnapshotParams
 */
export const createAvatarSnapshotParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Persona identity string (e.g., 'helper', 'teacher'). Used to select avatar model and name the output file.
    identity: string;
    // Render width in pixels (default: 480)
    width?: number;
    // Render height in pixels (default: 480)
    height?: number;
    // Force re-capture even if cached snapshot exists (default: false)
    force?: boolean;
  }
): AvatarSnapshotParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  width: data.width ?? 0,
  height: data.height ?? 0,
  force: data.force ?? false,
  ...data
});

/**
 * Avatar Snapshot Command Result
 */
export interface AvatarSnapshotResult extends CommandResult {
  success: boolean;
  // Relative URL path to the avatar PNG (e.g., '/avatars/helper.png')
  path: string;
  // Whether the result came from disk cache (true) or was freshly captured (false)
  cached: boolean;
  error?: JTAGError;
}

/**
 * Factory function for creating AvatarSnapshotResult with defaults
 */
export const createAvatarSnapshotResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Relative URL path to the avatar PNG (e.g., '/avatars/helper.png')
    path?: string;
    // Whether the result came from disk cache (true) or was freshly captured (false)
    cached?: boolean;
    error?: JTAGError;
  }
): AvatarSnapshotResult => createPayload(context, sessionId, {
  path: data.path ?? '',
  cached: data.cached ?? false,
  ...data
});

/**
 * Smart Avatar Snapshot-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createAvatarSnapshotResultFromParams = (
  params: AvatarSnapshotParams,
  differences: Omit<AvatarSnapshotResult, 'context' | 'sessionId' | 'userId'>
): AvatarSnapshotResult => transformPayload(params, differences);

/**
 * Avatar Snapshot — Type-safe command executor
 *
 * Usage:
 *   import { AvatarSnapshot } from '...shared/AvatarSnapshotTypes';
 *   const result = await AvatarSnapshot.execute({ ... });
 */
export const AvatarSnapshot = {
  execute(params: CommandInput<AvatarSnapshotParams>): Promise<AvatarSnapshotResult> {
    return Commands.execute<AvatarSnapshotParams, AvatarSnapshotResult>('avatar/snapshot', params as Partial<AvatarSnapshotParams>);
  },
  commandName: 'avatar/snapshot' as const,
} as const;
