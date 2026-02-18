/**
 * Social Feed Command - Shared Types
 *
 * Read the feed from a social media platform. Supports global feed,
 * personalized feed, and community-specific feeds.
 *
 * Usage:
 *   ./jtag social/feed --platform=moltbook --sort=hot --limit=10
 *   ./jtag social/feed --platform=moltbook --community=ai-development --sort=new
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { SocialPost as SocialPostData } from '@system/social/shared/SocialMediaTypes';

/**
 * Social Feed Command Parameters
 */
export interface SocialFeedParams extends CommandParams {
  /** Platform to read from (e.g., 'moltbook') */
  platform: string;

  /** Sort order: hot, new, top, rising */
  sort?: 'hot' | 'new' | 'top' | 'rising';

  /** Community/submolt to filter by */
  community?: string;

  /** Maximum number of posts to return */
  limit?: number;

  /** Whether to show personalized feed */
  personalized?: boolean;

  /** Persona user ID (auto-detected if not provided) */
  personaId?: UUID;
}

/**
 * Factory function for creating SocialFeedParams
 */
export const createSocialFeedParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    platform: string;
    sort?: 'hot' | 'new' | 'top' | 'rising';
    community?: string;
    limit?: number;
    personalized?: boolean;
    personaId?: UUID;
  }
): SocialFeedParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  sort: data.sort ?? undefined,
  community: data.community ?? '',
  limit: data.limit ?? 0,
  personalized: data.personalized ?? false,
  personaId: data.personaId ?? undefined,
  ...data
});

/**
 * Social Feed Command Result
 */
export interface SocialFeedResult extends CommandResult {
  success: boolean;
  message: string;

  /** Array of feed posts */
  posts?: SocialPostData[];

  error?: JTAGError;
}

/**
 * Factory function for creating SocialFeedResult with defaults
 */
export const createSocialFeedResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    message?: string;
    posts?: SocialPostData[];
    error?: JTAGError;
  }
): SocialFeedResult => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  message: data.message ?? '',
  ...data
});

/**
 * Smart Social Feed-specific inheritance from params
 * Auto-inherits context and sessionId from params
 */
export const createSocialFeedResultFromParams = (
  params: SocialFeedParams,
  differences: Omit<SocialFeedResult, 'context' | 'sessionId'>
): SocialFeedResult => transformPayload(params, differences);

/**
 * SocialFeed — Type-safe command executor
 *
 * Usage:
 *   import { SocialFeed } from '...shared/SocialFeedTypes';
 *   const result = await SocialFeed.execute({ platform: 'moltbook', sort: 'hot' });
 */
export const SocialFeed = {
  execute(params: CommandInput<SocialFeedParams>): Promise<SocialFeedResult> {
    return Commands.execute<SocialFeedParams, SocialFeedResult>('social/feed', params as Partial<SocialFeedParams>);
  },
  commandName: 'social/feed' as const,
} as const;
