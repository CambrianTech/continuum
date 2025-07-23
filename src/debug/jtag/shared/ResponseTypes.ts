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

import { JTAGPayload } from './JTAGTypes';
import type { LogLevel } from './LogLevels';

// Base response structure
export abstract class BaseResponsePayload extends JTAGPayload {
  success: boolean;
  timestamp: string;

  constructor(success: boolean) {
    super();
    this.success = success;
    this.timestamp = new Date().toISOString();
  }
}

// Console daemon response types
export class ConsoleSuccessResponse extends BaseResponsePayload {
  filtered?: boolean;
  processed?: boolean;
  context?: string;
  level?: LogLevel;

  constructor(data: { 
    filtered?: boolean; 
    processed?: boolean; 
    context?: string; 
    level?: LogLevel;
  }) {
    super(true);
    this.filtered = data.filtered;
    this.processed = data.processed;
    this.context = data.context;
    this.level = data.level;
  }
}

export class ConsoleErrorResponse extends BaseResponsePayload {
  error: string;

  constructor(error: string) {
    super(false);
    this.error = error;
  }
}

// Health daemon response types
export class HealthPingResponse extends BaseResponsePayload {
  pongId: string;
  uptime: number;
  memory?: {
    used: number;
    total: number;
  };

  constructor(pongId: string, uptime: number, memory?: { used: number; total: number; }) {
    super(true);
    this.pongId = pongId;
    this.uptime = uptime;
    this.memory = memory;
  }
}

export class HealthErrorResponse extends BaseResponsePayload {
  error: string;

  constructor(error: string) {
    super(false);
    this.error = error;
  }
}

// Command daemon response types
export class CommandSuccessResponse extends BaseResponsePayload {
  commandResult: unknown; // Specific command results can extend this
  executionTime?: number;

  constructor(commandResult: unknown, executionTime?: number) {
    super(true);
    this.commandResult = commandResult;
    this.executionTime = executionTime;
  }
}

export class CommandErrorResponse extends BaseResponsePayload {
  error: string;
  commandName?: string;

  constructor(error: string, commandName?: string) {
    super(false);
    this.error = error;
    this.commandName = commandName;
  }
}

// Screenshot command specific response
export class ScreenshotResponse extends CommandSuccessResponse {
  filename: string;
  path: string;
  size: number;

  constructor(filename: string, path: string, size: number, executionTime?: number) {
    super({ filename, path, size }, executionTime);
    this.filename = filename;
    this.path = path;
    this.size = size;
  }
}

// Union types for each daemon
export type ConsoleResponse = ConsoleSuccessResponse | ConsoleErrorResponse;
export type HealthResponse = HealthPingResponse | HealthErrorResponse;  
export type CommandResponse = CommandSuccessResponse | CommandErrorResponse | ScreenshotResponse;

// All possible response types
export type JTAGResponsePayload = ConsoleResponse | HealthResponse | CommandResponse;

// Type guards for response identification
export function isConsoleResponse(payload: JTAGResponsePayload): payload is ConsoleResponse {
  return payload instanceof ConsoleSuccessResponse || payload instanceof ConsoleErrorResponse;
}

export function isHealthResponse(payload: JTAGResponsePayload): payload is HealthResponse {
  return payload instanceof HealthPingResponse || payload instanceof HealthErrorResponse;
}

export function isCommandResponse(payload: JTAGResponsePayload): payload is CommandResponse {
  return payload instanceof CommandSuccessResponse || 
         payload instanceof CommandErrorResponse || 
         payload instanceof ScreenshotResponse;
}

export function isSuccessResponse(payload: JTAGResponsePayload): payload is BaseResponsePayload {
  return payload.success === true;
}

export function isErrorResponse(payload: JTAGResponsePayload): payload is BaseResponsePayload {
  return payload.success === false;
}