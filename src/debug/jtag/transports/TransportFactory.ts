/**
 * Transport Factory - Auto-detect and create appropriate transports
 */

import { JTAGTransport } from '../shared/JTAGRouter';
import type { JTAGContext } from '../shared/JTAGTypes';
import { JTAG_ENVIRONMENTS } from '../shared/JTAGTypes';
import { WebSocketServerTransport, WebSocketClientTransport } from './WebSocketTransport';
import { HTTPTransport } from './HTTPTransport';
import type { EventsInterface } from '../shared/JTAGRouter';

export interface TransportConfig {
  preferred?: 'websocket' | 'http';
  fallback?: boolean;
  serverPort?: number;
  serverUrl?: string;
  eventSystem?: EventsInterface;
}

export class TransportFactory {
  
  /**
   * Create appropriate transport for the environment
   */
  static async createTransport(
    environment: JTAGContext['environment'], 
    config: TransportConfig = {}
  ): Promise<JTAGTransport> {
    
    const { preferred = 'websocket', fallback = true, serverPort = 9001, serverUrl = 'ws://localhost:9001', eventSystem } = config;
    
    console.log(`🏭 Transport Factory: Creating transport for ${environment} environment`);
    
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