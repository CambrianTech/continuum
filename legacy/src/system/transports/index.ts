/**
 * Transport System - Public API Exports
 * 
 * Clean module interface for the transport system. Provides access to all
 * transport factories, types, utilities, and endpoint management through a single import point.
 */

// Core transport system  
export { TransportBase, TRANSPORT_EVENTS, TRANSPORT_TYPES, TRANSPORT_ROLES } from './shared';

// Transport factory interface - implemented by environment-specific factories
export type { ITransportFactory } from './shared/ITransportFactory';
export type { 
  JTAGTransport, 
  TransportConfig,
  TransportSendResult, 
  TransportRole,
  TransportProtocol 
} from './shared';

// Transport handler interface - payload-based architecture
export type { 
  ITransportHandler 
} from './shared/ITransportHandler';
export { TransportHandlerBase } from './shared/ITransportHandler';

// Transport endpoint management
export { TransportEndpointBase } from './shared/TransportEndpoint';
export type { TransportEndpoint, TransportEndpointStatus, TransportEndpointConfig } from './shared/TransportEndpoint';

// WebSocket transport module
export * from './websocket-transport';

// HTTP transport module  
export * from './http-transport';