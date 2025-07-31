/**
 * WebSocket Transport Factory - Environment-aware abstraction
 * 
 * Uses proper abstraction to delegate to environment-specific factories
 * without importing server code in browser or vice versa
 */

import type { JTAGContext } from '@shared/JTAGTypes';
import type { JTAGTransport, TransportConfig } from '../../shared/TransportTypes';

export class WebSocketTransportFactory {
  /**
   * Create WebSocket transport using environment-specific factories
   */
  static async createTransport(
    environment: JTAGContext['environment'],
    config: TransportConfig
  ): Promise<JTAGTransport> {
    const { role } = config;

    console.log(`🔗 WebSocket Factory: Creating ${role} transport in ${environment} environment`);

    // Server role: Use server factory (server environment only)
    if (role === 'server') {
      if (environment === 'browser') {
        throw new Error('Cannot create server transport in browser environment');
      }
      
      // Import server factory (abstraction - no server code in main factory)
      const { WebSocketServerFactory } = await import('../server/WebSocketServerFactory');
      return await WebSocketServerFactory.createServerTransport(environment, config);
    }
    
    // Client role: Use client factory (any environment)
    if (role === 'client') {
      // Import client factory (abstraction - no browser code in main factory)
      const { WebSocketClientFactory } = await import('../browser/WebSocketClientFactory');
      return await WebSocketClientFactory.createClientTransport(environment, config);
    }

    throw new Error(`WebSocket transport role '${role}' not supported`);
  }
}