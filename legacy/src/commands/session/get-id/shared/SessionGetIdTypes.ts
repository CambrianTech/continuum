import { Commands } from '../../../../system/core/shared/Commands';
import type { CommandParams, CommandResult, CommandInput} from '../../../../system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Session Get ID Command - Get current session ID
 *
 * Convenience command that returns the caller's session ID.
 * No parameters needed - sessionId is auto-injected by framework.
 */

export interface SessionGetIdParams extends CommandParams {
  // No additional parameters - uses auto-injected sessionId
}

export interface SessionGetIdResult extends CommandResult {
  readonly success: boolean;
  readonly error?: string;
  // Note: sessionId is already available via CommandResult -> JTAGPayload -> sessionId
}

/**
 * SessionGetId — Type-safe command executor
 *
 * Usage:
 *   import { SessionGetId } from '...shared/SessionGetIdTypes';
 *   const result = await SessionGetId.execute({ ... });
 */
export const SessionGetId = {
  execute(params: CommandInput<SessionGetIdParams>): Promise<SessionGetIdResult> {
    return Commands.execute<SessionGetIdParams, SessionGetIdResult>('session/get-id', params as Partial<SessionGetIdParams>);
  },
  commandName: 'session/get-id' as const,
} as const;

/**
 * Factory function for creating SessionGetIdParams
 */
export const createSessionGetIdParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<SessionGetIdParams, 'context' | 'sessionId' | 'userId'>
): SessionGetIdParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Factory function for creating SessionGetIdResult with defaults
 */
export const createSessionGetIdResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<SessionGetIdResult, 'context' | 'sessionId' | 'userId'>
): SessionGetIdResult => createPayload(context, sessionId, {
  ...data
});

/**
 * Smart session/get-id-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createSessionGetIdResultFromParams = (
  params: SessionGetIdParams,
  differences: Omit<SessionGetIdResult, 'context' | 'sessionId' | 'userId'>
): SessionGetIdResult => transformPayload(params, differences);

