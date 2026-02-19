/**
 * Social Profile Command - Shared Types
 *
 * View or update a social media profile. View your own profile, another agent's profile, or update your bio/description.
 *
 * Usage:
 *   ./jtag social/profile --platform=moltbook
 *   ./jtag social/profile --platform=moltbook --agentName=other-agent
 *   ./jtag social/profile --platform=moltbook --update --description="New bio"
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { SocialProfile as SocialProfileData } from '@system/social/shared/SocialMediaTypes';

/**
 * Social Profile Command Parameters
 */
export interface SocialProfileParams extends CommandParams {
  /** Platform to query (e.g., 'moltbook') */
  platform: string;

  /** Agent name to look up (omit for own profile) */
  agentName?: string;

  /** If true, update own profile instead of viewing */
  update?: boolean;

  /** New profile description/bio (requires --update) */
  description?: string;

  /** Persona user ID (auto-detected if not provided) */
  personaId?: UUID;
}

/**
 * Factory function for creating SocialProfileParams
 */
export const createSocialProfileParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    platform: string;
    agentName?: string;
    update?: boolean;
    description?: string;
    personaId?: UUID;
  }
): SocialProfileParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  agentName: data.agentName ?? undefined,
  update: data.update ?? false,
  description: data.description ?? undefined,
  personaId: data.personaId ?? undefined,
  ...data
});

/**
 * Social Profile Command Result
 */
export interface SocialProfileResult extends CommandResult {
  success: boolean;
  message: string;

  /** The profile data (when viewing) */
  profile?: SocialProfileData;

  /** Whether profile was updated (when updating) */
  updated?: boolean;

  error?: JTAGError;
}

/**
 * Factory function for creating SocialProfileResult with defaults
 */
export const createSocialProfileResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    message?: string;
    profile?: SocialProfileData;
    updated?: boolean;
    error?: JTAGError;
  }
): SocialProfileResult => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  message: data.message ?? '',
  ...data
});

/**
 * Smart Social Profile-specific inheritance from params
 * Auto-inherits context and sessionId from params
 */
export const createSocialProfileResultFromParams = (
  params: SocialProfileParams,
  differences: Omit<SocialProfileResult, 'context' | 'sessionId'>
): SocialProfileResult => transformPayload(params, differences);

/**
 * SocialProfile — Type-safe command executor
 *
 * Usage:
 *   import { SocialProfile } from '...shared/SocialProfileTypes';
 *   const result = await SocialProfile.execute({ platform: 'moltbook' });
 */
export const SocialProfile = {
  execute(params: CommandInput<SocialProfileParams>): Promise<SocialProfileResult> {
    return Commands.execute<SocialProfileParams, SocialProfileResult>('social/profile', params as Partial<SocialProfileParams>);
  },
  commandName: 'social/profile' as const,
} as const;
