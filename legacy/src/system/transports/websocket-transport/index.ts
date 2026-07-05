/**
 * WebSocket Transport - Shared API Exports
 * 
 * Only exports shared abstractions - environment-specific code
 * must be imported from their respective directories.
 */

// No more WebSocketTransportFactory - TransportFactory handles directly

// Shared base class for custom WebSocket implementations
export { WebSocketTransportClient } from './shared/WebSocketTransportClient';
export type { WebSocketConfig } from './shared/WebSocketTransportClient';