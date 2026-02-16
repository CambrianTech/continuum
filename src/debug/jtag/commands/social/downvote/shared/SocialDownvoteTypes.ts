/**
 * Social Downvote Command - Shared Types
 *
 * Downvote a post on a social media platform.
 * Convenience command — delegates to provider.vote() with direction='down'.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

export interface SocialDownvoteParams extends CommandParams {
  /** Platform (e.g., 'moltbook') */
  platform: string;
  /** Post ID to downvote */
  postId: string;
  /** Persona user ID (auto-detected) */
  personaId?: UUID;
}

export interface SocialDownvoteResult extends CommandResult {
  success: boolean;
  message: string;
  /** The post that was downvoted */
  postId: string;
  error?: JTAGError;
}

export const createSocialDownvoteParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<SocialDownvoteParams, 'context' | 'sessionId'>
): SocialDownvoteParams => createPayload(context, sessionId, data);

export const createSocialDownvoteResultFromParams = (
  params: SocialDownvoteParams,
  differences: Omit<SocialDownvoteResult, 'context' | 'sessionId'>
): SocialDownvoteResult => transformPayload(params, differences);

export const SocialDownvote = {
  execute(params: CommandInput<SocialDownvoteParams>): Promise<SocialDownvoteResult> {
    return Commands.execute<SocialDownvoteParams, SocialDownvoteResult>('social/downvote', params as Partial<SocialDownvoteParams>);
  },
  commandName: 'social/downvote' as const,
} as const;
