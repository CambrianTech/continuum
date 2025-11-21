import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';
import type { UserEntity } from '../../../../system/data/entities/UserEntity';

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
