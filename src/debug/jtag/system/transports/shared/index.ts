/**
 * Transport Shared - Internal API Exports
 * 
 * Shared utilities and base classes for transport implementations.
 * Most consumers should use the main transport index instead.
 */

// Core factory and types
export { TransportFactory } from './TransportFactory';
export type { JTAGTransport, TransportConfig, TransportSendResult, TransportRole } from './TransportTypes';
export { TRANSPORT_TYPES, TRANSPORT_ROLES } from './TransportTypes';

// Base classes
export { TransportBase } from './TransportBase';

// Configuration utilities
export { TransportConfigHelper } from './TransportConfig';

// Event definitions
export { TRANSPORT_EVENTS } from './TransportEvents';