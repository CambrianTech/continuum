/**
 * Transport System - Public API Exports
 * 
 * Clean module interface for the transport system. Provides access to all
 * transport factories, types, utilities, and endpoint management through a single import point.
 */

// Core transport system
export { TransportFactory, TransportBase, TRANSPORT_EVENTS, TRANSPORT_TYPES } from './shared';
export type { JTAGTransport, TransportConfig, TransportSendResult } from './shared';

// Transport endpoint management
export { TransportEndpointBase } from './shared/TransportEndpoint';
export type { TransportEndpoint, TransportEndpointStatus, TransportEndpointConfig } from './shared/TransportEndpoint';

// WebSocket transport module
export * from './websocket';

// HTTP transport module  
export * from './http';