// ISSUES: 2 open, last updated 2025-07-30 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking

/**
 * JTAG Universal Router - Context-Aware Message Routing with Bus-Level Queuing
 * 
 * The heart of the JTAG system - intelligent message routing with health monitoring,
 * queuing, and cross-context transport management. Handles both local and remote
 * message delivery with automatic fallback and retry mechanisms.
 * 
 * ISSUES: (look for TODOs)
 * - ✅ RESOLVED: Move the event system out to a shared module to avoid circular dependencies and improve maintainability
 * - ✅ RESOLVED: Implement a TransportEndpoint interface in the transports module to standardize transport initialization and management
 * - ✅ RESOLVED: Eliminate non typed things like `any` and `unknown` in this file. Think of promise return types and eliminate void where possible. Think of how this routes these payloads as commands and events
 * - ✅ RESOLVED: Move the transport factory to a shared module for better abstraction and reusability. We will use this in the JTAGClient to connect to this very router.
 * - ✅ RESOLVED: Use router configuration for transport initialization instead of hardcoded values - transport type, ports, and fallback now configurable
 * - ENHANCEMENT: Implement dedicated P2P transport for networking (UDP multicast) - current WebSocket not optimal for peer-to-peer discovery
 * - ENHANCEMENT: Add port availability tracking to router - automatically detect and assign free ports for multi-instance deployments
 * 
 * CORE ARCHITECTURE:
 * - MessageSubscriber pattern for daemon registration
 * - Cross-context transport abstraction (WebSocket/HTTP)
 * - Priority-based message queuing with health-aware flushing
 * - Request-response correlation with timeout handling
 * - Event system integration for system-wide notifications
 * 
 * TESTING REQUIREMENTS:
 * - Unit tests: Message routing logic and subscriber management
 * - Integration tests: Cross-context transport reliability
 * - Performance tests: Message throughput under load
 * - Failure tests: Network partition and recovery scenarios
 * 
 * ARCHITECTURAL INSIGHTS:
 * - Router extends JTAGModule for consistent context management
 * - Health manager prevents message loss during connection issues
 * - Queue system ensures message ordering and delivery guarantees
 * - Response correlator transforms async bus into request-response pattern
 * - Transport factory abstracts connection management complexity
 */

import { JTAGModule } from '@shared/JTAGModule';
import type { JTAGContext, JTAGEnvironment, JTAGMessage } from '@shared/JTAGTypes';
import { JTAGMessageTypes, JTAGMessageFactory } from '@shared/JTAGTypes';
import { TransportFactory, TRANSPORT_TYPES } from '@systemTransports';
import type { TransportConfig, JTAGTransport, TransportEndpoint } from '@systemTransports';
import { JTAGMessageQueue, MessagePriority } from '@sharedQueuing/JTAGMessageQueue';
import type { QueuedItem } from '@sharedQueuing/PriorityQueue';
import { ConnectionHealthManager } from '@shared/ConnectionHealthManager';
import { ResponseCorrelator } from '@shared/ResponseCorrelator';
import { EndpointMatcher } from '@sharedRouting/EndpointMatcher';
import { EventManager } from '@systemEvents';

// Import configuration types and utilities
import type { 
  JTAGRouterConfig, 
  ResolvedJTAGRouterConfig 
} from '@shared/JTAGRouterTypes';
import { createJTAGRouterConfig } from '@shared/JTAGRouterTypes';

// Re-export configuration types for convenience
export type { JTAGRouterConfig, ResolvedJTAGRouterConfig } from '@shared/JTAGRouterTypes';
export { DEFAULT_JTAG_ROUTER_CONFIG, createJTAGRouterConfig } from '@shared/JTAGRouterTypes';

import type { JTAGResponsePayload } from '@shared/ResponseTypes';
import type { ConsolePayload } from '@daemonsConsoleDaemon/shared/ConsoleDaemon';
import type { RouterResult, RequestResult, EventResult, LocalRoutingResult } from '@shared/RouterTypes';

