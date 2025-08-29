/**
 * Transport Factory Browser - Browser-specific transport creation
 * 
 * Registry-based factory using auto-generated BROWSER_ADAPTERS.
 * Eliminates god methods through plugin architecture.
 */

import type { JTAGContext } from '../../core/types/JTAGTypes';
import type { JTAGTransport, TransportConfig } from '../shared/TransportTypes';
import { TransportFactoryBase, validateAndConvertProtocol } from '../shared/TransportFactoryBase';
import { BROWSER_ADAPTERS } from '../../../browser/generated';
import type { AdapterEntry } from '../shared/TransportBase';
import type { WebSocketBrowserConfig } from '../websocket-transport/browser/WebSocketTransportClientBrowser';
import type { UDPMulticastConfig } from '../udp-multicast-transport/shared/UDPMulticastTypes';
import { NodeType, NodeCapability } from '../udp-multicast-transport/shared/UDPMulticastTypes';

export class TransportFactoryBrowser extends TransportFactoryBase {
  private readonly context: JTAGContext;
  
  constructor(context: JTAGContext) {
    super('browser');
    this.context = context;
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
    const adapterEntry = BROWSER_ADAPTERS.find(adapter => {
      // Check protocol matching - handle both 'websocket' and 'udp-multicast'
      if (config.protocol === 'udp-multicast') {
        return adapter.name.includes('udp-multicast') || adapter.className.toLowerCase().includes('udpmulticast');
      }
      
      return adapter.name.includes(config.protocol) || 
             adapter.className.toLowerCase().includes(config.protocol);
    });
    
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
      const instanceConfig = this.context.config.instance;
      const connectParam = config.protocol === 'websocket' ? config.serverUrl || `ws://localhost:${instanceConfig.ports.websocket_server}` : undefined;
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
  private createAdapterConfig(config: TransportConfig, adapterEntry: AdapterEntry): WebSocketBrowserConfig | UDPMulticastConfig | TransportConfig {
    // Use context configuration instead of global SystemConfiguration
    const instanceConfig = this.context.config.instance;
    
    if (config.protocol === 'websocket' && adapterEntry.className === 'WebSocketTransportBrowser') {
      // WebSocketBrowserConfig requires specific format
      const webSocketConfig: WebSocketBrowserConfig = {
        url: config.serverUrl || `ws://localhost:${instanceConfig.ports.websocket_server}`,
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
    
    if (config.protocol === 'udp-multicast') {
      // UDPMulticastConfig for P2P mesh networking - Browser uses WebRTC/WebSocket signalling
      const udpConfig: Partial<UDPMulticastConfig> & { signallingServer?: string } = {
        nodeType: NodeType.BROWSER,
        capabilities: [NodeCapability.SCREENSHOT, NodeCapability.BROWSER_AUTOMATION],
        multicastPort: config.serverPort ?? 37472, // Different port for browser signalling server
        unicastPort: (config.serverPort ?? 37472) + 1000, // Offset for unicast
        // Browser-specific: signalling server URL for WebRTC
        signallingServer: config.serverUrl || `ws://localhost:${config.serverPort ?? 37472}`
      };
      return udpConfig as UDPMulticastConfig;
    }
    
    // For other adapters, pass the config as-is
    return config;
  }

  /**
   * New architecture transport creation with resolved destination
   * Implements pure transport factory for dumb pipes architecture
   */
  protected async createTransportWithDestination(
    protocol: string,
    request: import('../shared/PureTransportTypes').TransportRequest,
    destination: string
  ): Promise<import('../shared/PureTransportTypes').PureTransport> {
    
    console.log(`🏭 Browser Factory: Creating ${protocol} pure transport to ${destination}`);
    
    // Convert destination URL back to legacy config for now
    // TODO: Implement pure transport adapters that accept destinations directly
    const legacyConfig: Partial<TransportConfig> & { 
      protocol: TransportConfig['protocol']; 
      role: 'client' | 'server'; 
      sessionId: string; 
    } = {
      protocol: validateAndConvertProtocol(protocol),
      serverUrl: destination,
      serverPort: request.port,
      handler: undefined, // Pure transports don't need legacy handlers
      eventSystem: undefined, // Pure transports don't need legacy event systems
      role: request.role,
      sessionId: 'pure-transport-bridge' // Bridge session until pure transports eliminate this
    };
    
    // Delegate to existing implementation for now
    const legacyTransport = await this.createTransportImpl('browser', legacyConfig as TransportConfig);
    
    // Wrap legacy transport in pure transport interface
    // This is a bridge pattern while we migrate to pure transports
    return {
      name: `pure-${protocol}-transport`,
      protocol: protocol as import('../shared/PureTransportTypes').PureTransportProtocol,
      async send(data: string | Uint8Array): Promise<import('../shared/PureTransportTypes').PureSendResult> {
        // Legacy transports use sendMessage - type-safe method detection
        if (legacyTransport && typeof legacyTransport === 'object' && 'sendMessage' in legacyTransport) {
          const sendMethod = legacyTransport.sendMessage as (data: string | Uint8Array) => Promise<void>;
          await sendMethod(data);
        }
        return { success: true, timestamp: new Date().toISOString() };
      },
      isConnected(): boolean {
        if (legacyTransport && typeof legacyTransport === 'object' && 'isConnected' in legacyTransport) {
          const isConnectedMethod = legacyTransport.isConnected as () => boolean;
          return isConnectedMethod();
        }
        return true;
      },
      async connect(): Promise<void> {
        // Already connected via createTransportImpl
      },
      async disconnect(): Promise<void> {
        if (legacyTransport && typeof legacyTransport === 'object' && 'disconnect' in legacyTransport) {
          const disconnectMethod = legacyTransport.disconnect as () => Promise<void>;
          await disconnectMethod();
        }
      }
    };
  }

  /**
   * Get factory label for logging
   */
  protected getFactoryLabel(): string {
    return 'Browser Transport Factory (Registry-Based)';
  }
}