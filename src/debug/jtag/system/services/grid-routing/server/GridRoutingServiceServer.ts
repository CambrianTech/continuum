/**
 * Grid Routing Service - Server Implementation
 * 
 * Server-side implementation of The Grid routing service using Node.js
 * networking and the existing UDP multicast transport layer.
 * 
 * Provides the server-specific 5-10% implementation while leveraging
 * the 80-90% shared logic in GridRoutingService.
 */

import { GridRoutingService } from '../shared/GridRoutingService';
import type {
  GridMessage,
  GridRoutingConfig,
  GridNode,
  MessageRoute,
  GridNodeIdentity,
  GridNodeStatus,
  GridNodeMetadata
} from '../shared/GridRoutingTypes';
import type { UUID } from '../../../core/types/CrossPlatformUUID';
import type { UDPMulticastTransportServer } from '../../../transports/udp-multicast-transport/server/UDPMulticastTransportServer';

export class GridRoutingServiceServer extends GridRoutingService {
  // Server-specific transport implementation - no command execution logic

  constructor(
    config: GridRoutingConfig,
    localNode: GridNode,
    private transport: UDPMulticastTransportServer
  ) {
    super(config, localNode);
    this.setupTransportHandlers();
  }

  /**
   * Initialize server-specific Grid routing
   */
  async initialize() {
    console.log(`🖥️ Grid Routing Server: Initializing with UDP transport`);
    
    // Ensure transport is initialized
    if (!this.transport.isConnected()) {
      console.log(`📡 Grid Routing Server: Initializing UDP transport`);
      await this.transport.initialize();
    }
    
    // Call parent initialization
    return await super.initialize();
  }

  /**
   * SERVER-SPECIFIC IMPLEMENTATIONS
   */

  /**
   * Forward message to next hop via UDP transport
   */
  protected async forwardMessage(message: GridMessage, route: MessageRoute): Promise<void> {
    console.log(`🔄 Grid Routing Server: Forwarding message ${message.messageId} to ${route.nextHop.substring(0, 8)}`);
    
    try {
      // Serialize message for UDP transport
      const messageBuffer = this.serializeMessage(message);
      
      // Get next hop node info
      const nextHopNode = this.nodes.get(route.nextHop);
      if (!nextHopNode) {
        throw new Error(`Next hop node ${route.nextHop} not found`);
      }
      
      // Send via UDP unicast to next hop
      await this.transport.sendUnicastMessage(
        nextHopNode.endpoints.unicastPort,
        messageBuffer
      );
      
      console.log(`✅ Grid Routing Server: Message forwarded successfully`);
      
    } catch (error) {
      console.error(`❌ Grid Routing Server: Failed to forward message:`, error);
      throw error;
    }
  }

  /**
   * Broadcast message via UDP multicast
   */
  protected async broadcastMessageToTransport(message: GridMessage): Promise<void> {
    console.log(`📡 Grid Routing Server: Broadcasting ${message.type} via UDP multicast`);
    
    try {
      const messageBuffer = this.serializeMessage(message);
      await this.transport.sendMulticastMessage(messageBuffer);
      
      console.log(`✅ Grid Routing Server: Broadcast sent successfully`);
      
    } catch (error) {
      console.error(`❌ Grid Routing Server: Failed to broadcast message:`, error);
      throw error;
    }
  }

  // REMOVED: waitForCommandResponse - Transport layer doesn't handle command execution

  /**
   * Generate UUID for messages using Node.js crypto
   */
  protected generateMessageId(): UUID {
    // Use crypto.randomUUID() in Node.js environment
    return require('crypto').randomUUID();
  }

  // REMOVED: handleCommandResponse - Transport layer routes messages, doesn't handle command responses

  /**
   * Clean up server-specific resources
   */
  async cleanup(): Promise<void> {
    console.log(`🧹 Grid Routing Server: Cleaning up server transport resources`);
    
    // Server-specific transport cleanup would go here
    
    // Call parent cleanup
    await super.cleanup();
  }

