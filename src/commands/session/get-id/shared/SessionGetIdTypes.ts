import { Commands } from '../../../../system/core/shared/Commands';
import type { CommandParams, CommandResult, CommandInput} from '../../../../system/core/types/JTAGTypes';

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
