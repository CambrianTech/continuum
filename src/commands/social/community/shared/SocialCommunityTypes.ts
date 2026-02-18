/**
 * Social Community Command - Shared Types
 *
 * Manage communities (submolts) — create, list, subscribe, unsubscribe, get info
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { SocialCommunity as SocialCommunityData } from '@system/social/shared/SocialMediaTypes';

export type CommunityAction = 'list' | 'info' | 'create' | 'subscribe' | 'unsubscribe';

export interface SocialCommunityParams extends CommandParams {
  /** Platform (e.g., 'moltbook') */
  platform: string;
  /** Action: list, info, create, subscribe, unsubscribe */
  action: CommunityAction;
  /** Community name (required for info, create, subscribe, unsubscribe) */
  name?: string;
  /** Community description (for create) */
  description?: string;
  /** Persona user ID (auto-detected) */
  personaId?: UUID;
}

export interface SocialCommunityResult extends CommandResult {
  success: boolean;
  message: string;
  summary?: string;
  /** List of communities (for list action) */
  communities?: SocialCommunityData[];
  /** Community info (for info/create actions) */
  community?: SocialCommunityData;
  error?: JTAGError;
}

export const createSocialCommunityParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<SocialCommunityParams, 'context' | 'sessionId'>
): SocialCommunityParams => createPayload(context, sessionId, data);

export const createSocialCommunityResultFromParams = (
  params: SocialCommunityParams,
  differences: Omit<SocialCommunityResult, 'context' | 'sessionId'>
): SocialCommunityResult => transformPayload(params, differences);

export const SocialCommunity = {
  execute(params: CommandInput<SocialCommunityParams>): Promise<SocialCommunityResult> {
    return Commands.execute<SocialCommunityParams, SocialCommunityResult>('social/community', params as Partial<SocialCommunityParams>);
  },
  commandName: 'social/community' as const,
} as const;
