/**
 * Social Trending Command - Shared Types
 *
 * Discover trending and popular content on a social media platform.
 * Shows hot posts, top communities, and rising discussions.
 *
 * Usage:
 *   ./jtag social/trending --platform=moltbook
 *   ./jtag social/trending --platform=moltbook --community=ai-development --sort=top
 *   ./jtag social/trending --platform=moltbook --sort=rising --limit=5
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { SocialPost } from '@system/social/shared/SocialMediaTypes';

/**
 * Social Trending Command Parameters
 */
export interface SocialTrendingParams extends CommandParams {
  /** Platform to browse (e.g., 'moltbook') */
  platform: string;

  /** Sort order: hot (default), top, rising */
  sort?: 'hot' | 'top' | 'rising';

  /** Filter to specific community/submolt */
  community?: string;

  /** Maximum number of posts to return (default: 10) */
  limit?: number;

  /** Persona user ID (auto-detected if not provided) */
  personaId?: UUID;
}

/**
 * Factory function for creating SocialTrendingParams
 */
export const createSocialTrendingParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    platform: string;
    sort?: 'hot' | 'top' | 'rising';
    community?: string;
    limit?: number;
    personaId?: UUID;
  }
): SocialTrendingParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  sort: data.sort ?? undefined,
  community: data.community ?? undefined,
  limit: data.limit ?? 0,
  personaId: data.personaId ?? undefined,
  ...data
});

/**
 * Social Trending Command Result
 */
export interface SocialTrendingResult extends CommandResult {
  success: boolean;
  message: string;

  /** Array of trending posts */
  posts?: SocialPost[];

  error?: JTAGError;
}

/**
 * Factory function for creating SocialTrendingResult with defaults
 */
export const createSocialTrendingResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    message?: string;
    posts?: SocialPost[];
    error?: JTAGError;
  }
): SocialTrendingResult => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  message: data.message ?? '',
  ...data
});

/**
 * Smart Social Trending-specific inheritance from params
 * Auto-inherits context and sessionId from params
 */
export const createSocialTrendingResultFromParams = (
  params: SocialTrendingParams,
  differences: Omit<SocialTrendingResult, 'context' | 'sessionId'>
): SocialTrendingResult => transformPayload(params, differences);

/**
 * SocialTrending — Type-safe command executor
 *
 * Usage:
 *   import { SocialTrending } from '...shared/SocialTrendingTypes';
 *   const result = await SocialTrending.execute({ platform: 'moltbook', sort: 'hot' });
 */
export const SocialTrending = {
  execute(params: CommandInput<SocialTrendingParams>): Promise<SocialTrendingResult> {
    return Commands.execute<SocialTrendingParams, SocialTrendingResult>('social/trending', params as Partial<SocialTrendingParams>);
  },
  commandName: 'social/trending' as const,
} as const;
