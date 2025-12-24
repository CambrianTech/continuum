/**
 * Logger Types - Shared types and enums for the logging system
 */

export enum LogLevel {
  DEBUG = 0,
  INFO = 1,
  WARN = 2,
  ERROR = 3,
  SILENT = 4
}

export enum FileMode {
  CLEAN = 'w',     // Start fresh on restart (default)
  APPEND = 'a',    // Keep existing logs
  ARCHIVE = 'archive'  // Archive and rotate logs (not implemented yet)
}

export interface LoggerConfig {
  level: LogLevel;
  enableColors: boolean;
  enableTimestamps: boolean;
  enableFileLogging: boolean;
  enableConsoleLogging: boolean;
}

export type LogCategory = string;

export interface LogQueueEntry {
  message: string;
}

/**
 * Parse log level from environment
 */
export function parseLogLevel(envLevel?: string): LogLevel {
  const level = envLevel?.toUpperCase() || 'INFO';
  const levelMap: Record<string, LogLevel> = {
    'DEBUG': LogLevel.DEBUG,
    'INFO': LogLevel.INFO,
    'WARN': LogLevel.WARN,
    'ERROR': LogLevel.ERROR,
    'SILENT': LogLevel.SILENT
  };
  return levelMap[level] || LogLevel.INFO;
}

/**
 * Parse file mode from environment
 */
export function parseFileMode(envMode?: string): FileMode {
  const mode = envMode?.toLowerCase() || 'clean';
  const modeMap: Record<string, FileMode> = {
    'clean': FileMode.CLEAN,
    'append': FileMode.APPEND,
    'archive': FileMode.ARCHIVE
  };
  return modeMap[mode] || FileMode.CLEAN;
}

/**
 * Create default logger config from environment
 */
export function createLoggerConfig(): LoggerConfig {
  return {
    level: parseLogLevel(process.env.LOG_LEVEL),
    enableColors: process.env.NO_COLOR !== '1',
    enableTimestamps: process.env.LOG_TIMESTAMPS !== '0',
    enableFileLogging: process.env.LOG_TO_FILES !== '0',
    enableConsoleLogging: process.env.LOG_TO_CONSOLE === '1'
  };
}
