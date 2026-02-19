/**
 * Social Engage Command - Shared Types
 *
 * All social interaction in one command: vote, follow, subscribe.
 * Designed for AI tool use — one command covers all engagement actions.
 *
 * Actions:
 *   vote        — Upvote or downvote a post or comment
 *   follow      — Follow an agent
 *   unfollow    — Unfollow an agent
 *   subscribe   — Subscribe to a community
 *   unsubscribe — Unsubscribe from a community
 *   delete      — Delete own post or comment
 *
 * Usage:
 *   ./jtag social/engage --platform=moltbook --action=vote --target=abc123 --targetType=post --direction=up
 *   ./jtag social/engage --platform=moltbook --action=follow --target=eudaemon_0
 *   ./jtag social/engage --platform=moltbook --action=subscribe --target=ai-development
 *   ./jtag social/engage --platform=moltbook --action=delete --target=abc123 --targetType=post
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/** Engagement actions */
export type EngageAction = 'vote' | 'follow' | 'unfollow' | 'subscribe' | 'unsubscribe' | 'delete';

/**
 * Social Engage Command Parameters
 */
export interface SocialEngageParams extends CommandParams {
  /** Platform (e.g., 'moltbook') */
  platform: string;

  /** Engagement action */
  action: EngageAction;

  /**
   * Target identifier — meaning depends on action:
   *   vote        → post or comment ID
   *   follow      → agent username
   *   unfollow    → agent username
   *   subscribe   → community/submolt name
   *   unsubscribe → community/submolt name
   */
  target: string;

  /** For vote action: target type */
  targetType?: 'post' | 'comment';

  /** For vote action: direction */
  direction?: 'up' | 'down';

  /** Persona user ID (auto-detected if not provided) */
  personaId?: UUID;
}

/**
 * Social Engage Command Result
 */
export interface SocialEngageResult extends CommandResult {
  success: boolean;
  message: string;
  action: EngageAction;
  target: string;
  error?: JTAGError;
}

export const createSocialEngageParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<SocialEngageParams, 'context' | 'sessionId'>
): SocialEngageParams => createPayload(context, sessionId, data);

export const createSocialEngageResultFromParams = (
  params: SocialEngageParams,
  differences: Omit<SocialEngageResult, 'context' | 'sessionId'>
): SocialEngageResult => transformPayload(params, differences);

/**
 * SocialEngage — Type-safe command executor
 */
export const SocialEngage = {
  execute(params: CommandInput<SocialEngageParams>): Promise<SocialEngageResult> {
    return Commands.execute<SocialEngageParams, SocialEngageResult>('social/engage', params as Partial<SocialEngageParams>);
  },
  commandName: 'social/engage' as const,
} as const;
