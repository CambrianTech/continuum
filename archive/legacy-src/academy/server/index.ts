/**
 * Academy Server Module Exports
 * 
 * Server-side Academy functionality for Node.js environments
 */

// Server base class
export { ServerAcademy } from './ServerAcademy';
export type {
  ServerAcademyStatus,
  ServerSessionConfig,
  SessionMetrics,
  SystemResources
} from './ServerAcademy';

// Server utilities
export { 
  createSecureServerConfig,
  checkSystemResources
} from './ServerAcademy';