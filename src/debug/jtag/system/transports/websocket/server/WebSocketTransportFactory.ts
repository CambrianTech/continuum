/**
 * WebSocket Transport Factory - Creates WebSocket transports for different environments
 * 
 * Handles the WebSocket-specific creation logic extracted from main TransportFactory.
 */

import { JTAG_ENVIRONMENTS } from '@shared/JTAGTypes';
import type { JTAGContext } from '@shared/JTAGTypes';
import type { JTAGTransport, TransportConfig } from '../../shared/TransportTypes';
import { WebSocketServerTransport } from './WebSocketServerTransport';
import { WebSocketClientTransport } from '../client/WebSocketClientTransport';

export class WebSocketTransportFactory {
  /**
   * Create WebSocket transport based on role (client/server) and environment
   */
  static async createTransport(
    environment: JTAGContext['environment'],
    config: TransportConfig
  ): Promise<JTAGTransport> {
    const { 
      role = 'server', // Default to server for backward compatibility
      serverPort = 9001, 
      serverUrl = 'ws://localhost:9001', 
      eventSystem, 
      sessionId 
    } = config;

    console.log(`🔗 WebSocket Factory: Creating ${role} transport in ${environment} environment`);

    // Server role: Create listener (regardless of environment)
    if (role === 'server') {
      const transport = new WebSocketServerTransport({ port: serverPort });
      if (eventSystem) {
        transport.setEventSystem(eventSystem);
      }
      await transport.start(serverPort);
      console.log(`✅ WebSocket Factory: Server transport listening on port ${serverPort}`);
      return transport;
    } 
    
    // Client role: Create connector (regardless of environment)
    if (role === 'client') {
      const transport = new WebSocketClientTransport({ url: serverUrl });
      if (eventSystem) {
        transport.setEventSystem(eventSystem);
      }
      if (sessionId) {
        transport.setSessionId(sessionId);
      }
      await transport.connect(serverUrl);
      console.log(`✅ WebSocket Factory: Client transport connected to ${serverUrl}`);
      return transport;
    }

    throw new Error(`WebSocket transport role '${role}' not supported`);
  }
}