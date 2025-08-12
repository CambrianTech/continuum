/**
 * Transport Factory Browser - Browser-specific transport creation
 * 
 * Registry-based factory using auto-generated BROWSER_ADAPTERS.
 * Eliminates god methods through plugin architecture.
 */

import type { JTAGContext } from '../../core/types/JTAGTypes';
import type { JTAGTransport, TransportConfig } from '../shared/TransportTypes';
import { TransportFactoryBase } from '../shared/TransportFactoryBase';
import { BROWSER_ADAPTERS } from '../../../browser/generated';
import type { AdapterEntry } from '../shared/TransportBase';
import type { WebSocketBrowserConfig } from '../websocket-transport/browser/WebSocketTransportClientBrowser';

export class TransportFactoryBrowser extends TransportFactoryBase {
  
  constructor() {
    super('browser');
  }

  /**
   * Registry-based transport creation - no god methods!
   */
  protected async createTransportImpl(
    environment: JTAGContext['environment'], 
    config: TransportConfig
  ): Promise<JTAGTransport> {
    
    console.log(`🔍 Browser Factory: Looking for ${config.protocol} transport in registry`);
    
    // Find adapter by protocol in auto-generated registry
    const adapterEntry = BROWSER_ADAPTERS.find(adapter => 
      adapter.name.includes(config.protocol) || 
      adapter.className.toLowerCase().includes(config.protocol)
    );
    
    if (!adapterEntry) {
      console.log(`📋 Available browser adapters: ${BROWSER_ADAPTERS.map(a => a.name).join(', ')}`);
      this.throwUnsupportedProtocol(config.protocol);
    }
    
    console.log(`✅ Browser Factory: Found adapter ${adapterEntry.className} for ${config.protocol}`);
    
    // Create adapter-specific configuration from generic TransportConfig
    const adapterConfig = this.createAdapterConfig(config, adapterEntry);
    
    // ✅ Type-safe adapter creation - TypeScript enforces ITransportAdapter interface
    const adapter = new adapterEntry.adapterClass(adapterConfig);
    
    // ✅ Type-safe connection - handle different connection patterns
    if (adapter.connect) {
      // New adapter pattern with connect() method (use URL for WebSocket)
      const connectParam = config.protocol === 'websocket' ? config.serverUrl || `ws://localhost:${config.serverPort || 9001}` : undefined;
      await adapter.connect(connectParam);
    } else {
      // Legacy transport pattern - already connected in constructor
      console.log(`📡 Browser Factory: Legacy transport ${adapterEntry.className} connected via constructor`);
    }
    
    return this.createTransportResult(adapter, adapterEntry.name);
  }

  /**
   * Create adapter-specific configuration from generic TransportConfig
   */
  private createAdapterConfig(config: TransportConfig, adapterEntry: AdapterEntry): WebSocketBrowserConfig | TransportConfig {
    if (config.protocol === 'websocket' && adapterEntry.className === 'WebSocketTransportBrowser') {
      // WebSocketBrowserConfig requires specific format
      const webSocketConfig: WebSocketBrowserConfig = {
        url: config.serverUrl || `ws://localhost:${config.serverPort || 9001}`,
        handler: config.handler,
        eventSystem: config.eventSystem,
        // WebSocket-specific options
        reconnectAttempts: 5,
        reconnectDelay: 1000,
        pingInterval: 30000,
        sessionHandshake: true
      };
      return webSocketConfig;
    }
    
    // For other adapters, pass the config as-is
    return config;
  }

  /**
   * Get factory label for logging
   */
  protected getFactoryLabel(): string {
    return 'Browser Transport Factory (Registry-Based)';
  }
}