import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';

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
