/**
 * Grid Routing Service
 * 
 * Core routing and discovery service for The Grid P2P mesh network.
 * Handles node discovery, routing table management, and message forwarding.
 * 
 * This is the fundamental transport layer for The Grid backbone - built
 * step-by-step with proper validation and testing.
 */

import type {
  GridNode,
  GridNodeIdentity,
  GridNodeEndpoints,
  GridNodeMetadata,
  GridNodeStatus,
  RoutingEntry,
  RoutingTable,
  GridMessage,
  GridMessageType,
  NodeAnnounceMessage,
  NodeHeartbeatMessage,
  RoutingUpdateMessage,
  ForwardMessage,
  MessageRoute,
  GridTopology,
  NodeDiscoveryQuery,
  NodeDiscoveryResult,
  GridRoutingConfig,
  GridError,
  GridResult,
  GridEventType,
  GridEvent,
  ExecutionContext,
  GRID_ROUTING_DEFAULTS
} from './GridRoutingTypes';

import type { UUID } from '../../../core/types/CrossPlatformUUID';

/**
 * Abstract base class for Grid routing service
 * 
 * Follows sparse override pattern:
 * - 80-90% of routing logic here in shared base
 * - 5-10% environment-specific overrides (browser/server)
 */
export abstract class GridRoutingService {
  protected nodes: Map<UUID, GridNode> = new Map();
  protected routingTable: Map<UUID, RoutingEntry> = new Map();
  protected pendingMessages: Map<UUID, PendingMessage> = new Map();
  protected eventHandlers: Map<GridEventType, Set<(event: GridEvent) => void>> = new Map();
  protected initialized = false;
  
  constructor(
    protected config: GridRoutingConfig,
    protected localNode: GridNode
  ) {
    this.initializeEventHandlers();
  }

  /**
   * Initialize the Grid routing service
   */
  async initialize(): Promise<GridResult<void>> {
    if (this.initialized) {
      console.log(`⚡ Grid Routing: Already initialized for node ${this.config.nodeId.substring(0, 8)}`);
      return { success: true, data: undefined };
    }

    try {
      console.log(`🌐 Grid Routing: Initializing service for node ${this.localNode.identity.nodeId.substring(0, 8)}`);
      
      // Register local node
      this.nodes.set(this.localNode.identity.nodeId, this.localNode);
      
      // Start periodic processes
      this.startPeriodicProcesses();
      
      // Send initial node announcement
      await this.announceNode();
      
      this.initialized = true;
      console.log(`✅ Grid Routing: Service initialized`);
      
      this.emitEvent({
        type: GridEventType.NODE_JOINED,
        timestamp: new Date().toISOString(),
        nodeId: this.localNode.identity.nodeId,
        data: this.localNode
      });

      return { success: true, data: undefined };
      
    } catch (error) {
      const gridError: GridError = {
        type: 'network-error' as any,
        message: `Failed to initialize Grid routing: ${(error as Error).message}`,
        nodeId: this.config.nodeId
      };
      
      return { success: false, error: gridError };
    }
  }

  /**
   * Discover nodes matching query criteria
   */
  async discoverNodes(query: NodeDiscoveryQuery): Promise<GridResult<NodeDiscoveryResult[]>> {
    console.log(`🔍 Grid Routing: Discovering nodes with query:`, query);
    
    try {
      const results: NodeDiscoveryResult[] = [];
      
      // Search local routing table
      for (const [nodeId, node] of this.nodes) {
        if (nodeId === this.localNode.identity.nodeId) continue;
        
        const matchScore = this.calculateMatchScore(node, query);
        if (matchScore > 0) {
          const route = this.routingTable.get(nodeId);
          if (route) {
            results.push({
              node,
              route,
              matchScore
            });
          }
        }
      }
      
      // Sort by match score (best matches first)
      results.sort((a, b) => b.matchScore - a.matchScore);
      
      console.log(`📡 Grid Routing: Found ${results.length} matching nodes`);
      return { success: true, data: results };
      
    } catch (error) {
      const gridError: GridError = {
        type: 'network-error' as any,
        message: `Node discovery failed: ${(error as Error).message}`
      };
      
      return { success: false, error: gridError };
    }
  }

