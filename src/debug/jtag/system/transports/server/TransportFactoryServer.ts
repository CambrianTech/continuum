/**
 * Transport Factory Server - Server-specific transport creation
 * 
 * Registry-based factory using auto-generated SERVER_ADAPTERS.
 * Eliminates god methods through plugin architecture.
 */

import type { JTAGContext } from '../../core/types/JTAGTypes';
import type { JTAGTransport, TransportConfig } from '../shared/TransportTypes';
import { TransportFactoryBase } from '../shared/TransportFactoryBase';
import { SERVER_ADAPTERS } from '../../../server/generated';
import type { AdapterEntry } from '../shared/TransportBase';
import type { WebSocketServerConfig } from '../websocket-transport/server/WebSocketTransportServer';
import type { WebSocketServerClientConfig } from '../websocket-transport/server/WebSocketTransportClientServer';

export class TransportFactoryServer extends TransportFactoryBase {
  
  constructor() {
    super('server');
  }

  /**
   * Registry-based transport creation - no god methods!
   */
  protected async createTransportImpl(
    environment: JTAGContext['environment'], 
    config: TransportConfig
  ): Promise<JTAGTransport> {
    
    console.log(`🔍 Server Factory: Looking for ${config.protocol} transport (role: ${config.role}) in registry`);
    
    // Find adapter by protocol AND role in auto-generated registry
    const adapterEntry = SERVER_ADAPTERS.find(adapter => {
      // Check protocol matching
      if (!adapter.name.includes(config.protocol) && !adapter.className.toLowerCase().includes(config.protocol)) {
        return false;
      }
      
      // For WebSocket, also match role (server vs client)
      if (config.protocol === 'websocket') {
        if (config.role === 'server') {
          return adapter.name.includes('server') && !adapter.name.includes('client');
        }
        if (config.role === 'client') {
          return adapter.name.includes('client');
        }
        return false;
      }
      
      return true;
    });
    
    if (!adapterEntry) {
      console.log(`📋 Available server adapters: ${SERVER_ADAPTERS.map(a => a.name).join(', ')}`);
      this.throwUnsupportedProtocol(`${config.protocol} (role: ${config.role})`);
    }
    
    console.log(`✅ Server Factory: Found adapter ${adapterEntry.className} for ${config.protocol}/${config.role}`);
    
    // Create adapter-specific configuration from generic TransportConfig
    const adapterConfig = this.createAdapterConfig(config, adapterEntry);
    
    // ✅ Type-safe adapter creation - TypeScript enforces ITransportAdapter interface
    const adapter = new adapterEntry.adapterClass(adapterConfig);
    
    // ✅ Type-safe connection - all adapters implement ITransportAdapter.connect()
    if (adapter.connect) {
      // Universal adapter pattern with connect() method
      const connectParam = config.protocol === 'websocket' ? config.serverUrl ?? `ws://localhost:${config.serverPort ?? 9001}` : undefined;
      await adapter.connect(connectParam);
      console.log(`🚀 Server Factory: Adapter ${adapterEntry.className} connected successfully`);
    } else {
      // Legacy transport pattern - already connected in constructor
      console.log(`📡 Server Factory: Legacy transport ${adapterEntry.className} connected via constructor`);
    }
    
    return this.createTransportResult(adapter, adapterEntry.name);
  }

  /**
   * Create adapter-specific configuration from generic TransportConfig
   */
  private createAdapterConfig(config: TransportConfig, adapterEntry: AdapterEntry): WebSocketServerConfig | WebSocketServerClientConfig | TransportConfig {
    if (config.protocol === 'websocket') {
      if (adapterEntry.className === 'WebSocketTransportServer') {
        // WebSocketServerConfig requires port
        const serverConfig: WebSocketServerConfig = {
          port: config.serverPort ?? 9001,
          // WebSocket-specific options
          reconnectAttempts: 5,
          reconnectDelay: 1000,
          pingInterval: 30000,
          sessionHandshake: true
        };
        return serverConfig;
      } else if (adapterEntry.className === 'WebSocketTransportClientServer') {
        // WebSocketServerClientConfig requires url, handler, eventSystem
        const clientConfig: WebSocketServerClientConfig = {
          url: config.serverUrl ?? `ws://localhost:${config.serverPort ?? 9001}`,
          handler: config.handler,
          eventSystem: config.eventSystem,
          // WebSocket-specific options
          reconnectAttempts: 5,
          reconnectDelay: 1000,
          pingInterval: 30000,
          sessionHandshake: true
        };
        return clientConfig;
      }
    }
    
    // For other adapters, pass the config as-is
    return config;
  }

  /**
   * Get factory label for logging
   */
  protected getFactoryLabel(): string {
    return 'Server Transport Factory (Registry-Based)';
  }
}