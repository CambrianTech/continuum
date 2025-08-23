/**
 * UDP Multicast Transport Base
 * 
 * Shared base implementation for UDP multicast P2P networking.
 * Handles node discovery, mesh routing, and P2P message delivery.
 */

import type { JTAGMessage, JTAGPayload } from '../../../core/types/JTAGTypes';
import type { JTAGTransport, TransportSendResult } from '../../shared/TransportTypes';
import { TransportBase } from '../../shared/TransportBase';
import { TRANSPORT_EVENTS } from '../../shared/TransportEvents';
import type { 
  P2PNodeInfo, 
  DiscoveryMessage, 
  DiscoveryMessageType,
  P2PMessage,
  UDPMulticastConfig,
  NetworkTopology,
  UDPTransportStats
} from './UDPMulticastTypes';
import { 
  UDP_MULTICAST_DEFAULTS, 
  PROTOCOL_MAGIC,
  DiscoveryMessageType as MsgType,
  NodeType,
  NodeCapability
} from './UDPMulticastTypes';
import { generateUUID } from '../../../core/types/CrossPlatformUUID';

/**
 * Abstract base class for UDP multicast transport
 * Contains shared P2P logic for both server and browser environments
 */
export abstract class UDPMulticastTransportBase extends TransportBase implements JTAGTransport {
  public readonly name = 'udp-multicast-p2p';
  
  protected config: UDPMulticastConfig;
  protected nodes: Map<string, P2PNodeInfo> = new Map();
  protected messageHandler?: (message: JTAGMessage) => void;
  protected discoveryInterval?: NodeJS.Timeout;
  protected heartbeatInterval?: NodeJS.Timeout;
  protected cleanupInterval?: NodeJS.Timeout;
  protected stats: UDPTransportStats;
  // Remove _isConnected since TransportBase.connected provides this
  
  constructor(config: Partial<UDPMulticastConfig>) {
    super();
    
    this.config = {
      nodeId: config.nodeId || generateUUID(),
      nodeType: config.nodeType || NodeType.SERVER,
      capabilities: config.capabilities || [NodeCapability.FILE_OPERATIONS],
      multicastAddress: config.multicastAddress || UDP_MULTICAST_DEFAULTS.MULTICAST_ADDRESS,
      multicastPort: config.multicastPort || UDP_MULTICAST_DEFAULTS.MULTICAST_PORT,
      unicastPort: config.unicastPort || (9000 + Math.floor(Math.random() * 1000)),
      ttl: config.ttl || UDP_MULTICAST_DEFAULTS.TTL,
      discoveryInterval: config.discoveryInterval || UDP_MULTICAST_DEFAULTS.DISCOVERY_INTERVAL,
      heartbeatInterval: config.heartbeatInterval || UDP_MULTICAST_DEFAULTS.HEARTBEAT_INTERVAL,
      nodeTimeout: config.nodeTimeout || UDP_MULTICAST_DEFAULTS.NODE_TIMEOUT,
      encryptionKey: config.encryptionKey
    };
    
    this.stats = {
      messagesRx: 0,
      messagesTx: 0,
      bytesRx: 0,
      bytesTx: 0,
      nodesDiscovered: 0,
      activeNodes: 0,
      lastActivity: new Date().toISOString()
    };
    
    console.log(`🌐 P2P Transport: Initializing ${this.config.nodeType} node ${this.config.nodeId.substring(0, 8)}`);
    console.log(`📡 P2P Config: ${this.config.multicastAddress}:${this.config.multicastPort} (unicast: ${this.config.unicastPort})`);
  }

  /**
   * Abstract methods for environment-specific implementation
   */
  protected abstract initializeMulticastSocket(): Promise<void>;
  protected abstract initializeUnicastSocket(): Promise<void>;
  protected abstract sendMulticastMessage(message: Buffer): Promise<void>;
  protected abstract sendUnicastMessage(targetPort: number, message: Buffer): Promise<void>;
  protected abstract cleanup(): Promise<void>;

  /**
   * Initialize P2P transport
   */
  async initialize(): Promise<void> {
    // Check if already initialized
    if (this.connected) {
      console.log(`⚡ P2P Transport: Already initialized for ${this.config.nodeType} node ${this.config.nodeId.substring(0, 8)}`);
      return;
    }
    
    console.log(`🚀 P2P Transport: Starting mesh networking for ${this.config.nodeType}`);
    
    try {
      // Initialize network sockets
      await this.initializeMulticastSocket();
      await this.initializeUnicastSocket();
      
      // Start discovery and maintenance processes
      this.startDiscoveryProcess();
      this.startHeartbeatProcess(); 
      this.startNodeCleanup();
      
      this.connected = true;
      console.log(`✅ P2P Transport: Mesh networking active (${this.config.nodeId.substring(0, 8)})`);
      
      // Announce this node to the network
      await this.announceNode();
      
    } catch (error: any) {
      console.error('❌ P2P Transport: Failed to initialize:', error.message);
      throw error;
    }
  }