  /**
   * Send a message to a specific node
   */
  async sendMessage(targetNodeId: UUID, message: GridMessage): Promise<GridResult<void>> {
    console.log(`📤 Grid Routing: Sending ${message.type} to node ${targetNodeId.substring(0, 8)}`);
    
    try {
      // Check if target is local node
      if (targetNodeId === this.localNode.identity.nodeId) {
        console.log(`📨 Grid Routing: Message for local node - processing directly`);
        await this.handleIncomingMessage(message);
        return { success: true, data: undefined };
      }
      
      // Find route to target
      const route = this.routingTable.get(targetNodeId);
      if (!route) {
        const gridError: GridError = {
          type: 'route-not-found' as any,
          message: `No route found to node ${targetNodeId}`,
          nodeId: targetNodeId
        };
        return { success: false, error: gridError };
      }
      
      // Create message route
      const messageRoute: MessageRoute = {
        messageId: message.messageId,
        originalSource: this.localNode.identity.nodeId,
        finalTarget: targetNodeId,
        currentHop: this.localNode.identity.nodeId,
        nextHop: route.nextHopNodeId,
        hopCount: 0,
        maxHops: message.ttl,
        routingPath: [this.localNode.identity.nodeId]
      };
      
      // Track pending message
      this.trackPendingMessage(message, messageRoute);
      
      // Forward message to next hop
      await this.forwardMessage(message, messageRoute);
      
      return { success: true, data: undefined };
      
    } catch (error) {
      const gridError: GridError = {
        type: 'network-error' as any,
        message: `Failed to send message: ${(error as Error).message}`,
        messageId: message.messageId
      };
      
      return { success: false, error: gridError };
    }
  }

  /**
   * Broadcast message to all known nodes
   */
  async broadcastMessage(message: GridMessage): Promise<GridResult<void>> {
    console.log(`📡 Grid Routing: Broadcasting ${message.type} to ${this.nodes.size - 1} nodes`);
    
    try {
      // Send to all nodes except self
      const promises: Promise<GridResult<void>>[] = [];
      
      for (const nodeId of this.nodes.keys()) {
        if (nodeId !== this.localNode.identity.nodeId) {
          promises.push(this.sendMessage(nodeId, message));
        }
      }
      
      await Promise.all(promises);
      return { success: true, data: undefined };
      
    } catch (error) {
      const gridError: GridError = {
        type: 'network-error' as any,
        message: `Broadcast failed: ${(error as Error).message}`,
        messageId: message.messageId
      };
      
      return { success: false, error: gridError };
    }
  }

  // REMOVED: executeRemoteCommand - Router connects Continuum environments to greater Grid, routes messages agnostically

  /**
   * Get current Grid topology
   */
  getTopology(): GridTopology {
    const routingTables = new Map<UUID, RoutingTable>();
    const connections = new Map<UUID, UUID[]>();
    
    // Build routing tables for each known node
    for (const nodeId of this.nodes.keys()) {
      const nodeRoutingTable: Map<UUID, RoutingEntry> = new Map();
      
      // Add routing entries for this node
      for (const [targetId, entry] of this.routingTable) {
        if (entry.nextHopNodeId === nodeId || targetId === nodeId) {
          nodeRoutingTable.set(targetId, entry);
        }
      }
      
      routingTables.set(nodeId, {
        localNodeId: nodeId,
        entries: nodeRoutingTable,
        lastUpdated: new Date().toISOString(),
        version: 1
      });
      
      // Build connections map
      const nodeConnections: UUID[] = [];
      for (const entry of this.routingTable.values()) {
        if (entry.distance === 1) { // Direct connections
          nodeConnections.push(entry.targetNodeId);
        }
      }
      connections.set(nodeId, nodeConnections);
    }
    
    return {
      nodes: new Map(this.nodes),
      routingTables,
      connections,
      lastUpdated: new Date().toISOString(),
      version: 1
    };
  }

  /**
   * Add event listener
   */
  addEventListener(eventType: GridEventType, handler: (event: GridEvent) => void): void {
    if (!this.eventHandlers.has(eventType)) {
      this.eventHandlers.set(eventType, new Set());
    }
    this.eventHandlers.get(eventType)!.add(handler);
  }

