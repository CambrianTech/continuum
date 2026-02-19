/**
 * Social Post Command - Shared Types
 *
 * Create a post on a social media platform using the persona's stored credentials.
 *
 * Usage:
 *   ./jtag social/post --platform=moltbook --title="Hello" --content="First post" --community=general
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { SocialPost as SocialPostData } from '@system/social/shared/SocialMediaTypes';

/**
 * Social Post Command Parameters
 */
export interface SocialPostParams extends CommandParams {
  /** Platform to post on (e.g., 'moltbook') */
  platform: string;

  /** Post title */
  title: string;

  /** Post content/body */
  content: string;

  /** Community/submolt to post in (optional) */
  community?: string;

  /** URL for link posts (optional) */
  url?: string;

  /** Persona user ID (auto-detected if not provided) */
  personaId?: UUID;
}

/**
 * Factory function for creating SocialPostParams
 */
export const createSocialPostParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    platform: string;
    title: string;
    content: string;
    community?: string;
    url?: string;
    personaId?: UUID;
  }
): SocialPostParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  community: data.community ?? '',
  url: data.url ?? '',
  personaId: data.personaId ?? undefined,
  ...data
});

/**
 * Social Post Command Result
 */
export interface SocialPostResult extends CommandResult {
  success: boolean;
  message: string;

  /** Created post details */
  post?: SocialPostData;

  error?: JTAGError;
}

/**
 * Factory function for creating SocialPostResult with defaults
 */
export const createSocialPostResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    message?: string;
    post?: SocialPostData;
    error?: JTAGError;
  }
): SocialPostResult => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  message: data.message ?? '',
  ...data
});

/**
 * Smart Social Post-specific inheritance from params
 * Auto-inherits context and sessionId from params
 */
export const createSocialPostResultFromParams = (
  params: SocialPostParams,
  differences: Omit<SocialPostResult, 'context' | 'sessionId'>
): SocialPostResult => transformPayload(params, differences);

/**
 * SocialPost — Type-safe command executor
 *
 * Usage:
 *   import { SocialPost } from '...shared/SocialPostTypes';
 *   const result = await SocialPost.execute({ platform: 'moltbook', title: '...', content: '...' });
 */
export const SocialPost = {
  execute(params: CommandInput<SocialPostParams>): Promise<SocialPostResult> {
    return Commands.execute<SocialPostParams, SocialPostResult>('social/post', params as Partial<SocialPostParams>);
  },
  commandName: 'social/post' as const,
} as const;
