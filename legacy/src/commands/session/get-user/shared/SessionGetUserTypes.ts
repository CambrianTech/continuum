import type { CommandParams, CommandResult, CommandInput} from '../../../../system/core/types/JTAGTypes';
import type { UserEntity } from '../../../../system/data/entities/UserEntity';
import { Commands } from '../../../../system/core/shared/Commands';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/** Resolve a session to its owning UserEntity, defaulting to the caller's session or targeting a specific one. */
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

/**
 * Factory function for creating SessionGetUserParams
 */
export const createSessionGetUserParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<SessionGetUserParams, 'context' | 'sessionId' | 'userId'>
): SessionGetUserParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Factory function for creating SessionGetUserResult with defaults
 */
export const createSessionGetUserResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<SessionGetUserResult, 'context' | 'sessionId' | 'userId'>
): SessionGetUserResult => createPayload(context, sessionId, {
  ...data
});

/**
 * Smart session/get-user-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createSessionGetUserResultFromParams = (
  params: SessionGetUserParams,
  differences: Omit<SessionGetUserResult, 'context' | 'sessionId' | 'userId'>
): SessionGetUserResult => transformPayload(params, differences);

