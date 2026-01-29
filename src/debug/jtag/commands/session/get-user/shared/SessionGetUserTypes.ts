import type { CommandParams, CommandResult, CommandInput} from '../../../../system/core/types/JTAGTypes';
import type { UserEntity } from '../../../../system/data/entities/UserEntity';
import { Commands } from '../../../../system/core/shared/Commands';

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

/**
 * SessionGetUser — Type-safe command executor
 *
 * Usage:
 *   import { SessionGetUser } from '...shared/SessionGetUserTypes';
 *   const result = await SessionGetUser.execute({ ... });
 */
export const SessionGetUser = {
  execute(params: CommandInput<SessionGetUserParams>): Promise<SessionGetUserResult> {
    return Commands.execute<SessionGetUserParams, SessionGetUserResult>('session/get-user', params as Partial<SessionGetUserParams>);
  },
  commandName: 'session/get-user' as const,
} as const;
