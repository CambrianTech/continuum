/**
 * System Scopes - Well-defined constants for system vs session operations
 * 
 * Provides standardized scope identifiers for dual-scope operations like logging,
 * where some operations need to work at both system level and session level.
 */

import { type UUID } from './CrossPlatformUUID';

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