  /**
   * Send JTAG message via P2P network
   */
  async send(message: JTAGMessage): Promise<TransportSendResult> {
    if (!this.connected) {
      return this.createResult(false);
    }

    try {
      const p2pMessage: P2PMessage = {
        ...message,
        p2p: {
          sourceNodeId: this.config.nodeId,
          targetNodeId: this.extractTargetNodeId(message),
          routingPath: [this.config.nodeId],
          hops: 0,
          maxHops: UDP_MULTICAST_DEFAULTS.MAX_HOPS
        }
      } as P2PMessage;

      const messageBuffer = this.serializeMessage(PROTOCOL_MAGIC.P2P_MESSAGE, p2pMessage);
      
      if (p2pMessage.p2p.targetNodeId) {
        // Unicast to specific node
        const targetNode = this.nodes.get(p2pMessage.p2p.targetNodeId);
        if (targetNode) {
          await this.sendUnicastMessage(targetNode.endpoints.unicastPort, messageBuffer);
          this.updateStats('tx', messageBuffer.length);
          return this.createResult(true, 1);
        }
      } else {
        // Multicast broadcast
        await this.sendMulticastMessage(messageBuffer);
        this.updateStats('tx', messageBuffer.length);
        return this.createResult(true, this.nodes.size);
      }
      
      return this.createResult(false);
      
    } catch (error: any) {
      console.error('❌ P2P Transport: Send failed:', error.message);
      return this.createResult(false);
    }
  }

  // isConnected() and setMessageHandler() inherited from TransportBase

  /**
   * Disconnect from P2P network
   */
  async disconnect(): Promise<void> {
    console.log(`🔌 P2P Transport: Disconnecting node ${this.config.nodeId.substring(0, 8)}`);
    
    // Send goodbye message
    await this.sendGoodbyeMessage();
    
    // Clear intervals
    if (this.discoveryInterval) clearInterval(this.discoveryInterval);
    if (this.heartbeatInterval) clearInterval(this.heartbeatInterval);
    if (this.cleanupInterval) clearInterval(this.cleanupInterval);
    
    // Environment-specific cleanup
    await this.cleanup();
    
    this.connected = false;
    this.nodes.clear();
    
    console.log(`✅ P2P Transport: Node disconnected`);
  }

  /**
   * Get current network topology
   */
  getNetworkTopology(): NetworkTopology {
    const nodes: Record<string, P2PNodeInfo> = {};
    const routes: Record<string, string[]> = {};
    
    this.nodes.forEach((node, nodeId) => {
      nodes[nodeId] = node;
      routes[nodeId] = [nodeId]; // Direct route for now - can implement multi-hop later
    });
    
    return {
      nodes,
      routes,
      lastUpdated: new Date().toISOString()
    };
  }

  /**
   * Get transport statistics
   */
  getStats(): UDPTransportStats {
    return {
      ...this.stats,
      activeNodes: this.nodes.size
    };
  }

  /**
   * Handle incoming raw UDP message from network
   */
  protected handleIncomingUDPMessage(messageBuffer: Buffer, remoteInfo: { address: string; port: number }): void {
    try {
      console.log(`📨 P2P Transport: Received message from ${remoteInfo.address}:${remoteInfo.port} (${messageBuffer.length} bytes)`);
      const message = this.deserializeMessage(messageBuffer);
      this.updateStats('rx', messageBuffer.length);
      
      if (message.magic === PROTOCOL_MAGIC.DISCOVERY) {
        console.log(`🔍 P2P Discovery: Processing discovery message type ${(message.payload as any).type}`);
        this.handleDiscoveryMessage(message.payload as DiscoveryMessage);
      } else if (message.magic === PROTOCOL_MAGIC.P2P_MESSAGE) {
        console.log(`📬 P2P Message: Processing P2P command message`);
        this.handleP2PMessage(message.payload as P2PMessage);
      }
      
    } catch (error: any) {
      console.warn(`⚠️ P2P Transport: Failed to process message from ${remoteInfo.address}:${remoteInfo.port}:`, error.message);
    }
  }

  /**
   * Start periodic node discovery
   */
  private startDiscoveryProcess(): void {
    this.discoveryInterval = setInterval(async () => {
      try {
        await this.announceNode();
      } catch (error: any) {
        console.warn('⚠️ P2P Discovery: Announcement failed:', error.message);
      }
    }, this.config.discoveryInterval);
  }

