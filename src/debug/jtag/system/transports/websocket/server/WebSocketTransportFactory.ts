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
   * Create WebSocket transport for the specified environment
   */
  static async createTransport(
    environment: JTAGContext['environment'],
    config: TransportConfig
  ): Promise<JTAGTransport> {
    const { serverPort = 9001, serverUrl = 'ws://localhost:9001', eventSystem, sessionId } = config;

    if (environment === JTAG_ENVIRONMENTS.SERVER) {
      const transport = new WebSocketServerTransport({ port: serverPort });
      if (eventSystem) {
        transport.setEventSystem(eventSystem);
      }
      await transport.start(serverPort);
      console.log(`✅ WebSocket Factory: Server transport created on port ${serverPort}`);
      return transport;
    } 
    
    if (environment === JTAG_ENVIRONMENTS.BROWSER) {
      const transport = new WebSocketClientTransport({ url: serverUrl });
      if (eventSystem) {
        transport.setEventSystem(eventSystem);
      }
      if (sessionId) {
        transport.setSessionId(sessionId);
      }
      await transport.connect(serverUrl);
      console.log(`✅ WebSocket Factory: Client transport created to ${serverUrl}`);
      return transport;
    }

    throw new Error(`WebSocket transport not supported for environment: ${environment}`);
  }
}