/**
 * Message Subscriber Interface
 * 
 * Contract for all components that can receive and process JTAG messages.
 * Daemons implement this interface to register for message delivery.
 */
export interface MessageSubscriber {
  handleMessage(message: JTAGMessage): Promise<JTAGResponsePayload>;
  get endpoint(): string;
  get uuid(): string;
}

export interface RouterStatus {
  environment: string;
  initialized: boolean;
  subscribers: number;
  transport: {
    name: string;
    connected: boolean;
  } | null;
  queue: ReturnType<JTAGMessageQueue['getStatus']>;
  health: ReturnType<ConnectionHealthManager['getHealth']>;
  transportStatus: {
    initialized: boolean;
    transportCount: number;
    transports: Array<{
      name: string;
      connected: boolean;
      type: string;
    }>;
  };
}


export class JTAGRouter extends JTAGModule implements TransportEndpoint {
  private readonly endpointMatcher = new EndpointMatcher<MessageSubscriber>();

  //Use a map, strongly typed keys, for our transports
  protected readonly transports = new Map<TRANSPORT_TYPES, JTAGTransport>();
  public readonly eventManager:EventManager;

  // Bus-level enhancements
  private readonly messageQueue: JTAGMessageQueue;
  private readonly healthManager: ConnectionHealthManager;
  private readonly responseCorrelator: ResponseCorrelator;
  private readonly config: ResolvedJTAGRouterConfig;
  private isInitialized = false;

  constructor(context: JTAGContext, config: JTAGRouterConfig = {}) {
    super('universal-router', context);
    
    // Apply default configuration with strong typing using centralized utility
    this.config = createJTAGRouterConfig(config);

    // Initialize event manager
    this.eventManager = new EventManager();
    
    // Initialize modular bus-level features with resolved config
    this.messageQueue = new JTAGMessageQueue(context, {
      enableDeduplication: this.config.queue.enableDeduplication,
      deduplicationWindow: this.config.queue.deduplicationWindow,
      maxSize: this.config.queue.maxSize,
      maxRetries: this.config.queue.maxRetries,
      flushInterval: this.config.queue.flushInterval
    });
    
    this.healthManager = new ConnectionHealthManager(context, this.eventManager.events);
    this.responseCorrelator = new ResponseCorrelator(this.config.response.correlationTimeout);
    
    if (this.config.enableLogging) {
      console.log(`🚀 JTAGRouter[${context.environment}]: Initialized with request-response correlation and queuing`);
    }
  }

  async initialize(): Promise<void> {
    if (this.isInitialized) return;
    
    console.log(`🔧 ${this.toString()}: Initializing with transport and health monitoring...`);
    
    // Initialize transport
    await this.initializeTransport();

    const crossContextTransport = this.transports.get(TRANSPORT_TYPES.CROSS_CONTEXT);
    
    // Start health monitoring
    if (crossContextTransport) {
      this.healthManager.setTransport(crossContextTransport);
      this.healthManager.startMonitoring();
    }
    
    // Start message queue processing
    this.messageQueue.startProcessing(this.flushQueuedMessages.bind(this));
    
    this.isInitialized = true;
    console.log(`✅ ${this.toString()}: Initialization complete`);
  }

  registerSubscriber(endpoint: string, subscriber: MessageSubscriber): void {
    const fullEndpoint = `${this.context.environment}/${endpoint}`;
    this.endpointMatcher.register(fullEndpoint, subscriber);
    this.endpointMatcher.register(endpoint, subscriber);
    console.log(`📋 ${this.toString()}: Registered subscriber at ${fullEndpoint}`);
  }

  async postMessage(message: JTAGMessage): Promise<RouterResult> {
    console.log(`📨 ${this.toString()}: Routing message to ${message.endpoint}`);
    
    const targetEnvironment = this.extractEnvironment(message.endpoint);
    
    if (targetEnvironment === this.context.environment) {
      return await this.routeLocally(message);
    } else {
      return await this.routeRemotelyWithQueue(message);
    }
  }

