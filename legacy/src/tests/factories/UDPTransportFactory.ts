/**
 * UDP Transport Factory for Testing
 * 
 * Simple factory that creates UDP transports for the test framework.
 * Based on the actual working architecture discovered through testing.
 */

import { UDPMulticastTransportServer } from '../../system/transports/udp-multicast-transport/server/UDPMulticastTransportServer';
import { NodeType, NodeCapability, type UDPMulticastConfig } from '../../system/transports/udp-multicast-transport/shared/UDPMulticastTypes';
import { generateUUID } from '../../system/core/types/CrossPlatformUUID';
import { TransportFactory, TestEnvironment } from '../framework/TransportTestFramework';
import type { JTAGTransport } from '../../system/transports/shared/TransportTypes';

/**
 * UDP Transport Factory Configuration
 */
export interface UDPTransportFactoryConfig {
  nodeType?: NodeType;
  capabilities?: NodeCapability[];
  basePort?: number;
}

/**
 * Factory for creating UDP Multicast Transports for testing
 */
export class UDPTransportFactory implements TransportFactory<UDPMulticastTransportServer, UDPTransportFactoryConfig> {
  public readonly name = 'udp-multicast';
  public readonly supportedEnvironments = [TestEnvironment.SERVER] as const;
  
  private portCounter = 0;
  private activeTransports = new Set<UDPMulticastTransportServer>();

  async create(config: UDPTransportFactoryConfig = {}): Promise<UDPMulticastTransportServer> {
    const transportConfig: Partial<UDPMulticastConfig> = {
      nodeId: generateUUID(),
      nodeType: config.nodeType || NodeType.SERVER,
      capabilities: config.capabilities || [NodeCapability.FILE_OPERATIONS],
      unicastPort: (config.basePort || 46000) + this.portCounter++,
      // Faster intervals for testing
      discoveryInterval: 5000,
      heartbeatInterval: 2000,
      nodeTimeout: 10000
    };

    const transport = new UDPMulticastTransportServer(transportConfig);
    this.activeTransports.add(transport);
    
    return transport;
  }

  async cleanup(transport: UDPMulticastTransportServer): Promise<void> {
    try {
      if (transport.isConnected()) {
        await transport.disconnect();
      }
      this.activeTransports.delete(transport);
    } catch (error: any) {
      console.warn(`⚠️ Error cleaning up UDP transport: ${error.message}`);
    }
  }

  /**
   * Cleanup all active transports
   */
  async cleanupAll(): Promise<void> {
    const cleanupPromises = Array.from(this.activeTransports).map(transport => 
      this.cleanup(transport)
    );
    
    await Promise.all(cleanupPromises);
    this.activeTransports.clear();
  }

  /**
   * Get count of active transports
   */
  getActiveCount(): number {
    return this.activeTransports.size;
  }
}