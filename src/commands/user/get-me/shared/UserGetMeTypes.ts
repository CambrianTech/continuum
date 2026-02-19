import type { CommandParams, CommandResult, CommandInput} from '../../../../system/core/types/JTAGTypes';
import type { UserEntity } from '../../../../system/data/entities/UserEntity';
import { Commands } from '../../../../system/core/shared/Commands';

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
