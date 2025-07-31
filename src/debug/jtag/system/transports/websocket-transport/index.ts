/**
 * WebSocket Transport - Shared API Exports
 * 
 * Only exports shared abstractions - environment-specific code
 * must be imported from their respective directories.
 */

// No more WebSocketTransportFactory - TransportFactory handles directly

// Shared base class for custom WebSocket implementations
export { WebSocketTransportBase } from './shared/WebSocketTransportBase';
export type { WebSocketConfig } from './shared/WebSocketTransportBase';