/**
 * Logger Daemon Module - ProcessBasedDaemon for async logging
 * Entry point for the logger daemon module
 */

export { LoggerDaemon } from './LoggerDaemon';
export { LoggerClient, loggerClient } from './LoggerClient';
export { 
  LoggerMessageFactory
} from './LoggerMessageTypes';
export type { 
  LoggerMessage, 
  LogLevel, 
  LogEntry, 
  LoggerDaemonMessage 
} from './LoggerMessageTypes';

// Re-export shared types for convenience
export type { 
  LoggingConfig, 
  LoggerInterface, 
  BaseLogEntry 
} from '../../logging/shared/LoggingTypes';

// Default export for main daemon class
import { LoggerDaemon } from './LoggerDaemon';
export default LoggerDaemon;