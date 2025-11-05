/**
 * Transport Factory Server - Server-specific transport creation
 * 
 * Registry-based factory using auto-generated SERVER_ADAPTERS.
 * Eliminates god methods through plugin architecture.
 */

import type { JTAGContext } from '../../core/types/JTAGTypes';
import type { JTAGTransport, TransportConfig } from '../shared/TransportTypes';
import { TransportFactoryBase, validateAndConvertProtocol } from '../shared/TransportFactoryBase';
import { SERVER_ADAPTERS } from '../../../server/generated';
import type { AdapterEntry } from '../shared/TransportBase';
import type { WebSocketServerConfig } from '../websocket-transport/server/WebSocketTransportServer';
import type { WebSocketServerClientConfig } from '../websocket-transport/server/WebSocketTransportClientServer';
import type { UDPMulticastConfig } from '../udp-multicast-transport/shared/UDPMulticastTypes';
import { NodeType, NodeCapability } from '../udp-multicast-transport/shared/UDPMulticastTypes';

export class TransportFactoryServer extends TransportFactoryBase {
  private readonly context: JTAGContext;
  
  constructor(context: JTAGContext) {
    super('server');
    this.context = context;
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
      // Check protocol matching - handle both 'websocket' and 'udp-multicast'
      if (config.protocol === 'udp-multicast') {
        return adapter.name.includes('udp-multicast') || adapter.className.toLowerCase().includes('udpmulticast');
      }
      
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
      const instanceConfig = this.context.config.instance;
      const connectParam = config.protocol === 'websocket' ? config.serverUrl ?? `ws://localhost:${instanceConfig.ports.websocket_server}` : undefined;
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
  private createAdapterConfig(config: TransportConfig, adapterEntry: AdapterEntry): WebSocketServerConfig | WebSocketServerClientConfig | UDPMulticastConfig | TransportConfig {
    // Use context configuration instead of global SystemConfiguration
    const instanceConfig = this.context.config.instance;
    
    if (config.protocol === 'websocket') {
      if (adapterEntry.className === 'WebSocketTransportServer') {
        // WebSocketServerConfig requires port
        const serverConfig: WebSocketServerConfig = {
          port: config.serverPort ?? instanceConfig.ports.websocket_server,
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
          url: config.serverUrl ?? `ws://localhost:${instanceConfig.ports.websocket_server}`,
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
    
    if (config.protocol === 'udp-multicast') {
      // UDPMulticastConfig for P2P mesh networking
      const udpConfig: Partial<UDPMulticastConfig> = {
        nodeType: NodeType.SERVER,
        capabilities: [NodeCapability.FILE_OPERATIONS, NodeCapability.SCREENSHOT],
        multicastPort: config.serverPort ?? 37471,
        unicastPort: (config.serverPort ?? 37471) + 1000, // Offset for unicast
        // Override any specific config properties
        ...config
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
    
    console.log(`🏭 Server Factory: Creating ${protocol} pure transport to ${destination}`);
    
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
    const legacyTransport = await this.createTransportImpl('server', legacyConfig as TransportConfig);
    
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
    return 'Server Transport Factory (Registry-Based)';
  }
}