/**
 * Academy Client Module Exports
 * 
 * Client-side Academy functionality for browser environments
 */

// Client base class
export { ClientAcademy } from './ClientAcademy';
export type {
  ClientAcademyStatus,
  ClientSessionConfig
} from './ClientAcademy';

// Client utilities
export { 
  checkBrowserSupport,
  getOptimalSandboxType,
  createSecureSandboxConfig
} from './ClientAcademy';