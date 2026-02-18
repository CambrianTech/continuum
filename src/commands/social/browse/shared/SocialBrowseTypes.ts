/**
 * Social Browse Command - Shared Types
 *
 * Intelligent exploration of social media platforms.
 * One command for all discovery: communities, feeds, posts, agents.
 *
 * Modes:
 *   discover   — List all communities with descriptions and activity
 *   community  — Browse a specific community's feed with context
 *   post       — Read a full post with threaded comments and author info
 *   agent      — View an agent's profile, karma, recent activity
 *   trending   — Hot posts across the platform (default)
 *
 * Usage:
 *   ./jtag social/browse --platform=moltbook                            # trending
 *   ./jtag social/browse --platform=moltbook --mode=discover            # list communities
 *   ./jtag social/browse --platform=moltbook --mode=community --target=ai-development
 *   ./jtag social/browse --platform=moltbook --mode=post --target=abc123
 *   ./jtag social/browse --platform=moltbook --mode=agent --target=eudaemon_0
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type {
  SocialPost as SocialPostData,
  SocialComment as SocialCommentData,
  SocialProfile as SocialProfileData,
  SocialCommunity as SocialCommunityData,
} from '@system/social/shared/SocialMediaTypes';

/** Browse modes */
export type BrowseMode = 'trending' | 'discover' | 'community' | 'post' | 'agent';

/**
 * Social Browse Command Parameters
 */
export interface SocialBrowseParams extends CommandParams {
  /** Platform to browse (e.g., 'moltbook') */
  platform: string;

  /** Browse mode (default: 'trending') */
  mode?: BrowseMode;

  /**
   * Target identifier — meaning depends on mode:
   *   community → community/submolt name
   *   post      → post ID
   *   agent     → agent username
   */
  target?: string;

  /** Sort order for feeds: hot, new, top, rising */
  sort?: 'hot' | 'new' | 'top' | 'rising';

  /** Max items to return */
  limit?: number;

  /** Persona user ID (auto-detected if not provided) */
  personaId?: UUID;
}

/**
 * Social Browse Command Result
 *
 * Returns different data depending on mode, but always includes
 * a human-readable summary for AI consumption.
 */
export interface SocialBrowseResult extends CommandResult {
  success: boolean;
  message: string;
  mode: BrowseMode;

  /** Rendered summary — AI-friendly overview of what was found */
  summary: string;

  /** Communities (mode=discover) */
  communities?: SocialCommunityData[];

  /** Posts (mode=trending, community) */
  posts?: SocialPostData[];

  /** Single post detail (mode=post) */
  post?: SocialPostData;

  /** Comment thread (mode=post) */
  comments?: SocialCommentData[];

  /** Agent profile (mode=agent) */
  profile?: SocialProfileData;

  error?: JTAGError;
}

export const createSocialBrowseParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<SocialBrowseParams, 'context' | 'sessionId'>
): SocialBrowseParams => createPayload(context, sessionId, data);

export const createSocialBrowseResultFromParams = (
  params: SocialBrowseParams,
  differences: Omit<SocialBrowseResult, 'context' | 'sessionId'>
): SocialBrowseResult => transformPayload(params, differences);

/**
 * SocialBrowse — Type-safe command executor
 */
export const SocialBrowse = {
  execute(params: CommandInput<SocialBrowseParams>): Promise<SocialBrowseResult> {
    return Commands.execute<SocialBrowseParams, SocialBrowseResult>('social/browse', params as Partial<SocialBrowseParams>);
  },
  commandName: 'social/browse' as const,
} as const;
