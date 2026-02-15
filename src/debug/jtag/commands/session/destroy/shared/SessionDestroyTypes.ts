/**
 * Session Destroy Command Types
 * 
 * Shared types for session destroy command across client/server contexts.
 */

import type { JTAGContext, JTAGPayload, CommandParams, CommandResult, CommandInput} from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../system/core/shared/Commands';

/** Tear down a user session and clean up all associated resources, optionally recording the reason for destruction. */
export interface SessionDestroyParams extends CommandParams {
  context: JTAGContext;
  sessionId: UUID;
  reason?: string; // Reason for destruction (e.g., 'client_disconnect', 'timeout', 'cleanup')
}

/**
 * Result of session destroy command
 */
export interface SessionDestroyResult extends CommandResult {
  success: boolean;
  timestamp: string;
  operation: 'destroy';
  destroyedSessionId?: UUID;
  error?: string;
}

/**
 * Create a session destroy result with proper typing
 */
export function createSessionDestroyResult(
  params: SessionDestroyParams, 
  result: Partial<SessionDestroyResult>
): SessionDestroyResult {
  return {
    context: params.context,
    sessionId: params.sessionId,
    success: result.success ?? false,
    timestamp: result.timestamp ?? new Date().toISOString(),
    operation: 'destroy',
    destroyedSessionId: result.destroyedSessionId,
    error: result.error
  };
}
/**
 * SessionDestroy — Type-safe command executor
 *
 * Usage:
 *   import { SessionDestroy } from '...shared/SessionDestroyTypes';
 *   const result = await SessionDestroy.execute({ ... });
 */
export const SessionDestroy = {
  execute(params: CommandInput<SessionDestroyParams>): Promise<SessionDestroyResult> {
    return Commands.execute<SessionDestroyParams, SessionDestroyResult>('session/destroy', params as Partial<SessionDestroyParams>);
  },
  commandName: 'session/destroy' as const,
} as const;
