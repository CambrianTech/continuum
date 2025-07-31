/**
 * WebSocket Server Factory - Server-only transport creation
 * 
 * Only imports server code - not safe for browser bundles
 */

import type { JTAGContext } from '@shared/JTAGTypes';
import type { JTAGTransport, TransportConfig } from '../../shared/TransportTypes';
import { WebSocketServerTransport } from './WebSocketServerTransport';

export class WebSocketServerFactory {
  /**
   * Create WebSocket server transport only
   */
  static async createServerTransport(
    environment: JTAGContext['environment'],
    config: TransportConfig
  ): Promise<JTAGTransport> {
    const { 
      serverPort = 9001, 
      eventSystem 
    } = config;

    console.log(`🔗 WebSocket Server Factory: Creating server transport in ${environment} environment`);

    if (environment === 'browser') {
      throw new Error('Cannot create server transport in browser environment');
    }
    
    const transport = new WebSocketServerTransport({ port: serverPort });
    if (eventSystem) {
      transport.setEventSystem(eventSystem);
    }
    await transport.start(serverPort);
    console.log(`✅ WebSocket Server Factory: Server transport listening on port ${serverPort}`);
    return transport;
  }
}