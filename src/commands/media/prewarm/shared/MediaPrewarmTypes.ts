/**
 * Media Prewarm Command - Shared Types
 *
 * Pre-warm vision description cache for image media. Fires VisionDescriptionService.describeBase64() so that by the time personas build RAG context, descriptions are cached. Called fire-and-forget by chat/send when images are attached.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/** Image data for pre-warming vision descriptions */
export interface PrewarmImageInput {
  base64: string;
  mimeType: string;
}

/**
 * Media Prewarm Command Parameters
 */
export interface MediaPrewarmParams extends CommandParams {
  images: PrewarmImageInput[];
}

/**
 * Factory function for creating MediaPrewarmParams
 */
export const createMediaPrewarmParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Array of image objects with base64 and mimeType fields
    images: PrewarmImageInput[];
  }
): MediaPrewarmParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,

  ...data
});

/**
 * Media Prewarm Command Result
 */
export interface MediaPrewarmResult extends CommandResult {
  success: boolean;
  // Number of images queued for description generation
  queued: number;
  error?: JTAGError;
}

/**
 * Factory function for creating MediaPrewarmResult with defaults
 */
export const createMediaPrewarmResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Number of images queued for description generation
    queued?: number;
    error?: JTAGError;
  }
): MediaPrewarmResult => createPayload(context, sessionId, {
  queued: data.queued ?? 0,
  ...data
});

/**
 * Smart Media Prewarm-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createMediaPrewarmResultFromParams = (
  params: MediaPrewarmParams,
  differences: Omit<MediaPrewarmResult, 'context' | 'sessionId' | 'userId'>
): MediaPrewarmResult => transformPayload(params, differences);

/**
 * Media Prewarm — Type-safe command executor
 *
 * Usage:
 *   import { MediaPrewarm } from '...shared/MediaPrewarmTypes';
 *   const result = await MediaPrewarm.execute({ ... });
 */
export const MediaPrewarm = {
  execute(params: CommandInput<MediaPrewarmParams>): Promise<MediaPrewarmResult> {
    return Commands.execute<MediaPrewarmParams, MediaPrewarmResult>('media/prewarm', params as Partial<MediaPrewarmParams>);
  },
  commandName: 'media/prewarm' as const,
} as const;
