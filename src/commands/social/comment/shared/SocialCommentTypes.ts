/**
 * Social Comment Command - Shared Types
 *
 * Comment on a post or reply to a comment on a social media platform.
 * Supports threaded replies.
 *
 * Usage:
 *   ./jtag social/comment --platform=moltbook --postId=abc123 --content="Great insight!"
 *   ./jtag social/comment --platform=moltbook --postId=abc123 --content="Agreed" --parentId=def456
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { SocialComment as SocialCommentData } from '@system/social/shared/SocialMediaTypes';

/**
 * Social Comment Command Parameters
 */
export interface SocialCommentParams extends CommandParams {
  /** Platform (e.g., 'moltbook') */
  platform: string;

  /** Post ID to comment on or list comments from */
  postId: string;

  /** Action: 'create' to post a comment, 'list' to read comments (default: 'create') */
  action?: 'create' | 'list';

  /** Comment text (required for action=create) */
  content?: string;

  /** Parent comment ID for threaded replies (optional, action=create only) */
  parentId?: string;

  /** Sort order for listing comments (action=list only) */
  sort?: string;

  /** Persona user ID (auto-detected if not provided) */
  personaId?: UUID;
}

/**
 * Factory function for creating SocialCommentParams
 */
export const createSocialCommentParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    platform: string;
    postId: string;
    content: string;
    parentId?: string;
    personaId?: UUID;
  }
): SocialCommentParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  parentId: data.parentId ?? '',
  personaId: data.personaId ?? undefined,
  ...data
});

/**
 * Social Comment Command Result
 */
export interface SocialCommentResult extends CommandResult {
  success: boolean;
  message: string;

  /** Created comment (action=create) */
  comment?: SocialCommentData;

  /** Listed comments (action=list) */
  comments?: SocialCommentData[];

  error?: JTAGError;
}

/**
 * Factory function for creating SocialCommentResult with defaults
 */
export const createSocialCommentResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    message?: string;
    comment?: SocialCommentData;
    error?: JTAGError;
  }
): SocialCommentResult => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  message: data.message ?? '',
  ...data
});

/**
 * Smart Social Comment-specific inheritance from params
 * Auto-inherits context and sessionId from params
 */
export const createSocialCommentResultFromParams = (
  params: SocialCommentParams,
  differences: Omit<SocialCommentResult, 'context' | 'sessionId'>
): SocialCommentResult => transformPayload(params, differences);

/**
 * SocialComment — Type-safe command executor
 *
 * Usage:
 *   import { SocialComment } from '...shared/SocialCommentTypes';
 *   const result = await SocialComment.execute({ platform: 'moltbook', postId: '...', content: '...' });
 */
export const SocialComment = {
  execute(params: CommandInput<SocialCommentParams>): Promise<SocialCommentResult> {
    return Commands.execute<SocialCommentParams, SocialCommentResult>('social/comment', params as Partial<SocialCommentParams>);
  },
  commandName: 'social/comment' as const,
} as const;
