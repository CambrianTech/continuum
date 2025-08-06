// ISSUES: 1 open, last updated 2025-07-23 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking
/**
 * 🔧 IMPROVEMENTS:
 * - [ ] Issue #1: CommandSuccessResponse still uses unknown type for commandResult (line 81)
 */

/**
 * Strong Response Types for JTAG System
 * 
 * Centralized type-safe response system eliminating unknown/any usage across
 * all daemon communications. Provides compile-time safety and runtime type
 * guards for reliable message handling.
 * 
 * CORE ARCHITECTURE:
 * - BaseResponsePayload: Common success/timestamp structure
 * - Daemon-specific response types: Console, Health, Command hierarchies
 * - Type guards: Runtime type identification for message processing
 * - Union types: Complete daemon response type coverage
 * 
 * TESTING REQUIREMENTS:
 * - Unit tests: Response construction and serialization validation
 * - Integration tests: Cross-daemon response type compatibility
 * - Type safety tests: Runtime type guard accuracy
 * - Error handling tests: Malformed response recovery
 * 
 * ARCHITECTURAL INSIGHTS:
 * - Inheritance hierarchy enables consistent error handling patterns
 * - Type guards eliminate instanceof checks in daemon code
 * - Union types provide exhaustive response coverage
 * - Specific response classes (ScreenshotResponse) extend base command pattern
 */

import { type JTAGPayload, type JTAGContext, createPayload } from './JTAGTypes';
import type { LogLevel } from '../../../daemons/console-daemon/shared/LogLevels';
import { type UUID } from './CrossPlatformUUID';

// Base response structure - for system-level responses
export interface BaseResponsePayload extends JTAGPayload {
  success: boolean;
  timestamp: string;
}

/**
 * Factory for creating base responses with defaults
 */
export const createBaseResponse = (
  success: boolean,
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<Partial<BaseResponsePayload>, 'context' | 'sessionId' | 'success' | 'timestamp'>
): BaseResponsePayload => createPayload(context, sessionId, {
  success,
  timestamp: new Date().toISOString(),
  ...data
});

// Console daemon response types
export interface ConsoleSuccessResponse extends BaseResponsePayload {
  filtered?: boolean;
  processed?: boolean;
  level?: LogLevel;
}

export interface ConsoleErrorResponse extends BaseResponsePayload {
  error: string;
}

export const createConsoleSuccessResponse = (
  data: { 
    filtered?: boolean; 
    processed?: boolean; 
    level?: LogLevel;
  }, 
  context: JTAGContext, 
  sessionId: UUID
): ConsoleSuccessResponse => createPayload(context, sessionId, {
  success: true,
  timestamp: new Date().toISOString(),
  ...data
});

export const createConsoleErrorResponse = (
  error: string,
  context: JTAGContext,
  sessionId: UUID
): ConsoleErrorResponse => createPayload(context, sessionId, {
  success: false,
  timestamp: new Date().toISOString(),
  error
});

// Health daemon response types
export interface HealthPingResponse extends BaseResponsePayload {
  pongId: string;
  uptime: number;
  memory?: {
    used: number;
    total: number;
  };
}

export interface HealthErrorResponse extends BaseResponsePayload {
  error: string;
}

export const createHealthPingResponse = (
  pongId: string,
  uptime: number,
  context: JTAGContext,
  memory: { used: number; total: number; } | undefined,
  sessionId: UUID
): HealthPingResponse => createPayload(context, sessionId, {
  success: true,
  timestamp: new Date().toISOString(),
  pongId,
  uptime,
  memory
});

export const createHealthErrorResponse = (
  error: string,
  context: JTAGContext,
  sessionId: UUID
): HealthErrorResponse => createPayload(context, sessionId, {
  success: false,
  timestamp: new Date().toISOString(),
  error
});

// Union types for each daemon
export type ConsoleResponse = ConsoleSuccessResponse | ConsoleErrorResponse;
export type HealthResponse = HealthPingResponse | HealthErrorResponse;

// All possible response types (import CommandResponse from specific daemon modules)
export type JTAGResponsePayload = ConsoleResponse | HealthResponse;

// Type guards for response identification - structural typing
export function isConsoleResponse(payload: JTAGResponsePayload): payload is ConsoleResponse {
  return 'filtered' in payload || 'processed' in payload || 'level' in payload || 
         (payload.success === false && 'error' in payload && !('pongId' in payload) && !('commandResult' in payload) && !('metadata' in payload) && !('sessions' in payload));
}

export function isHealthResponse(payload: JTAGResponsePayload): payload is HealthResponse {
  return 'pongId' in payload || 'uptime' in payload || 
         (payload.success === false && 'error' in payload && !('filtered' in payload) && !('commandResult' in payload) && !('metadata' in payload) && !('sessions' in payload));
}


// TODO: Import CommandResponse from specific daemon modules
// export function isCommandResponse(payload: JTAGResponsePayload): payload is CommandResponse {
//   return 'commandResult' in payload || 'commandName' in payload;
// }

export function isSuccessResponse(payload: JTAGResponsePayload): payload is BaseResponsePayload {
  return payload.success === true;
}

export function isErrorResponse(payload: JTAGResponsePayload): payload is BaseResponsePayload {
  return payload.success === false;
}