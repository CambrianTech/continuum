/**
 * Unified Console Types - Shared between logger daemon and console forwarding
 * Consolidates console-related types to eliminate duplication
 */

import { ContinuumContext } from '../../../types/shared/core/ContinuumTypes';

/**
 * Unified log levels for both console and logger daemon
 */
export type LogLevel = 'log' | 'debug' | 'info' | 'warn' | 'error' | 'trace' | 'probe';

/**
 * Console log levels enum for backward compatibility
 */
export enum ConsoleLevel {
  ERROR = 'error',
  WARN = 'warn', 
  INFO = 'info',
  LOG = 'log',
  DEBUG = 'debug',
  TRACE = 'trace',
  PROBE = 'probe'
}

/**
 * Browser console override interface
 */
export interface OriginalConsole {
  log: (...args: unknown[]) => void;
  warn: (...args: unknown[]) => void;
  error: (...args: unknown[]) => void;
  info: (...args: unknown[]) => void;
  debug: (...args: unknown[]) => void;
  trace: (...args: unknown[]) => void;
}

/**
 * AI Probe interface for diagnostic logging
 */
export interface ProbeData {
  /** Human-readable probe description */
  message: string;
  /** Structured data for analysis */
  data?: Record<string, unknown>;
  /** Optional JavaScript code to execute and capture result */
  executeJS?: string;
  /** Base64 encoded JavaScript for wire transmission (internal use) */
  executeJSBase64?: string;
  /** Probe category (ai-diagnostic, performance, error-trace, etc.) */
  category?: string;
  /** Tags for filtering and organization */
  tags?: string[];
}

/**
 * Properly typed console arguments that can be JSON serialized
 */
export type ConsoleArgument = 
  | string 
  | number 
  | boolean 
  | null 
  | undefined
  | Error
  | Record<string, unknown>
  | Array<unknown>;

/**
 * Unified log entry interface
 */
export interface LogEntry {
  level: LogLevel;
  message: string;
  timestamp: number;
  sessionId: string;
  source: string;
  context: ContinuumContext;
  data?: Record<string, unknown> | undefined;
}

/**
 * Legacy console log entry for backward compatibility
 */
export interface LegacyConsoleLogEntry {
  level: LogLevel;
  message: string;
  timestamp: number;
  sessionId: string;
  args?: ConsoleArgument[];
  serializedMessage?: string;
}

/**
 * Utility functions for console message handling
 */
export class ConsoleUtils {
  /**
   * Serialize console arguments for logging
   */
  static serializeArgs(args: unknown[]): string {
    return args.map(arg => ConsoleUtils.serializeArg(arg)).join(' ');
  }

  /**
   * Serialize a single argument for logging
   */
  static serializeArg(arg: unknown): string {
    if (arg === null) return 'null';
    if (arg === undefined) return 'undefined';
    if (typeof arg === 'string') return arg;
    if (typeof arg === 'number' || typeof arg === 'boolean') return String(arg);
    if (typeof arg === 'function') return `[Function: ${arg.name || 'anonymous'}]`;
    if (arg instanceof Error) return `${arg.name}: ${arg.message}`;
    
    try {
      return JSON.stringify(arg, null, 2);
    } catch {
      return String(arg);
    }
  }

  /**
   * Create a log entry from console arguments
   */
  static createLogEntry(
    level: LogLevel,
    args: unknown[],
    context: ContinuumContext,
    source: string = 'browser-console'
  ): LogEntry {
    const entry: LogEntry = {
      level,
      message: ConsoleUtils.serializeArgs(args),
      timestamp: Date.now(),
      sessionId: context.sessionId,
      source,
      context
    };
    
    if (args.length > 0) {
      entry.data = { args };
    }
    
    return entry;
  }

  /**
   * Convert between old Console.Level and new LogLevel
   */
  static normalizeLogLevel(level: string): LogLevel {
    switch (level.toLowerCase()) {
      case 'error':
        return 'error';
      case 'warn':
        return 'warn';
      case 'info':
        return 'info';
      case 'log':
        return 'log';
      case 'debug':
        return 'debug';
      case 'trace':
        return 'trace';
      case 'probe':
        return 'probe';
      default:
        return 'log';
    }
  }
}