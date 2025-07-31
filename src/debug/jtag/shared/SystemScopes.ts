/**
 * System Scopes - Well-defined constants for system vs session operations
 * 
 * Provides standardized scope identifiers for dual-scope operations like logging,
 * where some operations need to work at both system level and session level.
 */

import { type UUID } from '@shared/CrossPlatformUUID';

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
   * Used for: system health, daemon status, console logging, events
   * Logs stored in: .continuum/jtag/sessions/system/{SYSTEM_UUID}/logs/
   */
  SYSTEM: '00000000-0000-0000-0000-000000000000' as UUID,
  
  /**
   * Bootstrap session for clients before SessionDaemon assigns real session
   * Used for: client initialization, session requests, temporary operations
   * This session should be replaced by SessionDaemon with a real session ID
   * Format: DEADBEEF-style hex that spells "UNKNOWN SESSION" using hex digits
   * Uses only valid hex digits: 0-9, A-F (case insensitive)
   */
  UNKNOWN_SESSION: 'deadbeef-cafe-4bad-8ace-5e551000c0de' as UUID
} as const;

/**
 * UUID utility functions
 */
export function isSystemUUID(sessionId: UUID): boolean {
  return sessionId === SYSTEM_SCOPES.SYSTEM;
}

export function isUnknownSession(sessionId: UUID): boolean {
  return sessionId === SYSTEM_SCOPES.UNKNOWN_SESSION;
}

export function isBootstrapSession(sessionId: UUID): boolean {
  return isSystemUUID(sessionId) || isUnknownSession(sessionId);
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

