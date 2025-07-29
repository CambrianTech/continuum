/**
 * Transport System - Public API Exports
 * 
 * Clean module interface for the transport system. Provides access to all
 * transport factories, types, and utilities through a single import point.
 */

// Core transport system
export { TransportFactory, TransportBase, TRANSPORT_EVENTS } from './shared';
export type { JTAGTransport, TransportConfig, TransportSendResult } from './shared';

// WebSocket transport module
export * from './websocket';

// HTTP transport module  
export * from './http';