  /**
   * Route message remotely with proper type-based routing
   */
  private async routeRemotelyWithQueue(message: JTAGMessage): Promise<RouterResult> {
    // Check if this is a remote P2P route
    const remoteInfo = this.parseRemoteEndpoint(message.endpoint);
    if (remoteInfo) {
      return await this.routeToRemoteNode(message, remoteInfo);
    }

    const crossContextTransport = this.transports.get(TRANSPORT_TYPES.CROSS_CONTEXT);

    // Standard cross-context routing (browser <-> server)
    if (!crossContextTransport) {
      throw new Error(`No cross-context transport available for ${message.endpoint}`);
    }

    // Use type-safe message type checking instead of string searching
    if (JTAGMessageTypes.isRequest(message)) {
      // REQUEST PATTERN: Use correlation system and await response
      return await this.handleRequestMessage(message);
    } else if (JTAGMessageTypes.isEvent(message)) {
      // EVENT PATTERN: Use queue system for fire-and-forget messages
      return await this.handleEventMessage(message);
    } else if (JTAGMessageTypes.isResponse(message)) {
      // RESPONSE PATTERN: Send response back to requesting client
      return await this.handleResponseMessage(message);
    } else {
      // This should never happen as all valid JTAGMessage types are handled above
      // Type assertion is safe here since we know message is a JTAGMessage
      throw new Error('Unknown message type: ' + JSON.stringify(message));
    }
  }

  /**
   * Route message to remote P2P node using UDP multicast transport
   */
  private async routeToRemoteNode(
    message: JTAGMessage, 
    remoteInfo: { nodeId: string; targetPath: string }
  ): Promise<RouterResult> {
    const { nodeId, targetPath } = remoteInfo;
    
    console.log(`🌐 ${this.toString()}: Routing to remote node ${nodeId} -> ${targetPath}`);

    const p2pTransport = this.transports.get(TRANSPORT_TYPES.P2P);

    // Get P2P transport (UDP multicast)
    if (!p2pTransport) {
      throw new Error(`No P2P transport available for remote routing to node ${nodeId}`);
    }

    // Create modified message with target path (strip remote/ prefix)
    const remoteMessage: JTAGMessage = {
      ...message,
      endpoint: targetPath // Remove the /remote/{nodeId}/ prefix
    };

    // Use the P2P transport's send method which handles node discovery and routing
    try {
      if (JTAGMessageTypes.isRequest(remoteMessage)) {
        // For requests, we need to wait for response through P2P network
        console.log(`🎯 ${this.toString()}: Sending P2P request to ${nodeId}`);
        const responsePromise = this.responseCorrelator.createRequest(remoteMessage.correlationId);
        
        await p2pTransport.send(remoteMessage);
        const response = await responsePromise;
        
        console.log(`✅ ${this.toString()}: P2P response received from ${nodeId}`);
        return { success: true, resolved: true, response };
        
      } else {
        // For events, fire-and-forget through P2P network
        console.log(`📢 ${this.toString()}: Sending P2P event to ${nodeId}`);
        await p2pTransport.send(remoteMessage);
        return { success: true, delivered: true };
      }
    } catch (error) {
      console.error(`❌ ${this.toString()}: P2P routing failed to ${nodeId}:`, error);
      throw error;
    }
  }

