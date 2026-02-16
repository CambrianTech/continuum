/**
 * Social Signup Command - Shared Types
 *
 * Register a persona on a social media platform (e.g., Moltbook).
 * Creates an account with a chosen username and stores credentials for future use.
 *
 * Usage:
 *   ./jtag social/signup --platform=moltbook --agentName="helper-ai" --description="I help with code"
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Social Signup Command Parameters
 */
export interface SocialSignupParams extends CommandParams {
  /** Platform to register on (e.g., 'moltbook') */
  platform: string;

  /** Desired username on the platform */
  agentName: string;

  /** Profile description/bio */
  description?: string;

  /** Persona user ID (auto-detected if not provided) */
  personaId?: UUID;

  /** Additional platform-specific metadata */
  metadata?: Record<string, unknown>;
}

/**
 * Factory function for creating SocialSignupParams
 */
export const createSocialSignupParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    platform: string;
    agentName: string;
    description?: string;
    personaId?: UUID;
    metadata?: Record<string, unknown>;
  }
): SocialSignupParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  description: data.description ?? '',
  personaId: data.personaId ?? undefined,
  metadata: data.metadata ?? undefined,
  ...data
});

/**
 * Social Signup Command Result
 */
export interface SocialSignupResult extends CommandResult {
  success: boolean;
  message: string;

  /** API key for future authenticated requests */
  apiKey?: string;

  /** Assigned username on the platform */
  agentName?: string;

  /** URL to claim/verify the account */
  claimUrl?: string;

  /** URL to the agent's profile page */
  profileUrl?: string;

  /** Verification code if applicable */
  verificationCode?: string;

  error?: JTAGError;
}

/**
 * Factory function for creating SocialSignupResult with defaults
 */
export const createSocialSignupResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    message?: string;
    apiKey?: string;
    agentName?: string;
    claimUrl?: string;
    profileUrl?: string;
    verificationCode?: string;
    error?: JTAGError;
  }
): SocialSignupResult => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  message: data.message ?? '',
  ...data
});

/**
 * Smart Social Signup-specific inheritance from params
 * Auto-inherits context and sessionId from params
 */
export const createSocialSignupResultFromParams = (
  params: SocialSignupParams,
  differences: Omit<SocialSignupResult, 'context' | 'sessionId'>
): SocialSignupResult => transformPayload(params, differences);

/**
 * SocialSignup — Type-safe command executor
 *
 * Usage:
 *   import { SocialSignup } from '...shared/SocialSignupTypes';
 *   const result = await SocialSignup.execute({ platform: 'moltbook', agentName: '...' });
 */
export const SocialSignup = {
  execute(params: CommandInput<SocialSignupParams>): Promise<SocialSignupResult> {
    return Commands.execute<SocialSignupParams, SocialSignupResult>('social/signup', params as Partial<SocialSignupParams>);
  },
  commandName: 'social/signup' as const,
} as const;
