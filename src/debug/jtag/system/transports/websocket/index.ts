/**
 * WebSocket Transport - Public API Exports
 * 
 * Clean module interface for WebSocket transport implementations.
 */

// Factory for creating WebSocket transports
export { WebSocketTransportFactory } from './server/WebSocketTransportFactory';

// Transport implementations
export { WebSocketClientTransport } from './client/WebSocketClientTransport';
export { WebSocketServerTransport } from './server/WebSocketServerTransport';

// Shared base class for custom WebSocket implementations
export { WebSocketTransportBase } from './shared/WebSocketTransportBase';
export type { WebSocketConfig } from './shared/WebSocketTransportBase';