
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

import { JTAGModule } from '../../shared/JTAGModule';
import type { JTAGContext, JTAGEnvironment, JTAGMessage } from '../../types/JTAGTypes';
import { JTAGMessageTypes, JTAGMessageFactory } from '../../types/JTAGTypes';
import type { UUID } from '../../types/CrossPlatformUUID';
import { TRANSPORT_TYPES } from '../../../transports';
import type { ITransportFactory, TransportConfig, JTAGTransport, TransportEndpoint } from '../../../transports';
import type { ITransportHandler } from '../../../transports';
import { MessagePriority } from './queuing/JTAGMessageQueue';
import type { QueuedItem } from './queuing/PriorityQueue';

// Import transport strategy for extraction pattern
import { RouterUtilities } from './RouterUtilities';
import { CorrelationManager } from './CorrelationManager';
import { 
  ROUTER_CONSTANTS,
  UNIVERSAL_ROUTER,
  CLIENT_ENDPOINT,
  STRATEGY_HARDCODED,
  ENV_JTAG_FORCE_LEGACY,
  REQUEST_TOKEN_PREFIX,
  RESPONSE_TOKEN_PREFIX,
  CLIENT_CORRELATION_PREFIX,
  HIERARCHICAL_MATCH_TYPE,
  TIMEOUTS,
  type TransportStrategy
} from './RouterConstants';

// Import configuration types and utilities
import type { JTAGRouterConfig} from './JTAGRouterTypes';

// Re-export configuration types for convenience
export type { JTAGRouterConfig, ResolvedJTAGRouterConfig, RouterStatus } from './JTAGRouterTypes';
export { DEFAULT_JTAG_ROUTER_CONFIG, createJTAGRouterConfig } from './JTAGRouterTypes';

import type { JTAGResponsePayload, BaseResponsePayload } from '../../types/ResponseTypes';
import type { RouterResult, RequestResult, EventResult, LocalRoutingResult } from './RouterTypes';

// Import base class components that were in JTAGRouterBase
import { EndpointMatcher } from './EndpointMatcher';
import type { ITransportStrategy } from './ITransportStrategy';
import type { IRouterEnhancementStrategy } from './enhancements/RouterEnhancementStrategy';
import { DynamicTransportStrategy } from './DynamicTransportStrategy';
import { MinimalEnhancementStrategy } from './enhancements/RouterEnhancementStrategy';
import { JTAGMessageQueue } from './queuing/JTAGMessageQueue';
import { ConnectionHealthManager } from './ConnectionHealthManager';
import { ResponseCorrelator } from '../../shared/ResponseCorrelator';
import { EventManager } from '../../../events';
import { createJTAGRouterConfig, type ResolvedJTAGRouterConfig, type RouterStatus } from './JTAGRouterTypes';
import type { TransportEndpointStatus } from '../../../transports';
import { ExternalClientDetector } from './ExternalClientDetector';

/**
 * Message Subscriber Interface - Core contract for message handling
 */
export interface MessageSubscriber {
  handleMessage(message: JTAGMessage): Promise<JTAGResponsePayload>;
  get endpoint(): string;
  get uuid(): string;
}

// RouterStatus interface moved to JTAGRouterTypes.ts for better organization


export abstract class JTAGRouter extends JTAGModule implements TransportEndpoint, ITransportHandler {
  // Core subscriber management (moved from JTAGRouterBase)  
  protected readonly endpointMatcher = new EndpointMatcher<MessageSubscriber>();
  
  // Transport management pattern (moved from JTAGRouterBase)
  protected readonly transports = new Map<TRANSPORT_TYPES, JTAGTransport>();
  protected isInitialized = false;
  
  // Transport strategy pattern - extensible and P2P ready
  protected transportStrategy!: ITransportStrategy;
  
  // Enhancement strategy pattern - pluggable cross-cutting concerns
  protected enhancementStrategy!: IRouterEnhancementStrategy;
  
  // Bus-level components - shared initialization
  public readonly eventManager: EventManager;
  protected readonly messageQueue: JTAGMessageQueue;
  protected readonly healthManager: ConnectionHealthManager;
  protected readonly responseCorrelator: ResponseCorrelator;
  protected readonly config: ResolvedJTAGRouterConfig;

  // Track who sent each request for response routing (correlationId -> sender info)
  private readonly requestSenders = new Map<string, {
    environment: JTAGEnvironment;
    transport?: JTAGTransport;
  }>();

  // Clean external client detection (replaces sloppy prefix approach)
  protected readonly externalClientDetector = new ExternalClientDetector();

  // Correlation management - extracted for better modularity
  private readonly correlationManager = new CorrelationManager();

