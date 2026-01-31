/**
 * Social Search Command - Shared Types
 *
 * Semantic search across social media platforms.
 * Find posts, agents, and communities by keyword.
 *
 * Usage:
 *   ./jtag social/search --platform=moltbook --query="memory systems"
 *   ./jtag social/search --platform=moltbook --query="rust concurrency" --type=post --limit=10
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { SocialPost as SocialPostData } from '@system/social/shared/SocialMediaTypes';

/**
 * Social Search Command Parameters
 */
export interface SocialSearchParams extends CommandParams {
  /** Platform to search (e.g., 'moltbook') */
  platform: string;

  /** Search query */
  query: string;

  /** Filter by type: post, comment, agent, submolt */
  type?: 'post' | 'comment' | 'agent' | 'submolt';

  /** Max results */
  limit?: number;

  /** Persona user ID (auto-detected if not provided) */
  personaId?: UUID;
}

/**
 * Social Search Command Result
 */
export interface SocialSearchResult extends CommandResult {
  success: boolean;
  message: string;

  /** AI-friendly summary of results */
  summary: string;

  /** Search results */
  posts?: SocialPostData[];

  /** Total matching results (may exceed returned count) */
  totalCount?: number;

  error?: JTAGError;
}

export const createSocialSearchParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<SocialSearchParams, 'context' | 'sessionId'>
): SocialSearchParams => createPayload(context, sessionId, data);

export const createSocialSearchResultFromParams = (
  params: SocialSearchParams,
  differences: Omit<SocialSearchResult, 'context' | 'sessionId'>
): SocialSearchResult => transformPayload(params, differences);

/**
 * SocialSearch — Type-safe command executor
 */
export const SocialSearch = {
  execute(params: CommandInput<SocialSearchParams>): Promise<SocialSearchResult> {
    return Commands.execute<SocialSearchParams, SocialSearchResult>('social/search', params as Partial<SocialSearchParams>);
  },
  commandName: 'social/search' as const,
} as const;
