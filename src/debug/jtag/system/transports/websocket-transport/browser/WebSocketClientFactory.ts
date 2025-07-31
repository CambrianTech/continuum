/**
 * WebSocket Client Factory - Client-only transport creation
 * 
 * Only imports client code - safe for browser bundles
 */

import type { JTAGContext } from '@shared/JTAGTypes';
import type { JTAGTransport, TransportConfig } from '../../shared/TransportTypes';
import { WebSocketClientTransport } from './WebSocketClientTransport';

export class WebSocketClientFactory {
  /**
   * Create WebSocket client transport only
   */
  static async createClientTransport(
    environment: JTAGContext['environment'],
    config: TransportConfig
  ): Promise<JTAGTransport> {
    const { 
      serverPort = 9001, 
      serverUrl = `ws://localhost:${serverPort}`, 
      eventSystem, 
      sessionId 
    } = config;

    console.log(`🔗 WebSocket Client Factory: Creating client transport in ${environment} environment`);

    const transport = new WebSocketClientTransport({ 
      url: serverUrl,
      handler: config.handler
    });
    
    if (eventSystem) {
      transport.setEventSystem(eventSystem);
    }
    if (sessionId) {
      transport.setSessionId(sessionId);
    }
    
    await transport.connect(serverUrl);
    console.log(`✅ WebSocket Client Factory: Client transport connected to ${serverUrl}`);
    return transport;
  }
}