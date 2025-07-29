/**
 * Transport Factory - Backwards compatibility exports
 * 
 * Re-exports from the new modular transport structure for backwards compatibility.
 * All new code should import directly from the modular structure.
 */

// Re-export the main factory and types from modular structure
export { TransportFactory } from './shared/TransportFactory';
export type { JTAGTransport, TransportConfig, TransportSendResult } from './shared/TransportTypes';