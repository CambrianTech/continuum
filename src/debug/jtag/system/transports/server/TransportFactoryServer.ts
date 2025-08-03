/**
 * Transport Factory Server - Server-specific transport creation
 * 
 * Extends TransportFactoryBase following Universal Module Architecture.
 * Only implements server-specific transport creation logic.
 */

import type { JTAGContext } from '../../core/types/JTAGTypes';
import type { JTAGTransport, TransportConfig } from '../shared/TransportTypes';
import { TransportFactoryBase } from '../shared/TransportFactoryBase';
import { WebSocketTransportServer } from '../websocket-transport/server/WebSocketTransportServer';
import { WebSocketTransportServerClient } from '../websocket-transport/server/WebSocketTransportServerClient';
import { HTTPTransport } from '../http-transport/shared/HTTPTransport';

export class TransportFactoryServer extends TransportFactoryBase {
  
  constructor() {
    super('server');
  }
  /**
   * Server-specific transport creation implementation
   */
  protected async createTransportImpl(
    environment: JTAGContext['environment'], 
    config: TransportConfig
  ): Promise<JTAGTransport> {
    
    // UDP multicast transport for P2P networking
    if (config.protocol === 'udp-multicast') {
      this.throwUnsupportedProtocol('udp-multicast (not yet modularized)');
    }
    
    // WebSocket transport
    if (config.protocol === 'websocket') {
      return await this.createWebSocketTransportImpl(environment, config);
    }
    
    // HTTP transport
    if (config.protocol === 'http') {
      return await this.createHTTPTransport(config);
    }
    
    this.throwUnsupportedProtocol(config.protocol);
  }

  /**
   * Server-specific WebSocket transport implementation
   */
  protected async createWebSocketTransportImpl(
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
      return this.createTransportResult(transport, 'WebSocket Server');
    }

    if (role === 'client') {
      if (environment === 'browser') {
        throw new Error('Use TransportFactoryBrowser for browser client connections');
      }

      // Create Node.js WebSocket client for server environment (CLI, server-to-server)
      const url = serverUrl ?? `ws://localhost:${serverPort}`;

      if (!handler) {
        throw new Error('WebSocket client transport requires handler configuration');
      }

      const transport = new WebSocketTransportServerClient({ 
        url,
        handler,
        sessionHandshake: true,
        eventSystem
      });
      await transport.connect(url);
      return this.createTransportResult(transport, 'WebSocket Client');
    }

    throw new Error(`WebSocket transport role '${role}' not supported in server environment`);
  }

  /**
   * Get factory label for logging
   */
  protected getFactoryLabel(): string {
    return 'Server Transport Factory';
  }

  /**
   * Create HTTP transport
   */
  private async createHTTPTransport(config: TransportConfig): Promise<JTAGTransport> {
    const baseUrl = config.serverUrl ?? 'http://localhost:9002';
    const transport = new HTTPTransport(baseUrl);
    console.log(`✅ Server Transport Factory: HTTP transport created`);
    return transport;
  }
}