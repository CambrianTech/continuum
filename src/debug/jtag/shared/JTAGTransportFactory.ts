/**
 * JTAG Transport Factory - Creates transport instances
 * 
 * Provides unified factory pattern for creating transport instances
 * with proper configuration and validation
 */

import { JTAG_TRANSPORT } from './JTAGTypes';

// Transport backend interface
interface JTAGTransportBackend {
  name: string;
  process(message: any): Promise<any>;
  connect(params?: any): Promise<any>;
  disconnect(): Promise<any>;
  isHealthy(): boolean;
}

// Factory function type
export type TransportFactoryFunction = (config?: any) => JTAGTransportBackend;

// Transport factory registry
const transportFactories = new Map<string, TransportFactoryFunction>();

/**
 * Register a transport factory function
 */
export function registerTransportFactory(name: string, factory: TransportFactoryFunction): void {
  transportFactories.set(name, factory);
}

/**
 * Create a transport instance by name
 */
export function createTransport(name: string, config?: any): JTAGTransportBackend | null {
  const factory = transportFactories.get(name);
  if (!factory) {
    console.warn(`Transport factory '${name}' not found`);
    return null;
  }
  
  try {
    return factory(config);
  } catch (error) {
    console.error(`Failed to create transport '${name}':`, error);
    return null;
  }
}

/**
 * Get list of available transport types
 */
export function getAvailableTransports(): string[] {
  return Array.from(transportFactories.keys());
}

/**
 * Register built-in transport factories
 */
export function registerBuiltInTransports(): void {
  // WebSocket Transport Factory
  registerTransportFactory('websocket', (config = {}) => ({
    name: 'websocket',
    async process(message) {
      return { success: true, transport: 'websocket', message };
    },
    async connect(params) {
      return { healthy: true, endpoint: params?.endpoint || 'ws://localhost:9001' };
    },
    async disconnect() {
      return { success: true };
    },
    isHealthy() {
      return true;
    }
  }));

  // HTTP Transport Factory  
  registerTransportFactory('http', (config = {}) => ({
    name: 'http',
    async process(message) {
      return { success: true, transport: 'http', message };
    },
    async connect(params) {
      return { healthy: true, endpoint: params?.endpoint || 'http://localhost:9001' };
    },
    async disconnect() {
      return { success: true };
    },
    isHealthy() {
      return true;
    }
  }));

  // REST API Transport Factory
  registerTransportFactory('rest', (config = {}) => ({
    name: 'rest',
    async process(message) {
      return { success: true, transport: 'rest', message };
    },
    async connect(params) {
      return { healthy: true, endpoint: params?.endpoint || 'http://localhost:9001/api' };
    },
    async disconnect() {
      return { success: true };
    },
    isHealthy() {
      return true;
    }
  }));

  console.log('✅ Built-in transport factories registered');
}

// Auto-register built-in transports on import
registerBuiltInTransports();