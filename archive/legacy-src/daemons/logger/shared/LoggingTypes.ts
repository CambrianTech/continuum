/**
 * Logging Types - Universal types for all logging contexts
 * Used across client, server, and remote logging implementations
 */

import { ContinuumContext } from '../../../types/shared/core/ContinuumTypes';

export type LogLevel = 'debug' | 'info' | 'warn' | 'error';

export interface BaseLogEntry {
  level: LogLevel;
  message: string;
  timestamp: string;
  source: string;
  context: ContinuumContext;
  metadata?: Record<string, unknown>;
}

export interface LoggingConfig {
  enableBatching?: boolean;
  batchSize?: number;
  flushInterval?: number;
  logLevel?: LogLevel;
  maxFileSize?: number;
  maxQueueSize?: number;
}

export interface LoggingCapabilities {
  supportsBatching: boolean;
  supportsAsync: boolean;
  supportsRemote: boolean;
  supportsFileOutput: boolean;
  supportsConsoleOutput: boolean;
}

export interface LoggerInterface {
  log(context: ContinuumContext, level: LogLevel, message: string, source?: string, metadata?: Record<string, unknown>): Promise<void>;
  debug(context: ContinuumContext, message: string, source?: string, metadata?: Record<string, unknown>): Promise<void>;
  info(context: ContinuumContext, message: string, source?: string, metadata?: Record<string, unknown>): Promise<void>;
  warn(context: ContinuumContext, message: string, source?: string, metadata?: Record<string, unknown>): Promise<void>;
  error(context: ContinuumContext, message: string, source?: string, metadata?: Record<string, unknown>): Promise<void>;
  flush(sessionId?: string): Promise<void>;
  shutdown(): Promise<void>;
}

export interface LoggerFactory {
  create(config?: LoggingConfig): LoggerInterface;
  getCapabilities(): LoggingCapabilities;
}

export interface LogEntryFormatter {
  formatHuman(entry: BaseLogEntry): string;
  formatJSON(entry: BaseLogEntry): string;
  formatConsole(entry: BaseLogEntry): string;
}