// ISSUES: 1 open, last updated 2025-07-30 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking

/**
 * Transport Factory - Modular transport creation system
 * 
 * Refactored from megafile into proper modular structure.
 * Delegates transport creation to specialized factory modules.
 * 
 * ISSUES: (look for TODOs)
 * - ENHANCEMENT: Implement shared configuration-based port management system
 *   - Create @shared/TransportConfig.ts with port ranges and defaults per transport type
 *   - Transport layer manages port availability scanning within configured ranges  
 *   - JTAGClient uses same config for service discovery and connection
 *   - Each transport type owns its port range (WebSocket: 9001-9010, HTTP: 9002-9012)
 *   - Transport instances track their own port usage and can self-cleanup
 *   - Eliminates need for code generation while maintaining client-server coordination
 */

import type { JTAGContext } from '@shared/JTAGTypes';
import type { JTAGTransport, TransportConfig } from './TransportTypes';
import { TransportConfigHelper } from './TransportConfig';
import { WebSocketTransportFactory } from '../websocket-transport/shared/WebSocketTransportFactory';
import { HTTPTransport } from '../http-transport/shared/HTTPTransport';

export class TransportFactory {
  
  /**
   * Create appropriate transport for the environment
   */
  static async createTransport(
    environment: JTAGContext['environment'], 
    config: TransportConfig
  ): Promise<JTAGTransport> {
    
    // Validate required fields are present
    TransportConfigHelper.validateConfig(config);
    
    console.log(`🏭 Transport Factory: Creating ${config.protocol} transport for ${environment} environment`);
    
    // UDP multicast transport for P2P networking
    if (config.protocol === 'udp-multicast') {
      // return await this.createUDPMulticastTransport(environment, config); // Future module
      throw new Error('UDP Multicast transport not yet modularized');
    }
    
    // WebSocket transport - use abstracted factory
    if (config.protocol === 'websocket') {
      return await WebSocketTransportFactory.createTransport(environment, config);
    }
    
    // HTTP transport
    if (config.protocol === 'http') {
      return await this.createHTTPTransport(config);
    }
    
    throw new Error(`Unsupported transport protocol: ${config.protocol}`);
  }
  
  /**
   * Create HTTP transport
   */
  private static async createHTTPTransport(config: TransportConfig): Promise<JTAGTransport> {
    const baseUrl = config.serverUrl || 'http://localhost:9002';
    const transport = new HTTPTransport(baseUrl);
    console.log(`✅ Transport Factory: HTTP transport created`);
    return transport;
  }
  
}

// Re-export types and interfaces for backwards compatibility
export type { JTAGTransport, TransportConfig, TransportSendResult } from './TransportTypes';