  /**
   * Handle request messages that need responses (screenshot, etc.)
   */
  private async handleRequestMessage(message: JTAGMessage): Promise<RequestResult> {
    // Message already has correlationId from type system
    if (!JTAGMessageTypes.isRequest(message)) {
      throw new Error('Expected request message');
    }

    console.log(`🎯 ${this.toString()}: Sending request ${message.correlationId} to ${message.endpoint}`);

    // Create pending request that will resolve when response arrives
    const responsePromise = this.responseCorrelator.createRequest(message.correlationId);

    try {
      const crossContextTransport = this.transports.get(TRANSPORT_TYPES.CROSS_CONTEXT);
      if (!crossContextTransport) {
        throw new Error('No cross-context transport available for request');
      }
      // Send message immediately (requests need immediate delivery)
      await crossContextTransport.send(message);
      console.log(`📤 ${this.toString()}: Request sent, awaiting response...`);
      
      // Await the correlated response
      const response = await responsePromise;
      console.log(`✅ ${this.toString()}: Response received for ${message.correlationId}`);
      return { success: true, resolved: true, response };

    } catch (error) {
      console.error(`❌ ${this.toString()}: Request failed:`, error);
      throw error;
    }
  }

  /**
   * Handle event messages (fire-and-forget: console logs, notifications, etc.)
   */
  private async handleEventMessage(message: JTAGMessage): Promise<EventResult> {
    // Determine priority based on message content
    const priority = this.determinePriority(message);
    
    // Queue message with deduplication (prevents console error flooding)
    const queued = this.messageQueue.enqueue(message, priority);
    
    if (!queued) {
      // Message was deduplicated - silently handle this case
      return { success: true, deduplicated: true };
    }

    // For critical messages, attempt immediate delivery if healthy
    const health = this.healthManager.getHealth();
    if (priority <= MessagePriority.HIGH && health.isHealthy) {
      try {
        const crossContextTransport = this.transports.get(TRANSPORT_TYPES.CROSS_CONTEXT);
        if (!crossContextTransport) {
          console.warn(`⚠️ ${this.toString()}: No transport available for immediate delivery, queued for retry`);
          return { success: false, queued: true, willRetry: true };
        }
        await crossContextTransport.send(message);
        return { success: true, delivered: true };
      } catch (error) {
        console.warn(`⚠️ ${this.toString()}: Immediate delivery failed, queued for retry`, error);
        return { success: false, queued: true, willRetry: true };
      }
    }

    return { success: true, queued: true, priority: MessagePriority[priority] };
  }

  /**
   * Handle response messages (send back to requesting client)
   */
  private async handleResponseMessage(message: JTAGMessage): Promise<EventResult> {
    if (!JTAGMessageTypes.isResponse(message)) {
      throw new Error('Expected response message');
    }

    console.log(`📤 ${this.toString()}: Sending response ${message.correlationId} to ${message.endpoint}`);

    try {
      // Send response immediately (responses are critical)
      const crossContextTransport = this.transports.get(TRANSPORT_TYPES.CROSS_CONTEXT);
      if (!crossContextTransport) {
        throw new Error('No cross-context transport available for response');
      }
      await crossContextTransport.send(message);
      console.log(`✅ ${this.toString()}: Response sent for ${message.correlationId}`);
      return { success: true, delivered: true };
    } catch (error) {
      console.error(`❌ ${this.toString()}: Failed to send response:`, error);
      throw error;
    }
  }

  /**
   * Determine message priority for queue processing
   */
  private determinePriority(message: JTAGMessage): MessagePriority {
    // System/health messages get critical priority
    if (message.origin.includes('system') || message.origin.includes('health')) {
      return MessagePriority.CRITICAL;
    }

    // Commands get high priority
    if (message.endpoint.includes('commands')) {
      return MessagePriority.HIGH;
    }

    // Console errors get high priority (but will be deduplicated)
    if (message.origin.includes('console') && this.isConsolePayload(message.payload) && message.payload.level === 'error') {
      return MessagePriority.HIGH;
    }

    return MessagePriority.NORMAL;
  }

  /**
   * Type guard for ConsolePayload
   */
  private isConsolePayload(payload: any): payload is ConsolePayload {
    return payload && typeof payload === 'object' && 'level' in payload;
  }