  /**
   * Remove event listener
   */
  removeEventListener(eventType: GridEventType, handler: (event: GridEvent) => void): void {
    const handlers = this.eventHandlers.get(eventType);
    if (handlers) {
      handlers.delete(handler);
    }
  }

  /**
   * Clean up resources
   */
  async cleanup(): Promise<void> {
    console.log(`🧹 Grid Routing: Cleaning up service for node ${this.config.nodeId.substring(0, 8)}`);
    
    // Send goodbye message
    await this.sendGoodbyeMessage();
    
    // Clear all data structures
    this.nodes.clear();
    this.routingTable.clear();
    this.pendingMessages.clear();
    this.eventHandlers.clear();
    
    this.initialized = false;
    console.log(`✅ Grid Routing: Cleanup complete`);
  }

  /**
   * PROTECTED METHODS - Override in browser/server implementations
   */

  /**
   * Send node announcement (environment-specific implementation)
   */
  protected async announceNode(): Promise<void> {
    console.log(`📡 Grid Routing: Announcing node ${this.localNode.identity.nodeId.substring(0, 8)}`);
    
    const announcement: NodeAnnounceMessage = {
      messageId: this.generateMessageId(),
      type: GridMessageType.NODE_ANNOUNCE,
      sourceNodeId: this.localNode.identity.nodeId,
      timestamp: new Date().toISOString(),
      ttl: GRID_ROUTING_DEFAULTS.MAX_TTL,
      priority: GRID_ROUTING_DEFAULTS.DEFAULT_PRIORITY,
      payload: {
        node: this.localNode,
        routingTable: Array.from(this.routingTable.values())
      }
    };
    
    // Environment-specific broadcast implementation
    await this.broadcastMessageToTransport(announcement);
  }

  /**
   * Handle incoming Grid message
   */
  protected async handleIncomingMessage(message: GridMessage): Promise<void> {
    console.log(`📨 Grid Routing: Received ${message.type} from node ${message.sourceNodeId.substring(0, 8)}`);
    
    switch (message.type) {
      case GridMessageType.NODE_ANNOUNCE:
        await this.handleNodeAnnouncement(message as NodeAnnounceMessage);
        break;
        
      case GridMessageType.NODE_HEARTBEAT:
        await this.handleNodeHeartbeat(message as NodeHeartbeatMessage);
        break;
        
      case GridMessageType.ROUTING_UPDATE:
        await this.handleRoutingUpdate(message as RoutingUpdateMessage);
        break;
        
      case GridMessageType.FORWARD_MESSAGE:
        // Dumb pipe - forward payload without understanding it
        await this.forwardMessage(message as ForwardMessage);
        break;
        
      default:
        console.warn(`🤷 Grid Routing: Unknown message type: ${message.type}`);
    }
    
    this.emitEvent({
      type: GridEventType.MESSAGE_RECEIVED,
      timestamp: new Date().toISOString(),
      nodeId: message.sourceNodeId,
      data: message
    });
  }

  /**
   * Forward message to next hop (environment-specific implementation)
   */
  protected abstract forwardMessage(message: GridMessage, route: MessageRoute): Promise<void>;

  /**
   * Broadcast message via transport layer (environment-specific implementation)
   */
  protected abstract broadcastMessageToTransport(message: GridMessage): Promise<void>;


  /**
   * Generate unique message ID (environment-specific implementation)
   */
  protected abstract generateMessageId(): UUID;

  /**
   * PRIVATE METHODS - Internal implementation
   */

  private async handleNodeAnnouncement(message: NodeAnnounceMessage): Promise<void> {
    const node = message.payload.node;
    const nodeId = node.identity.nodeId;
    
    console.log(`📡 Grid Routing: Node announcement from ${nodeId.substring(0, 8)}`);
    
    // Update node registry
    this.nodes.set(nodeId, node);
    
    // Add direct route to announcing node
    const directRoute: RoutingEntry = {
      targetNodeId: nodeId,
      nextHopNodeId: nodeId,
      distance: 1,
      cost: 1,
      path: [this.localNode.identity.nodeId, nodeId],
      updatedAt: new Date().toISOString(),
      reliability: node.metadata.reliability
    };
    
    this.routingTable.set(nodeId, directRoute);
    
    // Process shared routing table
    for (const sharedRoute of message.payload.routingTable) {
      this.considerRoutingUpdate(sharedRoute, nodeId);
    }
    
    this.emitEvent({
      type: GridEventType.NODE_JOINED,
      timestamp: new Date().toISOString(),
      nodeId,
      data: node
    });
  }

