// ISSUES: 1 open, last updated 2025-07-29 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking

/**
 * JTAG TransportFactory - Context-Aware Message Routing with Bus-Level Queuing
 * 
 * The heart of the JTAG system - intelligent message routing with health monitoring,
 * queuing, and cross-context transport management. Handles both local and remote
 * message delivery with automatic fallback and retry mechanisms.
 * 
 * ISSUES: (look for TODOs)
 * - Move types elsewhere, such as with JTAGTransport
 * 
 * CORE ARCHITECTURE:

 * 
 * TESTING REQUIREMENTS:
 * - Unit tests: Message routing logic and subscriber management
 * - Integration tests: Cross-context transport reliability
 * - Performance tests: Message throughput under load
 * - Failure tests: Network partition and recovery scenarios
 * 
 * ARCHITECTURAL INSIGHTS:
  * - Provides a unified interface for message routing across different environments
 */


import type { JTAGContext, JTAGMessage } from '@shared/JTAGTypes';
import { JTAG_ENVIRONMENTS } from '@shared/JTAGTypes';
import { WebSocketServerTransport, WebSocketClientTransport } from '@transports/WebSocketTransport';
import { HTTPTransport } from '@transports/HTTPTransport';
import type { EventsInterface } from '@shared/JTAGEventSystem';

//TODO: Move types elsewhere, such as with JTAGTransport
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

// Transport send result
export interface TransportSendResult {
  success: boolean;
  timestamp: string;
  sentCount?: number;
}

/**
 * JTAG Transport Interface
 * 
 * Abstraction for cross-context message delivery mechanisms.
 * Implementations include WebSocket, HTTP, and in-memory transports.
 */
// TODO: Move to tranpsport module class or file
export interface JTAGTransport {
  name: string;
  send(message: JTAGMessage): Promise<TransportSendResult>;
  isConnected(): boolean;
  disconnect(): Promise<void>;
  reconnect?(): Promise<void>;
  setMessageHandler?(handler: (message: JTAGMessage) => void): void;
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