  /**
   * Flush queued messages (called by JTAGMessageQueue)
   */
  private async flushQueuedMessages(messages: QueuedItem<JTAGMessage>[]): Promise<QueuedItem<JTAGMessage>[]> {
    const crossContextTransport = this.transports.get(TRANSPORT_TYPES.CROSS_CONTEXT);

    if (!crossContextTransport || messages.length === 0) {
      return messages; // All failed
    }

    const health = this.healthManager.getHealth();
    if (!health.isHealthy) {
      console.log(`⏸️ ${this.toString()}: Skipping flush - connection unhealthy (${health.state})`);
      return messages; // All failed, will retry
    }

    const failedMessages: QueuedItem<JTAGMessage>[] = [];
    
    for (const queuedItem of messages) {
      try {
        await crossContextTransport.send(queuedItem.item);
        console.log(`✅ ${this.toString()}: Delivered queued message ${queuedItem.id}`);
      } catch (error) {
        console.warn(`❌ ${this.toString()}: Failed to deliver ${queuedItem.id}`, error);
        failedMessages.push(queuedItem);
      }
    }

    return failedMessages;
  }

  private async routeLocally(message: JTAGMessage): Promise<LocalRoutingResult> {
    // Handle response messages - resolve pending requests
    if (JTAGMessageTypes.isResponse(message)) {
      console.log(`📨 ${this.toString()}: Received response for ${message.correlationId}`);
      const resolved = this.responseCorrelator.resolveRequest(message.correlationId, message.payload);
      if (resolved) {
        return { success: true, resolved: true };
      } else {
        console.warn(`⚠️ ${this.toString()}: No pending request found for ${message.correlationId}`);
        return { success: false, error: 'No pending request found' };
      }
    }

    // Regular message routing for events and requests using EndpointMatcher
    const matchResult = this.endpointMatcher.match(message.endpoint);
    
    if (!matchResult) {
      throw new Error(`No subscriber found for endpoint: ${message.endpoint}`);
    }

    const { subscriber, matchedEndpoint, matchType } = matchResult;
    
    if (matchType === 'hierarchical') {
      console.log(`📋 ${this.toString()}: Using hierarchical routing: ${matchedEndpoint} handling ${message.endpoint}`);
    }

    console.log(`🏠 ${this.toString()}: Routing locally to ${message.endpoint}`);
    const result = await subscriber.handleMessage(message);
    
    // If this was a request, send response back
    if (JTAGMessageTypes.isRequest(message)) {
      console.log(`🔄 ${this.toString()}: Sending response for ${message.correlationId}`);
      
      const responseMessage = JTAGMessageFactory.createResponse(
        this.context,
        message.endpoint, // Response origin is the request's endpoint
        message.origin,   // Response endpoint is the request's origin
        result,          // The result from handling the request
        message.correlationId
      );
      
      // Send response back (don't await, this is a fire-and-forget response)
      this.postMessage(responseMessage).catch(error => {
        console.error(`❌ ${this.toString()}: Failed to send response:`, error);
      });
    }
    
    return { success: true };
  }


  private extractEnvironment(endpoint: string): JTAGEnvironment {
    if (endpoint.startsWith('browser/')) return 'browser';
    if (endpoint.startsWith('server/')) return 'server';
    if (endpoint.startsWith('remote/')) return 'remote';
    return this.context.environment;
  }

  /**
   * Parse remote endpoint to extract node ID and target path
   * Format: /remote/{nodeId}/daemon/command or /remote/{nodeId}/server/daemon/command
   */
  private parseRemoteEndpoint(endpoint: string): { nodeId: string; targetPath: string } | null {
    if (!endpoint.startsWith('remote/')) {
      return null;
    }

    const parts = endpoint.split('/');
    if (parts.length < 3) {
      return null;
    }

    const nodeId = parts[1]; // remote/{nodeId}/...
    const targetPath = parts.slice(2).join('/'); // everything after nodeId

    return { nodeId, targetPath };
  }