  private async handleNodeHeartbeat(message: NodeHeartbeatMessage): Promise<void> {
    const nodeId = message.sourceNodeId;
    const existingNode = this.nodes.get(nodeId);
    
    if (existingNode) {
      // Update node status
      const updatedNode: GridNode = {
        ...existingNode,
        status: message.payload.status,
        metadata: {
          ...existingNode.metadata,
          lastSeen: message.timestamp
        }
      };
      
      this.nodes.set(nodeId, updatedNode);
      
      // Process routing updates
      for (const routingUpdate of message.payload.routingUpdates) {
        this.considerRoutingUpdate(routingUpdate, nodeId);
      }
    }
  }

  private async handleRoutingUpdate(message: RoutingUpdateMessage): Promise<void> {
    const fromNodeId = message.sourceNodeId;
    
    console.log(`🗺️ Grid Routing: Routing update from ${fromNodeId.substring(0, 8)}`);
    
    // Process route updates
    for (const update of message.payload.updates) {
      this.considerRoutingUpdate(update, fromNodeId);
    }
    
    // Process route removals
    for (const removedNodeId of message.payload.removals) {
      this.removeRoute(removedNodeId);
    }
  }

  /**
   * Forward generic message - Router is agnostic but intelligent
   * Handles messages, payloads, events, promises, broadcasts generically
   * Deterministic rules-based routing without understanding payload content
   */
  private async forwardMessage(message: ForwardMessage): Promise<void> {
    console.log(`🔄 Grid Routing: Forwarding message ${message.messageId.substring(0, 8)} from ${message.sourceNodeId.substring(0, 8)}`);
    
    if (message.targetNodeId) {
      // Direct message routing - deterministic delivery to specific node
      const route = this.findRoute(message.targetNodeId);
      if (route) {
        await this.routeMessageViaPath(message, route);
      } else {
        console.warn(`🚫 Grid Routing: No route found for ${message.targetNodeId.substring(0, 8)}`);
      }
    } else {
      // Broadcast message - intelligent propagation without duplication
      await this.broadcastMessageToTransport(message);
    }
  }

  private considerRoutingUpdate(update: RoutingEntry, fromNodeId: UUID): void {
    const existingRoute = this.routingTable.get(update.targetNodeId);
    
    // Calculate new route via this node
    const newRoute: RoutingEntry = {
      ...update,
      nextHopNodeId: fromNodeId,
      distance: update.distance + 1,
      cost: update.cost + 1,
      path: [this.localNode.identity.nodeId, fromNodeId, ...update.path.slice(1)],
      updatedAt: new Date().toISOString()
    };
    
    // Accept route if better than existing or if no existing route
    if (!existingRoute || this.isRouteBetter(newRoute, existingRoute)) {
      console.log(`🗺️ Grid Routing: Updated route to ${update.targetNodeId.substring(0, 8)} via ${fromNodeId.substring(0, 8)}`);
      this.routingTable.set(update.targetNodeId, newRoute);
      
      this.emitEvent({
        type: GridEventType.ROUTE_DISCOVERED,
        timestamp: new Date().toISOString(),
        nodeId: update.targetNodeId,
        data: newRoute
      });
    }
  }

  private isRouteBetter(newRoute: RoutingEntry, existingRoute: RoutingEntry): boolean {
    // Prefer shorter distance
    if (newRoute.distance !== existingRoute.distance) {
      return newRoute.distance < existingRoute.distance;
    }
    
    // Prefer lower cost
    if (newRoute.cost !== existingRoute.cost) {
      return newRoute.cost < existingRoute.cost;
    }
    
    // Prefer higher reliability
    return newRoute.reliability > existingRoute.reliability;
  }

  private removeRoute(nodeId: UUID): void {
    if (this.routingTable.delete(nodeId)) {
      console.log(`🗺️ Grid Routing: Removed route to ${nodeId.substring(0, 8)}`);
      
      this.emitEvent({
        type: GridEventType.ROUTE_LOST,
        timestamp: new Date().toISOString(),
        nodeId,
        data: undefined
      });
    }
  }

