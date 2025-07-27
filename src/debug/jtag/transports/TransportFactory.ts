/**
 * Transport Factory - Auto-detect and create appropriate transports
 */

import { JTAGTransport } from '../shared/JTAGRouter';
import type { JTAGContext } from '../shared/JTAGTypes';
import { JTAG_ENVIRONMENTS } from '../shared/JTAGTypes';
import { WebSocketServerTransport, WebSocketClientTransport } from './WebSocketTransport';
import { HTTPTransport } from './HTTPTransport';
// import { UDPMulticastTransport } from './udp-multicast/UDPMulticastTransport'; // Disabled - god object
import type { EventsInterface } from '../shared/JTAGRouter';

export interface TransportConfig {
  preferred?: 'websocket' | 'http' | 'udp-multicast';
  fallback?: boolean;
  serverPort?: number;
  serverUrl?: string;
  eventSystem?: EventsInterface;
  sessionId?: string; // Session ID for client handshake
  // UDP multicast specific options
  p2p?: {
    nodeId?: string;
    nodeType?: 'server' | 'browser' | 'mobile' | 'ai-agent';
    capabilities?: string[];
    multicastAddress?: string;
    multicastPort?: number;
    unicastPort?: number;
    encryptionKey?: string;
  };
}

export class TransportFactory {
  
  /**
   * Create appropriate transport for the environment
   */
  static async createTransport(
    environment: JTAGContext['environment'], 
    config: TransportConfig = {}
  ): Promise<JTAGTransport> {
    
    const { preferred = 'websocket', fallback = true, serverPort = 9001, serverUrl = 'ws://localhost:9001', eventSystem, sessionId } = config;
    
    console.log(`🏭 Transport Factory: Creating transport for ${environment} environment`);
    
    // UDP multicast transport for P2P networking
    if (preferred === 'udp-multicast') {
      // return await this.createUDPMulticastTransport(environment, config); // Disabled - god object
      throw new Error('UDP Multicast transport disabled - god object violation');
    }
    
    if (preferred === 'websocket') {
      if (environment === JTAG_ENVIRONMENTS.SERVER) {
        const transport = new WebSocketServerTransport();
        if (eventSystem) {
          transport.setEventSystem(eventSystem);
        }
        try {
          await transport.start(serverPort);
          console.log(`✅ Transport Factory: WebSocket server transport created`);
          return transport;
        } catch (error) {
          console.warn(`⚠️ Transport Factory: WebSocket server failed, trying fallback:`, error);
          if (fallback) {
            return await this.createHTTPTransport();
          }
          throw error;
        }
      } else if (environment === 'browser') {
        const transport = new WebSocketClientTransport();
        if (eventSystem) {
          transport.setEventSystem(eventSystem);
        }
        if (sessionId) {
          transport.setSessionId(sessionId);
        }
        try {
          await transport.connect(serverUrl);
          console.log(`✅ Transport Factory: WebSocket client transport created`);
          return transport;
        } catch (error) {
          console.warn(`⚠️ Transport Factory: WebSocket client failed, trying fallback:`, error);
          if (fallback) {
            return await this.createHTTPTransport();
          }
          throw error;
        }
      }
    }
    
    // HTTP transport
    return await this.createHTTPTransport();
  }
  
  /**
   * Create HTTP transport
   */
  private static async createHTTPTransport(): Promise<JTAGTransport> {
    const transport = new HTTPTransport();
    console.log(`✅ Transport Factory: HTTP transport created`);
    return transport;
  }

  /**
   * Create UDP multicast transport for P2P networking
   */
  /* Disabled - god object violation
  private static async createUDPMulticastTransport(
    environment: JTAGContext['environment'],
    config: TransportConfig
  ): Promise<JTAGTransport> {
    const p2pConfig = config.p2p || {};
    
    // Generate node ID based on environment if not provided
    const nodeId = p2pConfig.nodeId || `${environment}-${Date.now()}-${Math.random().toString(36).substr(2, 4)}`;
    
    const udpConfig = {
      nodeId,
      nodeType: p2pConfig.nodeType || (environment === 'server' ? 'server' as const : 'browser' as const),
      capabilities: p2pConfig.capabilities || ['chat', 'database', 'compiler', 'artifacts'],
      multicastAddress: p2pConfig.multicastAddress || '239.255.7.33',
      multicastPort: p2pConfig.multicastPort || 7331,
      unicastPort: p2pConfig.unicastPort || 7332,
      encryptionKey: p2pConfig.encryptionKey,
      requireAuth: !!p2pConfig.encryptionKey,
      discoveryInterval: 30000,
      maxPacketSize: 65507,
      fragmentationEnabled: true,
      compressionEnabled: true,
      stunServers: ['stun:stun1.l.google.com:19302'],
      enableUPnP: false
    };

    console.log(`🌐 Transport Factory: Creating UDP multicast transport for node ${nodeId}`);
    
    // const transport = new UDPMulticastTransport(udpConfig);
    // const context: JTAGContext = { environment };
    // const initialized = await transport.initialize(context);
    
    // if (!initialized) {
    //   throw new Error('Failed to initialize UDP multicast transport');
    // }
    // console.log(`✅ Transport Factory: UDP multicast transport created for P2P networking`);
    // return transport;
    throw new Error('UDP Multicast transport disabled - god object violation');
  }
  */
  
  /**
   * Auto-detect optimal transport configuration
   */
  static detectOptimalConfig(environment: JTAGContext['environment']): TransportConfig {
    // In browser, prefer WebSocket client
    if (environment === JTAG_ENVIRONMENTS.BROWSER) {
      return {
        preferred: 'websocket',
        fallback: true,
        serverUrl: 'ws://localhost:9001'
      };
    }
    
    // On server, prefer WebSocket server
    if (environment === JTAG_ENVIRONMENTS.SERVER) {
      return {
        preferred: 'websocket',
        fallback: true,
        serverPort: 9001
      };
    }
    
    // Remote contexts use HTTP by default
    return {
      preferred: 'http',
      fallback: false
    };
  }
}