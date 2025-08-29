/**
 * Example App Shared Code - Exported for standalone examples
 * 
 * This module provides shared functionality that all JTAG examples can use.
 * Examples import from '@continuum/jtag/example-shared' to get clean, pre-built
 * functionality without having to implement config reading themselves.
 */

export { 
  createConnectionConfig,
  createConnectionConfigFromEnv,
  createConnectionConfigAuto
} from './ConnectionConfigFactory';

// Re-export API types for convenience 
export type { 
  ConnectionConfig,
  ConnectionConfigFactory,
  ConnectionConfigValidation
} from '@continuum/jtag/types';

export {
  validateConnectionConfig
} from '@continuum/jtag/types';

// Example-specific configuration (not part of the API)
export {
  getActiveExampleName,
  getActivePorts,
  getActiveExamplePath,
  getActiveExample,
  getActivePortsSync,
  configureForBrowser
} from './ExampleConfig';