  private calculateMatchScore(node: GridNode, query: NodeDiscoveryQuery): number {
    let score = 1.0;
    
    // Check node type
    if (query.nodeType && node.identity.nodeType !== query.nodeType) {
      score *= 0.5;
    }
    
    // Check capabilities
    if (query.capabilities) {
      const matchingCapabilities = query.capabilities.filter(cap => 
        node.identity.capabilities.includes(cap)
      ).length;
      score *= matchingCapabilities / query.capabilities.length;
    }
    
    // Check latency
    if (query.maxLatency && node.metadata.latency > query.maxLatency) {
      score *= 0.3;
    }
    
    // Check reliability
    if (query.minReliability && node.metadata.reliability < query.minReliability) {
      score *= 0.2;
    }
    
    return score;
  }

  private trackPendingMessage(message: GridMessage, route: MessageRoute): void {
    const pending: PendingMessage = {
      message,
      route,
      sentAt: new Date().toISOString(),
      timeoutMs: GRID_ROUTING_DEFAULTS.MESSAGE_TIMEOUT
    };
    
    this.pendingMessages.set(message.messageId, pending);
    
    // Set timeout to clean up pending message
    setTimeout(() => {
      this.pendingMessages.delete(message.messageId);
    }, pending.timeoutMs);
  }

  private startPeriodicProcesses(): void {
    // Periodic node announcements
    setInterval(() => {
      this.announceNode().catch(error => {
        console.error('❌ Grid Routing: Failed to send announcement:', error);
      });
    }, this.config.announceInterval);
    
    // Periodic heartbeats
    setInterval(() => {
      this.sendHeartbeat().catch(error => {
        console.error('❌ Grid Routing: Failed to send heartbeat:', error);
      });
    }, this.config.heartbeatInterval);
    
    // Periodic routing updates
    setInterval(() => {
      this.sendRoutingUpdate().catch(error => {
        console.error('❌ Grid Routing: Failed to send routing update:', error);
      });
    }, this.config.routingUpdateInterval);
  }

  private async sendHeartbeat(): Promise<void> {
    const heartbeat: NodeHeartbeatMessage = {
      messageId: this.generateMessageId(),
      type: GridMessageType.NODE_HEARTBEAT,
      sourceNodeId: this.localNode.identity.nodeId,
      timestamp: new Date().toISOString(),
      ttl: GRID_ROUTING_DEFAULTS.MAX_TTL,
      priority: GRID_ROUTING_DEFAULTS.DEFAULT_PRIORITY,
      payload: {
        status: this.localNode.status,
        routingUpdates: []
      }
    };
    
    await this.broadcastMessageToTransport(heartbeat);
  }

  private async sendRoutingUpdate(): Promise<void> {
    const routingUpdate: RoutingUpdateMessage = {
      messageId: this.generateMessageId(),
      type: GridMessageType.ROUTING_UPDATE,
      sourceNodeId: this.localNode.identity.nodeId,
      timestamp: new Date().toISOString(),
      ttl: GRID_ROUTING_DEFAULTS.MAX_TTL,
      priority: GRID_ROUTING_DEFAULTS.DEFAULT_PRIORITY,
      payload: {
        updates: Array.from(this.routingTable.values()),
        removals: [],
        fullTable: false
      }
    };
    
    await this.broadcastMessageToTransport(routingUpdate);
  }

  private async sendGoodbyeMessage(): Promise<void> {
    // Implementation would send goodbye message before cleanup
    console.log(`👋 Grid Routing: Sending goodbye message`);
  }

  private initializeEventHandlers(): void {
    // Initialize event handler sets for all event types
    for (const eventType of Object.values(GridEventType)) {
      this.eventHandlers.set(eventType, new Set());
    }
  }

  private emitEvent(event: GridEvent): void {
    const handlers = this.eventHandlers.get(event.type);
    if (handlers) {
      for (const handler of handlers) {
        try {
          handler(event);
        } catch (error) {
          console.error(`❌ Grid Routing: Event handler error for ${event.type}:`, error);
        }
      }
    }
  }
}

/**
 * Internal types for tracking state
 */
interface PendingMessage {
  readonly message: GridMessage;
  readonly route: MessageRoute;
  readonly sentAt: string;
  readonly timeoutMs: number;
}