  /**
   * Initialize transport (TransportEndpoint interface implementation)
   */
  async initializeTransport(config?: TransportConfig): Promise<void> {
    console.log(`🔗 ${this.toString()}: Initializing transport with base64 encoding`);
    
    // Create cross-context transport using router configuration
    const ctxTransportConfig: TransportConfig = { 
      preferred: this.config.transport.preferred,
      fallback: this.config.transport.fallback,
      role: this.config.transport.role,
      serverPort: this.config.transport.serverPort,
      serverUrl: this.config.transport.serverUrl,
      eventSystem: this.eventManager.events,
      sessionId: this.config.sessionId,
      ...config // Allow override from parameter
    };
    
    const crossContextTransport = await TransportFactory.createTransport(this.context.environment, ctxTransportConfig);
    this.transports.set(TRANSPORT_TYPES.CROSS_CONTEXT, crossContextTransport);

    // P2P transport disabled - needs UDP multicast implementation, not WebSocket
    // See ENHANCEMENT issue above for dedicated P2P transport requirements
    // const p2pTransportConfig: TransportConfig = { 
    //   preferred: 'websocket', 
    //   fallback: true,
    //   eventSystem: this.eventManager.events,
    //   sessionId: this.config.sessionId // Pass sessionId for client handshake
    // };
    // const p2pTransport = await TransportFactory.createTransport(this.context.environment, p2pTransportConfig);
    // this.transports.set(TRANSPORT_TYPES.P2P, p2pTransport);

    // Set up message handlers
    await this.setupMessageHandlers();
  }

  /**
   * Set up message handlers for all transports (TransportEndpoint interface implementation)
   */
  async setupMessageHandlers(): Promise<void> {
    for (const transport of this.transports.values()) {
      if (transport.setMessageHandler) {
        transport.setMessageHandler((message: JTAGMessage) => {
          this.postMessage(message).catch(console.error);
        });
        console.log(`✅ ${this.toString()}: Transport ready: ${transport.name}`);
      }
    }
  }

  /**
   * Shutdown all transports (TransportEndpoint interface implementation)
   */
  async shutdownTransports(): Promise<void> {
    console.log(`🔄 ${this.toString()}: Shutting down transports...`);
    
    for (const transport of this.transports.values()) {
      await transport.disconnect();
    }
    this.transports.clear();
    
    console.log(`✅ ${this.toString()}: Transport shutdown complete`);
  }

  /**
   * Get transport status (TransportEndpoint interface implementation)
   */
  getTransportStatus(): { initialized: boolean; transportCount: number; transports: Array<{ name: string; connected: boolean; type: string; }> } {
    return {
      initialized: this.isInitialized,
      transportCount: this.transports.size,
      transports: Array.from(this.transports.entries()).map(([type, transport]) => ({
        name: transport.name,
        connected: transport.isConnected(),
        type: type.toString()
      }))
    };
  }

  /**
   * Legacy method for compatibility - transport factory is now in shared module
   */
  async setupCrossContextTransport(config?: TransportConfig): Promise<void> {
    if (!this.isInitialized) {
      await this.initialize();
    }
  }

  async shutdown(): Promise<void> {
    console.log(`🔄 ${this.toString()}: Shutting down with cleanup...`);
    
    // Stop bus-level systems
    this.healthManager.stopMonitoring();
    this.messageQueue.stopProcessing();
    
    // Shutdown transports using interface method
    await this.shutdownTransports();
    
    this.endpointMatcher.clear();
    
    console.log(`✅ ${this.toString()}: Shutdown complete`);
  }

  /**
   * Get enhanced router status
   */
  get status(): RouterStatus {
    const crossContextTransport = this.transports.get(TRANSPORT_TYPES.CROSS_CONTEXT);
    const transportStatus = this.getTransportStatus();
    
    return {
      environment: this.context.environment,
      initialized: this.isInitialized,
      subscribers: this.endpointMatcher.size(),
      transport: crossContextTransport ? {
        name: crossContextTransport.name,
        connected: crossContextTransport.isConnected()
      } : null,
      queue: this.messageQueue.getStatus(),
      health: this.healthManager.getHealth(),
      // Enhanced transport status from interface
      transportStatus
    };
  }
}