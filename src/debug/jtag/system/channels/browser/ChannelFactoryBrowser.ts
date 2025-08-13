/**
 * Transport Factory Browser - Browser-specific transport creation
 * 
 * Implements ITransportFactory with browser-only transport implementations.
 * Creates WebSocketTransportBrowser for browser environment.
 */

import type { ITransportFactory } from '../shared/ITransportFactory';
import type { JTAGContext } from '../../core/types/JTAGTypes';
import type { JTAGTransport, TransportConfig } from '../shared/TransportTypes';
import { TransportConfigHelper } from '../shared/TransportConfig';
import { WebSocketTransportClientBrowser } from '../../transports/websocket-transport/browser/WebSocketTransportClientBrowser';
// Update the import path below if the actual location is different
import { HTTPTransport } from '../../transports/http-transport/shared/HTTPTransport';
import { getSystemConfig } from '../../core/config/SystemConfiguration';

export class TransportFactoryBrowser implements ITransportFactory {
  /**
   * Create appropriate transport for browser environment
   */
  async createTransport(
    environment: JTAGContext['environment'], 
    config: TransportConfig
  ): Promise<JTAGTransport> {
    
    // Validate required fields are present
    TransportConfigHelper.validateConfig(config);
    
    console.log(`🏭 Browser Transport Factory: Creating ${config.protocol} transport for ${environment} environment`);
    
    // UDP multicast transport not supported in browser
    if (config.protocol === 'udp-multicast') {
      throw new Error('UDP Multicast transport not supported in browser environment');
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
   * Create WebSocket transport for browser environment
   */
  async createWebSocketTransport(
    environment: JTAGContext['environment'],
    config: TransportConfig
  ): Promise<JTAGTransport> {
    const systemConfig = getSystemConfig();
    const { role, handler } = config;
    
    // Use system configuration for ports and URLs
    const serverPort = config.serverPort || systemConfig.getWebSocketPort();
    const serverUrl = config.serverUrl || systemConfig.getWebSocketUrl();

    console.log(`🔗 WebSocket Browser Factory: Creating ${role} transport in ${environment} environment`);

    if (role === 'client') {
      const url = serverUrl ?? `ws://localhost:${serverPort}`;
      const transport = new WebSocketTransportClientBrowser({ 
        url,
        handler: handler,
        sessionHandshake: true,
        eventSystem: config.eventSystem // CRITICAL: Pass event system for health management
      });
      await transport.connect(url);
      return transport;
    }

    throw new Error(`WebSocket transport role '${role}' not supported in browser environment`);
  }

  /**
   * Create HTTP transport
   */
  private async createHTTPTransport(config: TransportConfig): Promise<JTAGTransport> {
    const systemConfig = getSystemConfig();
    const baseUrl = config.serverUrl ?? systemConfig.getHTTPUrl();
    const transport = new HTTPTransport(baseUrl);
    console.log(`✅ Browser Transport Factory: HTTP transport created`);
    return transport;
  }
}