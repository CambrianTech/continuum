/**
 * Transport Factory Server - Server-specific transport creation
 * 
 * Implements ITransportFactory with server-only transport implementations.
 * Creates WebSocketTransportServer for server environment.
 */

import type { ITransportFactory } from '../shared/ITransportFactory';
import type { JTAGContext } from '../../core/types/JTAGTypes';
import type { JTAGTransport, TransportConfig } from '../shared/TransportTypes';
import { TransportConfigHelper } from '../shared/TransportConfig';
import { WebSocketTransportServer } from '../websocket-transport/server/WebSocketTransportServer';
import { WebSocketTransportClientServer } from '../websocket-transport/server/WebSocketTransportClientServer';
import { HTTPTransport } from '../http-transport/shared/HTTPTransport';

export class TransportFactoryServer implements ITransportFactory {
  /**
   * Create appropriate transport for server environment
   */
  async createTransport(
    environment: JTAGContext['environment'], 
    config: TransportConfig
  ): Promise<JTAGTransport> {
    
    // Validate required fields are present
    TransportConfigHelper.validateConfig(config);
    
    console.log(`🏭 Server Transport Factory: Creating ${config.protocol} transport for ${environment} environment`);
    
    // UDP multicast transport for P2P networking
    if (config.protocol === 'udp-multicast') {
      throw new Error('UDP Multicast transport not yet modularized');
    }
    
    // WebSocket transport
    if (config.protocol === 'websocket') {
      return await this.createWebSocketTransport(environment, config);
    }
    
    // HTTP transport
    if (config.protocol === 'http') {
      return await this.createHTTPTransport(config);
    }
    
    throw new Error(`Unsupported transport protocol: ${config.protocol}`);
  }

  /**
   * Create WebSocket transport for server environment
   */
  async createWebSocketTransport(
    environment: JTAGContext['environment'],
    config: TransportConfig
  ): Promise<JTAGTransport> {
    const { role, serverPort = 9001, serverUrl, handler, eventSystem } = config;

    console.log(`🔗 WebSocket Server Factory: Creating ${role} transport in ${environment} environment`);

    if (role === 'server') {
      if (environment === 'browser') {
        throw new Error('Cannot create server transport in browser environment');
      }

      const transport = new WebSocketTransportServer({ 
        port: serverPort,
        sessionHandshake: true
      });
      await transport.start(serverPort);
      return transport;
    }

    if (role === 'client') {
      if (environment === 'browser') {
        throw new Error('Use TransportFactoryBrowser for browser client connections');
      }

      // Create Node.js WebSocket client for server environment (CLI, server-to-server)
      const url = serverUrl || `ws://localhost:${serverPort}`;
      
      if (!handler) {
        throw new Error('WebSocket client transport requires handler configuration');
      }

      const transport = new WebSocketTransportClientServer({ 
        url,
        handler,
        sessionHandshake: true,
        eventSystem
      });
      await transport.connect(url);
      return transport;
    }

    throw new Error(`WebSocket transport role '${role}' not supported in server environment`);
  }

  /**
   * Create HTTP transport
   */
  private async createHTTPTransport(config: TransportConfig): Promise<JTAGTransport> {
    const baseUrl = config.serverUrl || 'http://localhost:9002';
    const transport = new HTTPTransport(baseUrl);
    console.log(`✅ Server Transport Factory: HTTP transport created`);
    return transport;
  }
}