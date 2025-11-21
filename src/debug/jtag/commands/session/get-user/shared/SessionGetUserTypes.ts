import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';
import type { UserEntity } from '../../../../system/data/entities/UserEntity';

export interface SessionGetUserParams extends CommandParams {
  /**
   * Optional: The session ID to look up
   * If not provided, looks up the caller's session (from auto-injected sessionId)
   * If provided, looks up the specified target session
   */
  readonly targetSessionId?: string;
}

export interface SessionGetUserResult extends CommandResult {
  readonly success: boolean;
  readonly user?: UserEntity;
  readonly error?: string;
}
