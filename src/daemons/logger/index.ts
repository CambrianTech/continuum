/**
 * Logger Daemon Module - Symmetric ProcessBasedDaemon for async logging
 * Entry point for the logger daemon module (server and client)
 * 
 * NOTE: Server and client implementations temporarily disabled for testing
 */

// Server exports
export { LoggerDaemon } from './server/LoggerDaemon';
export { LoggerClient, loggerClient } from './server/LoggerClient';

// Client exports
export { ClientLoggerDaemon } from './client/ClientLoggerDaemon';
export { ClientLoggerClient, clientLoggerClient } from './client/ClientLoggerClient';
export { ClientConsoleManager } from './client/ClientConsoleManager';

// Shared exports
export { 
  LoggerMessageFactory
} from './shared/LoggerMessageTypes';
export type { 
  LoggerMessage, 
  LogLevel, 
  LogEntry, 
  LoggerDaemonMessage 
} from './shared/LoggerMessageTypes';

// Re-export shared types for convenience
export type { 
  LoggingConfig, 
  LoggerInterface, 
  BaseLogEntry 
} from '../../logging/shared/LoggingTypes';

// Default export for main daemon class
import { LoggerDaemon } from './server/LoggerDaemon';
export default LoggerDaemon;