  // Message Processing Token System - prevent duplicate processing
  private readonly processedMessages = new Set<string>();
  private readonly MESSAGE_PROCESSING_TIMEOUT = TIMEOUTS.MESSAGE_PROCESSING;

  constructor(context: JTAGContext, config: JTAGRouterConfig) {
    super(UNIVERSAL_ROUTER, context);
    
    // Apply default configuration with strong typing using centralized utility
    this.config = createJTAGRouterConfig(config);
    
    // Initialize bus-level components with resolved config
    this.eventManager = new EventManager();
    
    this.messageQueue = new JTAGMessageQueue(context, {
      enableDeduplication: this.config.queue.enableDeduplication,
      deduplicationWindow: this.config.queue.deduplicationWindow,
      maxSize: this.config.queue.maxSize,
      maxRetries: this.config.queue.maxRetries,
      flushInterval: this.config.queue.flushInterval
    });
    
    this.healthManager = new ConnectionHealthManager(context, this.eventManager.events);
    this.responseCorrelator = new ResponseCorrelator(this.config.response.correlationTimeout);
    
    // Initialize transport and enhancement strategies using shared config logic
    this.initializeStrategies(config);
    
    if (this.config.enableLogging) {
      console.log(`🚀 ${this.toString()}: Initialized with request-response correlation and queuing`);
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

  /**
   * Shared strategy initialization logic - eliminates duplication
   */
  protected initializeStrategies(config: JTAGRouterConfig): void {
    // EVOLUTION: Dynamic is now DEFAULT - explicit opt-out to legacy
    const forceLegacy = config.transport?.forceLegacy === true ||
                       config.transport?.strategy === STRATEGY_HARDCODED ||
                       (typeof process !== 'undefined' && process.env?.[ENV_JTAG_FORCE_LEGACY] === 'true');
    
    const useDynamicTransport = !forceLegacy; // Dynamic by default
    
    if (useDynamicTransport) {
      console.log(`🚀 ${this.toString()}: Using dynamic transport strategy (P2P ready)`);
      this.transportStrategy = new DynamicTransportStrategy(this.transports, config.transport?.enableP2P ?? true);
      // Use minimal enhancements with dynamic strategy (following JTAGRouterDynamic pattern)
      this.enhancementStrategy = new MinimalEnhancementStrategy();
    } else {
      console.log(`📡 ${this.toString()}: Legacy transport strategy removed - using dynamic strategy instead`);
      this.transportStrategy = new DynamicTransportStrategy(this.transports);
      this.enhancementStrategy = new MinimalEnhancementStrategy();
    }
  }

  /**
   * Register subscriber for message routing (moved from JTAGRouterBase)
   */
  registerSubscriber(endpoint: string, subscriber: MessageSubscriber): void {
    const fullEndpoint = `${this.context.environment}/${endpoint}`;
    this.endpointMatcher.register(fullEndpoint, subscriber);
    
    // Only register short endpoint if it doesn't already exist to avoid duplicates
    if (!this.endpointMatcher.hasExact(endpoint)) {
      this.endpointMatcher.register(endpoint, subscriber);
      console.log(`📋 ${this.toString()}: Registered subscriber at ${fullEndpoint} AND ${endpoint}`);
    } else {
      console.log(`📋 ${this.toString()}: Registered subscriber at ${fullEndpoint} (${endpoint} already exists)`);
    }
  }

  /**
   * Get subscriber for endpoint - type-safe access
   */
  getSubscriber(endpoint: string): MessageSubscriber | null {
    const matchResult = this.endpointMatcher.match(endpoint);
    return matchResult?.subscriber ?? null;
  }

  async postMessage<T extends RouterResult>(message: JTAGMessage): Promise<T> {
    // Create unique processing tokens for request/response messages to prevent cross-message deduplication
    // Request and response with same correlationId should not block each other
    let processingToken: string | undefined;
    if (JTAGMessageTypes.isRequest(message)) {
      processingToken = `${REQUEST_TOKEN_PREFIX}${message.correlationId}`;
      // Register the correlation mapping for future response resolution
      this.correlationManager.registerRequest(message.correlationId);
    } else if (JTAGMessageTypes.isResponse(message)) {
      processingToken = `${RESPONSE_TOKEN_PREFIX}${message.correlationId}`;
    } else {
      // Event messages don't have correlationId and don't need deduplication
      processingToken = undefined;
    }
    
    if (processingToken) {
      // Check if we've already processed this exact message
      if (this.processedMessages.has(processingToken)) {
        console.warn(`🔄 ${this.toString()}: Skipping duplicate message ${processingToken}`);
        return { success: true, deduplicated: true } as T;
      }
      
      // Mark message as being processed
      this.processedMessages.add(processingToken);
      
      // Schedule cleanup of processing token after timeout
      setTimeout(() => {
        this.processedMessages.delete(processingToken);
      }, this.MESSAGE_PROCESSING_TIMEOUT);
      
      console.log(`📨 ${this.toString()}: Processing message ${processingToken} to ${message.endpoint}`);
    } else {
      // Event messages don't need logging - would flood logs with console events
      // console.log(`📨 ${this.toString()}: Processing event message to ${message.endpoint} (no deduplication)`);
    }
    
    try {
      const targetEnvironment = this.extractEnvironmentForMessage(message);
      
      if (targetEnvironment === this.context.environment) {
        // console.log(`🏠 ${this.toString()}: Routing locally to ${message.endpoint}`); // DISABLED: Causes infinite loop with ConsoleDaemon
        return await this.routeLocally(message) as T;
      } else {
        return await this.routeRemotelyWithQueue(message) as T;
      }
    } catch (error) {
      // Remove token on error so message can be retried
      if (processingToken) {
        this.processedMessages.delete(processingToken);
      }
      throw error;
    }
  }

  /**
   * Route message remotely with proper type-based routing
   */
  /**
   * Get environment-specific transport factory - implemented by JTAGRouterServer/JTAGRouterBrowser
   */
  protected abstract getTransportFactory(): Promise<ITransportFactory>;

  private async routeRemotelyWithQueue(message: JTAGMessage): Promise<RouterResult> {
    // Check if this is a remote P2P route
    const remoteInfo = RouterUtilities.parseRemoteEndpoint(message.endpoint);
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
        const response = await responsePromise as JTAGResponsePayload;
        
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
      const response = await responsePromise as JTAGResponsePayload;
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
    const priority = RouterUtilities.determinePriority(message);
    
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

  // determinePriority and isConsolePayload available in RouterUtilities

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
    const correlationId = (JTAGMessageTypes.isRequest(message) || JTAGMessageTypes.isResponse(message)) ? message.correlationId : 'none';
    // console.log(`🔍 ${this.toString()}: Routing locally - message type: ${message.messageType}, isRequest: ${JTAGMessageTypes.isRequest(message)}, isResponse: ${JTAGMessageTypes.isResponse(message)}, correlationId: ${correlationId}`); // DISABLED: Causes infinite loop with ConsoleDaemon
    
    // Handle different message types with focused methods
    if (JTAGMessageTypes.isResponse(message)) {
      console.log(`📥 ${this.toString()}: Taking response path for ${correlationId}`);
      return await this.handleIncomingResponse(message);
    }
    
    if (JTAGMessageTypes.isRequest(message)) {
      console.log(`📨 ${this.toString()}: Taking request path for ${correlationId}`);
      return await this.handleIncomingRequest(message);
    }
    
    // Events - just route to subscriber
    // CRITICAL FIX: Disable console logging that causes infinite loops with ConsoleDaemon
    // console.log(`📢 ${this.toString()}: Taking event path for ${correlationId}`);
    return await this.routeToSubscriber(message);
  }

  /**
   * Handle incoming response messages - resolve correlations
   */
  private async handleIncomingResponse(message: JTAGMessage): Promise<LocalRoutingResult> {
    // Type guard: response messages have correlationId
    if (!JTAGMessageTypes.isResponse(message)) {
      return { success: false, error: 'Expected response message' };
    }

    console.log(`📨 ${this.toString()}: Received response for ${message.correlationId}`);
    
    const resolved = this.responseCorrelator.resolveRequest(message.correlationId, message.payload);
    
    // Check if this response should be routed to an external client
    const isExternalCorrelation = this.externalClientDetector.isExternal(message.correlationId);
    console.log(`🔍 ${this.toString()}: External check for ${message.correlationId}: resolved=${resolved}, isExternal=${isExternalCorrelation}`);
    
    if (resolved && isExternalCorrelation) {
      console.log(`📡 ${this.toString()}: Routing external response for ${message.correlationId}`);
      await this.routeExternalResponse(message);
    }
    
    if (resolved) {
      return { success: true, resolved: true };
    } else {
      console.warn(`⚠️ ${this.toString()}: No pending request found for ${message.correlationId}`);
      return { success: false, error: 'No pending request found' };
    }
  }

  /**
   * Handle incoming request messages - register correlations and route
   */
  private async handleIncomingRequest(message: JTAGMessage): Promise<LocalRoutingResult> {
    // Type guard: request messages have correlationId
    if (!JTAGMessageTypes.isRequest(message)) {
      return { success: false, error: 'Expected request message' };
    }

    // Clean external client detection and registration
    if (this.externalClientDetector.isExternalClient(message)) {
      const correlationId = this.externalClientDetector.getCorrelationId(message);
      if (correlationId) {
        this.externalClientDetector.registerExternal(correlationId);
        console.log(`🔗 ${this.toString()}: Registered external client correlation ${correlationId}`);
        
        // Also register with response correlator for promise resolution
        this.responseCorrelator.createRequest(correlationId).catch(error => {
          console.warn(`⚠️ External correlation ${correlationId} failed: ${error.message}`);
        });
      }
    }

    // Route to subscriber and handle response creation
    const result = await this.routeToSubscriber(message);
    
    console.log(`🔍 ${this.toString()}: Request result for ${message.correlationId}: success=${result.success}, hasHandlerResult=${!!result.handlerResult}`);
    
    // Create and send response for request messages
    if (result.success && result.handlerResult) {
      console.log(`✅ ${this.toString()}: Creating response for ${message.correlationId}`);
      await this.createAndSendResponse(message, result.handlerResult);
    } else {
      console.log(`⚠️ ${this.toString()}: No response created for ${message.correlationId} - success=${result.success}, handlerResult=${!!result.handlerResult}`);
    }
    
    return result;
  }

  /**
   * Route external responses back via WebSocket
   */
  private async routeExternalResponse(message: JTAGMessage): Promise<void> {
    // Type guard: only response messages should reach here
    if (!JTAGMessageTypes.isResponse(message)) {
      return;
    }

    console.log(`📡 ${this.toString()}: Routing external response ${message.correlationId} back via WebSocket`);
    
    const webSocketTransport = this.transports.get(TRANSPORT_TYPES.CROSS_CONTEXT);
    if (!webSocketTransport) {
      console.warn(`⚠️ ${this.toString()}: No WebSocket transport available for external response ${message.correlationId}`);
      return;
    }

    try {
      await webSocketTransport.send(message);
      console.log(`✅ ${this.toString()}: External response sent via WebSocket for ${message.correlationId}`);
    } catch (error) {
      console.error(`❌ ${this.toString()}: Failed to send external response ${message.correlationId}:`, error);
    }
  }


  /**
   * Route message to appropriate subscriber
   */
  private async routeToSubscriber(message: JTAGMessage): Promise<LocalRoutingResult & { handlerResult: JTAGResponsePayload }> {
    const matchResult = this.endpointMatcher.match(message.endpoint);
    
    if (!matchResult) {
      console.log(`❌ ${this.toString()}: No subscriber found for endpoint: ${message.endpoint}`);
      console.log(`📋 ${this.toString()}: Available endpoints: ${this.endpointMatcher.getEndpoints().join(', ')}`);
      throw new Error(`No subscriber found for endpoint: ${message.endpoint}`);
    }

    const { subscriber, matchedEndpoint, matchType } = matchResult;
    
    // CRITICAL FIX: Disable console logging that causes infinite loops with ConsoleDaemon
    // console.log(`🎯 ${this.toString()}: Match found - endpoint: ${message.endpoint}, matched: ${matchedEndpoint}, type: ${matchType}, subscriber: ${subscriber.uuid}`);
    
    if (matchType === HIERARCHICAL_MATCH_TYPE) {
      // CRITICAL FIX: Disable console logging that causes infinite loops
      // console.log(`📋 ${this.toString()}: Using hierarchical routing: ${matchedEndpoint} handling ${message.endpoint}`);
    }

    // CRITICAL FIX: Disable console logging that causes infinite loops with ConsoleDaemon
    // console.log(`🏠 ${this.toString()}: Routing locally to ${message.endpoint} via ${matchedEndpoint}`);
    const result = await subscriber.handleMessage(message);
    
    return { success: true, handlerResult: result };
  }

  /**
   * Create and send response for request messages
   */
  private async createAndSendResponse(originalMessage: JTAGMessage, handlerResult: JTAGResponsePayload): Promise<void> {
    // Type guard: only request messages should reach here
    if (!JTAGMessageTypes.isRequest(originalMessage)) {
      return;
    }

    console.log(`🔄 ${this.toString()}: Sending response for ${originalMessage.correlationId}`);
    
    // Track who sent this request for proper response routing
    const senderEnvironment = RouterUtilities.determineSenderEnvironment(originalMessage, this.context.environment);
    this.requestSenders.set(originalMessage.correlationId, { 
      environment: senderEnvironment 
    });
    
    // Create response message using the original request
    const responseMessage = JTAGMessageFactory.createResponse(
      this.context,
      originalMessage.endpoint, // Response origin is the request's endpoint
      originalMessage.origin,   // Response endpoint is the request's origin
      handlerResult,           // The CommandResponse from the subscriber
      originalMessage          // Pass the original request message
    );
    
    console.log(`📤 ${this.toString()}: Created response message - origin: "${responseMessage.origin}", endpoint: "${responseMessage.endpoint}", correlationId: ${responseMessage.correlationId}`);
    
    // Send response back (don't await, this is a fire-and-forget response)
    this.postMessage(responseMessage).catch(error => {
      console.error(`❌ ${this.toString()}: Failed to send response:`, error);
    });
  }

  /**
   * Smart environment extraction that handles response routing
   * (Object-specific method - needs this.requestSenders state)
   */
  private extractEnvironmentForMessage(message: JTAGMessage): JTAGEnvironment {
    const endpoint = message.endpoint;
    
    // For responses to 'client', look up who sent the original request
    if (endpoint === CLIENT_ENDPOINT && JTAGMessageTypes.isResponse(message)) {
      console.log(`🔍 ${this.toString()}: Looking up sender for response ${message.correlationId} to endpoint '${endpoint}'`);
      const senderInfo = this.requestSenders.get(message.correlationId);
      if (senderInfo) {
        console.log(`🎯 ${this.toString()}: Found sender info - routing response ${message.correlationId} back to: ${senderInfo.environment}`);
        // Clean up tracking after use
        this.requestSenders.delete(message.correlationId);
        return senderInfo.environment;
      }
      
      console.warn(`⚠️ ${this.toString()}: No sender tracked for response ${message.correlationId}, available keys: [${Array.from(this.requestSenders.keys()).join(', ')}]`);
      return this.context.environment; // Stay in current environment for transport handling
    }
    
    // For all other cases, use static utility
    return RouterUtilities.extractEnvironment(endpoint, this.context.environment);
  }

  // determineSenderEnvironment available in RouterUtilities

  // parseRemoteEndpoint available in RouterUtilities

  /**
   * Initialize transport (TransportEndpoint interface implementation)
   */
  async initializeTransport(config?: Partial<TransportConfig>): Promise<void> {
    console.log(`🔗 ${this.toString()}: Initializing transport with strategy pattern`);
    
    // Create cross-context transport using router configuration
    const ctxTransportConfig: TransportConfig = { 
      protocol: this.config.transport.preferred,
      role: this.config.transport.role, // Use configured role (server for server, client for browser)
      eventSystem: this.eventManager.events,
      sessionId: this.config.sessionId!,
      serverPort: this.config.transport.serverPort,
      serverUrl: this.config.transport.serverUrl,
      fallback: this.config.transport.fallback,
      handler: this // TypeScript enforces ITransportHandler compliance
    };
    
    // Use environment-specific transport factory
    const factory = await this.getTransportFactory();
    
    // Delegate to transport strategy (preserves exact same behavior)
    await this.transportStrategy.initializeTransports(factory, this.context, ctxTransportConfig);

    // Set up message handlers through strategy
    await this.transportStrategy.setupMessageHandlers((message: JTAGMessage) => {
      this.postMessage(message).catch(console.error);
    });
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
   * Get transport status - conforms to TransportEndpoint interface
   */
  getTransportStatus(): TransportEndpointStatus {
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
   * Get enhanced router status - strongly typed router information
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
      transportStatus
    };
  }

  /**
   * Legacy method for compatibility - transport factory is now in shared module
   */
  async setupCrossContextTransport(config?: Partial<TransportConfig>): Promise<void> {
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


  // ITransportHandler implementation - ENFORCED by TypeScript
  
  /**
   * Handle transport protocol messages - same pattern as MessageSubscriber
   */
  async handleTransportMessage(message: JTAGMessage): Promise<JTAGResponsePayload> {
    console.log(`📥 JTAGRouter: Transport message received:`, message);
    
    // Route transport message through existing router infrastructure
    const result = await this.postMessage(message);
    
    // Extract response from RouterResult union type
    if ('response' in result && result.response) {
      return result.response as JTAGResponsePayload;
    }
    
    // Fallback: Create proper BaseResponsePayload for successful routing
    const response: BaseResponsePayload = {
      success: true,
      timestamp: new Date().toISOString(),
      context: this.context,
      sessionId: this.context.uuid
    };
    return response as JTAGResponsePayload;
  }
  
  /**
   * Transport identifier for routing
   */
  get transportId(): UUID {
    return this.context.uuid;
  }
}