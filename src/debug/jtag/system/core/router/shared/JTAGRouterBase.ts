/**
 * JTAGRouterBase - Abstract Base Class for Router Evolution
 * 
 * INHERITANCE HIERARCHY:
 * JTAGRouterBase (new abstract base)
 *    ├── JTAGRouter (current hardcoded implementation)
 *    └── JTAGRouterDynamic (new dynamic implementation)
 * 
 * BENEFITS:
 * - Clean separation of transport management vs routing logic
 * - Easier migration path - can switch transport layer independently
 * - Better testability - can mock transport layer
 * - Future-proof - can add more router types (mesh-only, etc.)
 * 
 * DESIGN PATTERN:
 * - Base class: Core routing, promise correlation, health management
 * - Subclasses: Only transport initialization and access methods
 * - Strategy pattern: Transport management is pluggable
 */

import { JTAGModule } from '../../shared/JTAGModule';
import type { JTAGContext, JTAGEnvironment, JTAGMessage } from '../../types/JTAGTypes';
import { JTAGMessageTypes, JTAGMessageFactory } from '../../types/JTAGTypes';
import type { UUID } from '../../types/CrossPlatformUUID';
import type { ITransportFactory, TransportConfig, JTAGTransport, TransportEndpoint } from '../../../transports';
import type { ITransportHandler } from '../../../transports';
import { JTAGMessageQueue, MessagePriority } from './queuing/JTAGMessageQueue';
import type { QueuedItem } from './queuing/PriorityQueue';
import { ConnectionHealthManager } from './ConnectionHealthManager';
import { ResponseCorrelator } from '../../shared/ResponseCorrelator';
import { EndpointMatcher } from './EndpointMatcher';
import { EventManager } from '../../../events';

import type { 
  JTAGRouterConfig, 
  ResolvedJTAGRouterConfig 
} from './JTAGRouterTypes';
import { createJTAGRouterConfig } from './JTAGRouterTypes';

import type { JTAGResponsePayload, BaseResponsePayload } from '../../types/ResponseTypes';
import type { ConsolePayload } from '../../../../daemons/console-daemon/shared/ConsoleDaemon';
import type { RouterResult, RequestResult, EventResult, LocalRoutingResult } from './RouterTypes';

/**
 * Message Subscriber Interface
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

// ============================================================================
// ABSTRACT TRANSPORT STRATEGY INTERFACE
// ============================================================================

/**
 * Abstract Transport Strategy - Implemented by subclasses
 * This is where hardcoded vs dynamic implementations differ
 */
export interface ITransportStrategy {
  /**
   * Initialize transport layer
   */
  initializeTransports(config?: Partial<TransportConfig>): Promise<void>;

  /**
   * Get transport for cross-context communication (browser ↔ server)
   */
  getCrossContextTransport(): JTAGTransport | undefined;

  /**
   * Get transport for P2P communication (node ↔ node)
   */
  getP2PTransport(): JTAGTransport | undefined;

  /**
   * Get transport by custom identifier
   */
  getTransportByIdentifier?(identifier: string): JTAGTransport | undefined;

  /**
   * Setup message handlers for all transports
   */
  setupTransportMessageHandlers(): Promise<void>;

  /**
   * Shutdown all transports
   */
  shutdownAllTransports(): Promise<void>;

  /**
   * Get transport status information
   */
  getTransportStatusInfo(): {
    initialized: boolean;
    transportCount: number;
    transports: Array<{
      name: string;
      connected: boolean;
      type: string;
    }>;
  };
}

// ============================================================================
// ABSTRACT BASE ROUTER - CORE ROUTING LOGIC
// ============================================================================

/**
 * JTAGRouterBase - Abstract base class with core routing logic
 * 
 * CONTAINS: All the sophisticated routing logic that works
 * ABSTRACT: Only transport management methods
 * PRESERVES: Promise correlation, health management, message queuing
 */
export abstract class JTAGRouterBase extends JTAGModule implements TransportEndpoint, ITransportHandler {
  private readonly endpointMatcher = new EndpointMatcher<MessageSubscriber>();