  /**
   * Start periodic heartbeat
   */
  private startHeartbeatProcess(): void {
    this.heartbeatInterval = setInterval(async () => {
      try {
        await this.sendHeartbeat();
      } catch (error: any) {
        console.warn('⚠️ P2P Heartbeat: Failed:', error.message);
      }
    }, this.config.heartbeatInterval);
  }

  /**
   * Start periodic cleanup of stale nodes
   */
  private startNodeCleanup(): void {
    this.cleanupInterval = setInterval(() => {
      const now = Date.now();
      const timeoutMs = this.config.nodeTimeout;
      
      for (const [nodeId, node] of this.nodes.entries()) {
        const lastSeen = new Date(node.metadata.lastSeen).getTime();
        if (now - lastSeen > timeoutMs) {
          console.log(`🧹 P2P Transport: Removing stale node ${nodeId.substring(0, 8)}`);
          this.nodes.delete(nodeId);
          this.emitTransportEvent('DISCONNECTED', { 
            nodeId: nodeId,
            reason: 'timeout' 
          });
        }
      }
    }, this.config.nodeTimeout / 2);
  }

  /**
   * Announce this node to the network
   */
  private async announceNode(): Promise<void> {
    const announcement: DiscoveryMessage = {
      type: MsgType.NODE_ANNOUNCE,
      nodeInfo: this.getNodeInfo(),
      timestamp: new Date().toISOString(),
      messageId: generateUUID()
    };

    const messageBuffer = this.serializeMessage(PROTOCOL_MAGIC.DISCOVERY, announcement);
    console.log(`📡 P2P Discovery: Broadcasting announcement from ${this.config.nodeId.substring(0, 8)} (${messageBuffer.length} bytes)`);
    await this.sendMulticastMessage(messageBuffer);
    console.log(`✅ P2P Discovery: Announcement sent successfully`);
  }

  /**
   * Send heartbeat to maintain presence
   */
  private async sendHeartbeat(): Promise<void> {
    const heartbeat: DiscoveryMessage = {
      type: MsgType.NODE_HEARTBEAT,
      nodeInfo: this.getNodeInfo(),
      timestamp: new Date().toISOString(),
      messageId: generateUUID()
    };

    const messageBuffer = this.serializeMessage(PROTOCOL_MAGIC.DISCOVERY, heartbeat);
    await this.sendMulticastMessage(messageBuffer);
  }

  /**
   * Send goodbye message when disconnecting
   */
  private async sendGoodbyeMessage(): Promise<void> {
    const goodbye: DiscoveryMessage = {
      type: MsgType.NODE_GOODBYE,
      nodeInfo: this.getNodeInfo(),
      timestamp: new Date().toISOString(),
      messageId: generateUUID()
    };

    const messageBuffer = this.serializeMessage(PROTOCOL_MAGIC.DISCOVERY, goodbye);
    await this.sendMulticastMessage(messageBuffer);
  }

  /**
   * Handle discovery protocol messages
   */
  private handleDiscoveryMessage(message: DiscoveryMessage): void {
    const { type, nodeInfo } = message;
    
    // Ignore messages from self
    if (nodeInfo.nodeId === this.config.nodeId) return;
    
    switch (type) {
      case MsgType.NODE_ANNOUNCE:
      case MsgType.NODE_HEARTBEAT:
        this.updateNode(nodeInfo);
        console.log(`📡 P2P Discovery: ${type === MsgType.NODE_ANNOUNCE ? 'Discovered' : 'Heartbeat from'} ${nodeInfo.nodeType} node ${nodeInfo.nodeId.substring(0, 8)}`);
        break;
        
      case MsgType.NODE_GOODBYE:
        this.removeNode(nodeInfo.nodeId);
        console.log(`👋 P2P Discovery: Node ${nodeInfo.nodeId.substring(0, 8)} left network`);
        break;
        
      case MsgType.NODE_QUERY:
        // Respond with our node info
        this.announceNode();
        break;
    }
  }

  /**
   * Handle P2P JTAG messages
   */
  private handleP2PMessage(message: P2PMessage): void {
    // Check if message is for us or should be forwarded
    if (message.p2p.targetNodeId && message.p2p.targetNodeId !== this.config.nodeId) {
      // Forward to target node (multi-hop routing)
      this.forwardMessage(message);
      return;
    }
    
    // Message is for us - pass to message handler
    if (this.messageHandler) {
      // P2PMessage extends JTAGMessage, so we can cast it directly
      // The P2P metadata is additional and doesn't interfere with the base message
      this.messageHandler(message as JTAGMessage);
    }
  }

