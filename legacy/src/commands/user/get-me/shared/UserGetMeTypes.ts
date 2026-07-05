import type { CommandParams, CommandResult, CommandInput} from '../../../../system/core/types/JTAGTypes';
import type { UserEntity } from '../../../../system/data/entities/UserEntity';
import { Commands } from '../../../../system/core/shared/Commands';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * User Get Me Command - Get current user info
 *
 * Convenience command that returns the caller's full user information.
 * No parameters needed - sessionId is auto-injected and used to look up user.
 */

export interface UserGetMeParams extends CommandParams {
  // No additional parameters - uses auto-injected sessionId
}

export interface UserGetMeResult extends CommandResult {
  readonly success: boolean;
  readonly user?: UserEntity;
  readonly error?: string;
}

/**
 * UserGetMe — Type-safe command executor
 *
 * Usage:
 *   import { UserGetMe } from '...shared/UserGetMeTypes';
 *   const result = await UserGetMe.execute({ ... });
 */
export const UserGetMe = {
  execute(params: CommandInput<UserGetMeParams>): Promise<UserGetMeResult> {
    return Commands.execute<UserGetMeParams, UserGetMeResult>('user/get-me', params as Partial<UserGetMeParams>);
  },
  commandName: 'user/get-me' as const,
} as const;

/**
 * Factory function for creating UserGetMeParams
 */
export const createUserGetMeParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<UserGetMeParams, 'context' | 'sessionId' | 'userId'>
): UserGetMeParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Factory function for creating UserGetMeResult with defaults
 */
export const createUserGetMeResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<UserGetMeResult, 'context' | 'sessionId' | 'userId'>
): UserGetMeResult => createPayload(context, sessionId, {
  ...data
});

/**
 * Smart user/get-me-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createUserGetMeResultFromParams = (
  params: UserGetMeParams,
  differences: Omit<UserGetMeResult, 'context' | 'sessionId' | 'userId'>
): UserGetMeResult => transformPayload(params, differences);

