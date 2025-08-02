/**
 * Transport Shared - Internal API Exports
 * 
 * Shared utilities and base classes for transport implementations.
 * Most consumers should use the main transport index instead.
 */

// Core factory interface - implemented by environment-specific factories
export type { ITransportFactory } from './ITransportFactory';
export type { 
  JTAGTransport, 
  TransportConfig,
  TransportSendResult, 
  TransportRole,
  TransportProtocol 
} from './TransportTypes';
export { TRANSPORT_TYPES, TRANSPORT_ROLES } from './TransportTypes';

// Base classes
export { TransportBase } from './TransportBase';

// Configuration utilities
export { TransportConfigHelper } from './TransportConfig';

// Event definitions
export { TRANSPORT_EVENTS } from './TransportEvents';