  // Core router components (same for all implementations)
  public readonly eventManager: EventManager;
  private readonly messageQueue: JTAGMessageQueue;
  private readonly healthManager: ConnectionHealthManager;
  private readonly responseCorrelator: ResponseCorrelator;
  protected readonly config: ResolvedJTAGRouterConfig;
  private isInitialized = false;

  // Request tracking for response routing
  private readonly requestSenders = new Map<string, {
    environment: JTAGEnvironment;
    transport?: JTAGTransport;
  }>();

  // Message processing deduplication
  private readonly processedMessages = new Set<string>();
  private readonly MESSAGE_PROCESSING_TIMEOUT = 30000;

  // Abstract transport strategy - implemented by subclasses
  protected abstract transportStrategy: ITransportStrategy;

  constructor(context: JTAGContext, config: JTAGRouterConfig = {}) {
    super('router-base', context);
    
    this.config = createJTAGRouterConfig(config);
    this.eventManager = new EventManager();
    
    // Initialize core router components (same for all implementations)
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
      console.log(`🚀 ${this.constructor.name}[${context.environment}]: Base router initialized`);
    }
  }

  // ============================================================================
  // ABSTRACT METHODS - IMPLEMENTED BY SUBCLASSES (TRANSPORT STRATEGY)
  // ============================================================================

  /**
   * Get environment-specific transport factory
   */
  protected abstract getTransportFactory(): Promise<ITransportFactory>;

  /**
   * Initialize the transport strategy (abstract - implemented by subclasses)
   */
  protected abstract initializeTransportStrategy(): Promise<void>;

  // ============================================================================
  // CORE ROUTER LOGIC - SAME FOR ALL IMPLEMENTATIONS
  // ============================================================================

  async initialize(): Promise<void> {
    if (this.isInitialized) return;
    
    console.log(`🔧 ${this.toString()}: Initializing router with core components...`);
    
    // Initialize transport strategy (implemented by subclass)
    await this.initializeTransportStrategy();

    // Get cross-context transport for health monitoring
    const crossContextTransport = this.transportStrategy.getCrossContextTransport();
    
    if (crossContextTransport) {
      this.healthManager.setTransport(crossContextTransport);
      this.healthManager.startMonitoring();
    }
    
    // Start message queue processing
    this.messageQueue.startProcessing(this.flushQueuedMessages.bind(this));
    
    this.isInitialized = true;
    console.log(`✅ ${this.toString()}: Router initialization complete`);
  }

  registerSubscriber(endpoint: string, subscriber: MessageSubscriber): void {
    const fullEndpoint = `${this.context.environment}/${endpoint}`;
    this.endpointMatcher.register(fullEndpoint, subscriber);
    
    if (!this.endpointMatcher.hasExact(endpoint)) {
      this.endpointMatcher.register(endpoint, subscriber);
      console.log(`📋 ${this.toString()}: Registered subscriber at ${fullEndpoint} AND ${endpoint}`);
    } else {
      console.log(`📋 ${this.toString()}: Registered subscriber at ${fullEndpoint} (${endpoint} already exists)`);
    }
  }

  /**
   * Core message routing - SAME FOR ALL IMPLEMENTATIONS
   * This is the heart of your promise correlation system
   */
  async postMessage(message: JTAGMessage): Promise<RouterResult> {
    // Message processing deduplication (preserves parent logic)
    let processingToken: string | undefined;
    if (JTAGMessageTypes.isRequest(message)) {
      processingToken = `req:${message.correlationId}`;
    } else if (JTAGMessageTypes.isResponse(message)) {
      processingToken = `res:${message.correlationId}`;
    }
    
    if (processingToken) {
      if (this.processedMessages.has(processingToken)) {
        console.log(`🔄 ${this.toString()}: Skipping duplicate message ${processingToken}`);
        return { success: true, deduplicated: true };
      }
      
      this.processedMessages.add(processingToken);
      setTimeout(() => {
        this.processedMessages.delete(processingToken);
      }, this.MESSAGE_PROCESSING_TIMEOUT);
      
      console.log(`📨 ${this.toString()}: Processing message ${processingToken} to ${message.endpoint}`);
    }
    
    try {
      const targetEnvironment = this.extractEnvironmentForMessage(message);
      
      if (targetEnvironment === this.context.environment) {
        return await this.routeLocally(message);
      } else {
        return await this.routeRemotelyWithQueue(message);
      }
    } catch (error) {
      if (processingToken) {
        this.processedMessages.delete(processingToken);
      }
      throw error;
    }
  }

  /**
   * Remote routing - SAME LOGIC, DIFFERENT TRANSPORT ACCESS
   */
  private async routeRemotelyWithQueue(message: JTAGMessage): Promise<RouterResult> {
    // Check for P2P routing
    const remoteInfo = this.parseRemoteEndpoint(message.endpoint);
    if (remoteInfo) {
      return await this.routeToRemoteNode(message, remoteInfo);
    }

    // Standard cross-context routing - use strategy pattern
    const crossContextTransport = this.transportStrategy.getCrossContextTransport();

    if (!crossContextTransport) {
      throw new Error(`No cross-context transport available for ${message.endpoint}`);
    }

    // Message type handling (preserves parent logic)
    if (JTAGMessageTypes.isRequest(message)) {
      return await this.handleRequestMessage(message, crossContextTransport);
    } else if (JTAGMessageTypes.isEvent(message)) {
      return await this.handleEventMessage(message, crossContextTransport);
    } else if (JTAGMessageTypes.isResponse(message)) {
      return await this.handleResponseMessage(message, crossContextTransport);
    } else {
      throw new Error('Unknown message type: ' + JSON.stringify(message));
    }
  }

  /**
   * P2P routing - uses strategy for transport access
   */
  private async routeToRemoteNode(
    message: JTAGMessage, 
    remoteInfo: { nodeId: string; targetPath: string }
  ): Promise<RouterResult> {
    const { nodeId, targetPath } = remoteInfo;
    
    console.log(`🌐 ${this.toString()}: Routing to remote node ${nodeId} → ${targetPath}`);

    const p2pTransport = this.transportStrategy.getP2PTransport();

    if (!p2pTransport) {
      throw new Error(`No P2P transport available for remote routing to node ${nodeId}`);
    }

    const remoteMessage: JTAGMessage = {
      ...message,
      endpoint: targetPath
    };

    try {
      if (JTAGMessageTypes.isRequest(remoteMessage)) {
        console.log(`🎯 ${this.toString()}: Sending P2P request to ${nodeId}`);
        const responsePromise = this.responseCorrelator.createRequest(remoteMessage.correlationId);
        
        await p2pTransport.send(remoteMessage);
        const response = await responsePromise;
        
        console.log(`✅ ${this.toString()}: P2P response received from ${nodeId}`);
        return { success: true, resolved: true, response };
        
      } else {
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
   * Handle request messages - PRESERVES PROMISE CORRELATION
   */
  private async handleRequestMessage(
    message: JTAGMessage, 
    transport: JTAGTransport
  ): Promise<RequestResult> {
    if (!JTAGMessageTypes.isRequest(message)) {
      throw new Error('Expected request message');
    }

    console.log(`🎯 ${this.toString()}: Sending request ${message.correlationId} to ${message.endpoint}`);

    const responsePromise = this.responseCorrelator.createRequest(message.correlationId);

    try {
      await transport.send(message);
      console.log(`📤 ${this.toString()}: Request sent, awaiting response...`);
      
      const response = await responsePromise;
      console.log(`✅ ${this.toString()}: Response received for ${message.correlationId}`);
      return { success: true, resolved: true, response };

    } catch (error) {
      console.error(`❌ ${this.toString()}: Request failed:`, error);
      throw error;
    }
  }

  /**
   * Handle event messages - PRESERVES QUEUE LOGIC
   */
  private async handleEventMessage(
    message: JTAGMessage,
    transport: JTAGTransport
  ): Promise<EventResult> {
    const priority = this.determinePriority(message);
    const queued = this.messageQueue.enqueue(message, priority);
    
    if (!queued) {
      return { success: true, deduplicated: true };
    }

    const health = this.healthManager.getHealth();
    if (priority <= MessagePriority.HIGH && health.isHealthy) {
      try {
        await transport.send(message);
        return { success: true, delivered: true };
      } catch (error) {
        console.warn(`⚠️ ${this.toString()}: Immediate delivery failed, queued for retry`, error);
        return { success: false, queued: true, willRetry: true };
      }
    }

    return { success: true, queued: true, priority: MessagePriority[priority] };
  }

  /**
   * Handle response messages
   */
  private async handleResponseMessage(
    message: JTAGMessage,
    transport: JTAGTransport
  ): Promise<EventResult> {
    if (!JTAGMessageTypes.isResponse(message)) {
      throw new Error('Expected response message');
    }

    console.log(`📤 ${this.toString()}: Sending response ${message.correlationId} to ${message.endpoint}`);

    try {
      await transport.send(message);
      console.log(`✅ ${this.toString()}: Response sent for ${message.correlationId}`);
      return { success: true, delivered: true };
    } catch (error) {
      console.error(`❌ ${this.toString()}: Failed to send response:`, error);
      throw error;
    }
  }

  /**
   * Local routing - PRESERVES ALL PROMISE CORRELATION LOGIC
   */
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

    // Regular message routing
    const matchResult = this.endpointMatcher.match(message.endpoint);
    
    if (!matchResult) {
      console.log(`❌ ${this.toString()}: No subscriber found for endpoint: ${message.endpoint}`);
      console.log(`📋 ${this.toString()}: Available endpoints: ${this.endpointMatcher.getEndpoints().join(', ')}`);
      throw new Error(`No subscriber found for endpoint: ${message.endpoint}`);
    }

    const { subscriber, matchedEndpoint, matchType } = matchResult;
    
    console.log(`🎯 ${this.toString()}: Match found - endpoint: ${message.endpoint}, matched: ${matchedEndpoint}, type: ${matchType}`);
    
    console.log(`🏠 ${this.toString()}: Routing locally to ${message.endpoint} via ${matchedEndpoint}`);
    const result = await subscriber.handleMessage(message);
    
    // For requests, send response back (PRESERVES PROMISE CHAIN)
    if (JTAGMessageTypes.isRequest(message)) {
      console.log(`🔄 ${this.toString()}: Sending response for ${message.correlationId}`);
      
      const senderEnvironment = this.determineSenderEnvironment(message);
      this.requestSenders.set(message.correlationId, { 
        environment: senderEnvironment 
      });
      
      const responseMessage = JTAGMessageFactory.createResponse(
        this.context,
        message.endpoint,
        message.origin,
        result,
        message
      );
      
      console.log(`📤 ${this.toString()}: Created response message - correlationId: ${responseMessage.correlationId}`);
      
      this.postMessage(responseMessage).catch(error => {
        console.error(`❌ ${this.toString()}: Failed to send response:`, error);
      });
    }
    
    return { success: true };
  }

  // ============================================================================
  // HELPER METHODS - SAME FOR ALL IMPLEMENTATIONS
  // ============================================================================

  private determinePriority(message: JTAGMessage): MessagePriority {
    if (message.origin.includes('system') || message.origin.includes('health')) {
      return MessagePriority.CRITICAL;
    }
    if (message.endpoint.includes('commands')) {
      return MessagePriority.HIGH;
    }
    if (message.origin.includes('console') && this.isConsolePayload(message.payload) && message.payload.level === 'error') {
      return MessagePriority.HIGH;
    }
    return MessagePriority.NORMAL;
  }

  private isConsolePayload(payload: any): payload is ConsolePayload {
    return payload && typeof payload === 'object' && 'level' in payload;
  }

  private async flushQueuedMessages(messages: QueuedItem<JTAGMessage>[]): Promise<QueuedItem<JTAGMessage>[]> {
    const crossContextTransport = this.transportStrategy.getCrossContextTransport();

    if (!crossContextTransport || messages.length === 0) {
      return messages;
    }

    const health = this.healthManager.getHealth();
    if (!health.isHealthy) {
      console.log(`⏸️ ${this.toString()}: Skipping flush - connection unhealthy (${health.state})`);
      return messages;
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

  private extractEnvironment(endpoint: string): JTAGEnvironment {
    if (endpoint.startsWith('browser/')) return 'browser';
    if (endpoint.startsWith('server/')) return 'server';
    if (endpoint.startsWith('remote/')) return 'remote';
    return this.context.environment;
  }

  private extractEnvironmentForMessage(message: JTAGMessage): JTAGEnvironment {
    const endpoint = message.endpoint;
    
    if (endpoint === 'client' && JTAGMessageTypes.isResponse(message)) {
      console.log(`🔍 ${this.toString()}: Looking up sender for response ${message.correlationId} to endpoint '${endpoint}'`);
      const senderInfo = this.requestSenders.get(message.correlationId);
      if (senderInfo) {
        console.log(`🎯 ${this.toString()}: Found sender info - routing response ${message.correlationId} back to: ${senderInfo.environment}`);
        this.requestSenders.delete(message.correlationId);
        return senderInfo.environment;
      }
      
      console.warn(`⚠️ ${this.toString()}: No sender tracked for response ${message.correlationId}`);
      return this.context.environment;
    }
    
    return this.extractEnvironment(endpoint);
  }

  private determineSenderEnvironment(message: JTAGMessage): JTAGEnvironment {
    if (message.origin === 'client') {
      console.log(`🎯 ${this.toString()}: Transport client detected - keeping response in ${this.context.environment} environment`);
      return this.context.environment;
    }
    return this.extractEnvironment(message.origin);
  }

  private parseRemoteEndpoint(endpoint: string): { nodeId: string; targetPath: string } | null {
    if (!endpoint.startsWith('remote/')) {
      return null;
    }

    const parts = endpoint.split('/');
    if (parts.length < 3) {
      return null;
    }

    const nodeId = parts[1];
    const targetPath = parts.slice(2).join('/');

    return { nodeId, targetPath };
  }

  // ============================================================================
  // TRANSPORT ENDPOINT INTERFACE - DELEGATES TO STRATEGY
  // ============================================================================

  async initializeTransport(config?: Partial<TransportConfig>): Promise<void> {
    return this.transportStrategy.initializeTransports(config);
  }

  async setupMessageHandlers(): Promise<void> {
    return this.transportStrategy.setupTransportMessageHandlers();
  }

  async shutdownTransports(): Promise<void> {
    return this.transportStrategy.shutdownAllTransports();
  }

  getTransportStatus() {
    return this.transportStrategy.getTransportStatusInfo();
  }

  // ============================================================================
  // PUBLIC INTERFACE - SAME FOR ALL IMPLEMENTATIONS
  // ============================================================================

  async shutdown(): Promise<void> {
    console.log(`🔄 ${this.toString()}: Shutting down with cleanup...`);
    
    this.healthManager.stopMonitoring();
    this.messageQueue.stopProcessing();
    await this.shutdownTransports();
    this.endpointMatcher.clear();
    
    console.log(`✅ ${this.toString()}: Shutdown complete`);
  }

  get status(): RouterStatus {
    const crossContextTransport = this.transportStrategy.getCrossContextTransport();
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
      transportStatus
    };
  }

  // ITransportHandler implementation
  async handleTransportMessage(message: JTAGMessage): Promise<JTAGResponsePayload> {
    console.log(`📥 ${this.constructor.name}: Transport message received:`, message);
    
    const result = await this.postMessage(message);
    
    if ('response' in result && result.response) {
      return result.response as JTAGResponsePayload;
    }
    
    const response: BaseResponsePayload = {
      success: true,
      timestamp: new Date().toISOString(),
      context: this.context,
      sessionId: this.context.uuid
    };
    return response as JTAGResponsePayload;
  }
  
  get transportId(): UUID {
    return this.context.uuid;
  }
}