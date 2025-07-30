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
import { WebSocketTransportFactory } from '../websocket/server/WebSocketTransportFactory';
import { HTTPTransport } from '../http/shared/HTTPTransport';

export class TransportFactory {
  
  /**
   * Create appropriate transport for the environment
   */
  static async createTransport(
    environment: JTAGContext['environment'], 
    config: Partial<TransportConfig> = {}
  ): Promise<JTAGTransport> {
    
    // Merge with defaults and validate
    const fullConfig = TransportConfigHelper.mergeWithDefaults(environment, config);
    TransportConfigHelper.validateConfig(fullConfig);
    
    console.log(`🏭 Transport Factory: Creating ${fullConfig.preferred} transport for ${environment} environment`);
    
    // UDP multicast transport for P2P networking
    if (fullConfig.preferred === 'udp-multicast') {
      // return await this.createUDPMulticastTransport(environment, fullConfig); // Future module
      throw new Error('UDP Multicast transport not yet modularized');
    }
    
    // WebSocket transport
    if (fullConfig.preferred === 'websocket') {
      try {
        return await WebSocketTransportFactory.createTransport(environment, fullConfig);
      } catch (error) {
        console.warn(`⚠️ Transport Factory: WebSocket failed, trying fallback:`, error);
        if (fullConfig.fallback) {
          return await this.createHTTPTransport(fullConfig);
        }
        throw error;
      }
    }
    
    // HTTP transport (also used as fallback)
    return await this.createHTTPTransport(fullConfig);
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
  
  /**
   * Auto-detect optimal transport configuration (delegated to helper)
   */
  static detectOptimalConfig(environment: JTAGContext['environment']): TransportConfig {
    return TransportConfigHelper.detectOptimalConfig(environment);
  }
}

// Re-export types and interfaces for backwards compatibility
export type { JTAGTransport, TransportConfig, TransportSendResult } from './TransportTypes';