  /**
   * PRIVATE SERVER-SPECIFIC METHODS
   */

  /**
   * Set up UDP transport message handlers
   */
  private setupTransportHandlers(): void {
    console.log(`🔧 Grid Routing Server: Setting up UDP transport handlers`);
    
    // Handle incoming messages from transport
    this.transport.setMessageHandler(async (messageBuffer, remoteInfo) => {
      try {
        const message = this.deserializeMessage(messageBuffer);
        await this.handleIncomingMessage(message);
      } catch (error) {
        console.error(`❌ Grid Routing Server: Failed to process incoming message:`, error);
      }
    });
  }

  /**
   * Serialize Grid message to Buffer for UDP transport
   */
  private serializeMessage(message: GridMessage): Buffer {
    try {
      const messageJson = JSON.stringify(message);
      return Buffer.from(messageJson, 'utf8');
    } catch (error) {
      console.error(`❌ Grid Routing Server: Failed to serialize message:`, error);
      throw error;
    }
  }

  /**
   * Deserialize Buffer to Grid message
   */
  private deserializeMessage(messageBuffer: Buffer): GridMessage {
    try {
      const messageJson = messageBuffer.toString('utf8');
      return JSON.parse(messageJson) as GridMessage;
    } catch (error) {
      console.error(`❌ Grid Routing Server: Failed to deserialize message:`, error);
      throw error;
    }
  }

  /**
   * Create local node info from system information
   */
  static createLocalNode(
    nodeId: UUID,
    endpoints: { multicastAddress: string; multicastPort: number; unicastPort: number }
  ): GridNode {
    const os = require('os');
    
    const identity: GridNodeIdentity = {
      nodeId,
      nodeType: 'server',
      hostname: os.hostname(),
      version: '1.0.0',
      capabilities: [
        'command-execution',
        'file-operations',
        'compilation',
        'screenshot',
        'routing'
      ]
    };

    const metadata: GridNodeMetadata = {
      platform: `${os.type()} ${os.release()} (${os.arch()})`,
      region: 'local',
      loadLevel: 0.1,
      reliability: 1.0,
      latency: 1,
      lastSeen: new Date().toISOString()
    };

    const status: GridNodeStatus = {
      isOnline: true,
      isReachable: true,
      connectionCount: 0,
      uptimeSeconds: Math.floor(os.uptime()),
      memoryUsage: Math.round(process.memoryUsage().rss / 1024 / 1024)
    };

    return {
      identity,
      endpoints,
      metadata,
      status
    };
  }
}

/**
 * Factory function for creating configured Grid routing service
 */
export async function createGridRoutingServiceServer(
  nodeId: UUID,
  transport: UDPMulticastTransportServer,
  overrideConfig?: Partial<GridRoutingConfig>
): Promise<GridRoutingServiceServer> {
  console.log(`🏭 Grid Routing Server: Creating service for node ${nodeId.substring(0, 8)}`);
  
  // Create local node
  const localNode = GridRoutingServiceServer.createLocalNode(nodeId, {
    multicastAddress: transport.getConfig().multicastAddress,
    multicastPort: transport.getConfig().multicastPort,
    unicastPort: transport.getConfig().unicastPort
  });
  
  // Create configuration
  const config: GridRoutingConfig = {
    nodeId,
    nodeType: 'server',
    announceInterval: 30000,
    heartbeatInterval: 15000,
    routingUpdateInterval: 60000,
    maxHops: 8,
    nodeTimeout: 90000,
    maxRoutingTableSize: 1000,
    enableLogging: true,
    ...overrideConfig
  };
  
  // Create and initialize service
  const service = new GridRoutingServiceServer(config, localNode, transport);
  
  console.log(`✅ Grid Routing Server: Service created successfully`);
  return service;
}