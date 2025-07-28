/**
 * System Scopes - Well-defined constants for system vs session operations
 * 
 * Provides standardized scope identifiers for dual-scope operations like logging,
 * where some operations need to work at both system level and session level.
 */

import { type UUID } from './CrossPlatformUUID';

/**
 * Session directory categories - passed in from connect() or caller
 */
export type SessionCategory = 'user' | 'persona' | 'agent' | 'system';

/**
 * Default display names for bootstrapping
 */
export const DEFAULT_DISPLAY_NAMES = {
  NEW_USER: 'New User',
  SYSTEM: 'System'
} as const;

/**
 * System-level scope constants
 */
export const SYSTEM_SCOPES = {
  /**
   * System-wide operations that apply to all sessions
   * Used for: .continuum/jtag/logs/, system health, daemon status, console logging, events
   */
  SYSTEM: '00000000-0000-0000-0000-000000000000' as UUID
} as const;

/**
 * UUID utility functions
 */
export function isSystemUUID(sessionId: UUID): boolean {
  return sessionId === SYSTEM_SCOPES.SYSTEM;
}

/**
 * Determine if logs should be dual-scoped (both system and session)
 */
export function shouldDualScope(sessionId?: UUID): boolean {
  // Only dual-scope for real sessions, not system operations
  return !!sessionId && !isSystemUUID(sessionId);
}

/**
 * Global session context for tracking current session during operations
 * This is similar to thread-local storage for session context
 */
class SessionContext {
  private currentSessionId: UUID | null = null;

  setSessionId(sessionId: UUID | null): void {
    this.currentSessionId = sessionId;
  }

  getCurrentSessionId(): UUID | null {
    return this.currentSessionId;
  }

  async withSession<T>(sessionId: UUID, fn: () => Promise<T>): Promise<T> {
    const previousSessionId = this.currentSessionId;
    this.currentSessionId = sessionId;
    try {
      return await fn();
    } finally {
      this.currentSessionId = previousSessionId;
    }
  }
}

export const globalSessionContext = new SessionContext();