  /**
   * Forward message to target node (multi-hop routing)
   */
  private async forwardMessage(message: P2PMessage): Promise<void> {
    if (message.p2p.hops >= message.p2p.maxHops) {
      console.warn(`⚠️ P2P Routing: Message exceeded max hops (${message.p2p.hops})`);
      return;
    }
    
    const targetNodeId = message.p2p.targetNodeId!;
    const targetNode = this.nodes.get(targetNodeId);
    
    if (!targetNode) {
      console.warn(`⚠️ P2P Routing: Unknown target node ${targetNodeId.substring(0, 8)}`);
      return;
    }
    
    // Update routing information
    const forwardedMessage: P2PMessage = {
      ...message,
      p2p: {
        ...message.p2p,
        routingPath: [...message.p2p.routingPath, this.config.nodeId],
        hops: message.p2p.hops + 1
      }
    };
    
    const messageBuffer = this.serializeMessage(PROTOCOL_MAGIC.P2P_MESSAGE, forwardedMessage);
    await this.sendUnicastMessage(targetNode.endpoints.unicastPort, messageBuffer);
    
    console.log(`🔀 P2P Routing: Forwarded message to ${targetNodeId.substring(0, 8)} (hop ${forwardedMessage.p2p.hops})`);
  }

  /**
   * Update node information
   */
  private updateNode(nodeInfo: P2PNodeInfo): void {
    const wasNew = !this.nodes.has(nodeInfo.nodeId);
    this.nodes.set(nodeInfo.nodeId, nodeInfo);
    
    if (wasNew) {
      this.stats.nodesDiscovered++;
      this.emitTransportEvent('CONNECTED', { 
        nodeId: nodeInfo.nodeId, 
        nodeType: nodeInfo.nodeType,
        capabilities: nodeInfo.capabilities 
      });
    }
  }

  /**
   * Remove node from network
   */
  private removeNode(nodeId: string): void {
    const node = this.nodes.get(nodeId);
    if (node) {
      this.nodes.delete(nodeId);
      this.emitTransportEvent('DISCONNECTED', { 
        nodeId: nodeId,
        reason: 'node_left' 
      });
    }
  }

  /**
   * Get this node's information
   */
  private getNodeInfo(): P2PNodeInfo {
    return {
      nodeId: this.config.nodeId,
      nodeType: this.config.nodeType,
      capabilities: this.config.capabilities,
      endpoints: {
        unicastPort: this.config.unicastPort,
        multicastAddress: this.config.multicastAddress,
        multicastPort: this.config.multicastPort
      },
      metadata: {
        version: '1.0.0', // TODO: Get from package.json
        hostname: this.getHostname(),
        platform: this.getPlatform(),
        lastSeen: new Date().toISOString()
      }
    };
  }

  /**
   * Extract target node ID from JTAG message
   */
  private extractTargetNodeId(message: JTAGMessage): string | undefined {
    // Check if message has remote routing information
    if (message.endpoint?.startsWith('remote/')) {
      const parts = message.endpoint.split('/');
      if (parts.length >= 2) {
        return parts[1]; // remote/{nodeId}/daemon/command
      }
    }
    return undefined; // Broadcast message
  }

  /**
   * Serialize message with protocol magic
   */
  private serializeMessage(magic: string, payload: unknown): Buffer {
    const message = { magic, payload };
    return Buffer.from(JSON.stringify(message), 'utf-8');
  }

  /**
   * Deserialize message and validate protocol magic
   */
  private deserializeMessage(buffer: Buffer): { magic: string; payload: unknown } {
    const messageStr = buffer.toString('utf-8');
    const message = JSON.parse(messageStr);
    
    if (!message.magic || !Object.values(PROTOCOL_MAGIC).includes(message.magic)) {
      throw new Error('Invalid protocol magic');
    }
    
    return message;
  }

  /**
   * Update transport statistics
   */
  private updateStats(direction: 'rx' | 'tx', bytes: number): void {
    if (direction === 'rx') {
      this.stats.messagesRx++;
      this.stats.bytesRx += bytes;
    } else {
      this.stats.messagesTx++;
      this.stats.bytesTx += bytes;
    }
    this.stats.lastActivity = new Date().toISOString();
  }

  /**
   * Environment-specific implementations
   */
  protected abstract getHostname(): string;
  protected abstract getPlatform(): string;

  /**
   * Emit transport events using eventSystem (following WebSocket pattern)
   */
  protected emitTransportEvent(eventType: keyof typeof TRANSPORT_EVENTS, data: any): void {
    if (this.eventSystem) {
      this.eventSystem.emit(TRANSPORT_EVENTS[eventType], {
        transportType: 'udp-multicast' as const,
        ...data
      });
    }
  }
}