// ISSUES: 0 open, last updated 2025-07-23 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking

/**
 * JTAG Universal Router - Context-Aware Message Routing with Bus-Level Queuing
 * 
 * The heart of the JTAG system - intelligent message routing with health monitoring,
 * queuing, and cross-context transport management. Handles both local and remote
 * message delivery with automatic fallback and retry mechanisms.
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

import { JTAGModule } from './JTAGModule';
import type { JTAGContext, JTAGMessage } from './JTAGTypes';
import { JTAGMessageTypes, JTAGMessageFactory } from './JTAGTypes';
import { TransportFactory } from '../transports/TransportFactory';
import type { TransportConfig } from '../transports/TransportFactory';
import { JTAGEventSystem } from './JTAGEventSystem';
import { JTAGMessageQueue, MessagePriority } from './queuing/JTAGMessageQueue';
import type { QueuedItem } from './queuing/PriorityQueue';
import { ConnectionHealthManager } from './ConnectionHealthManager';
import { ResponseCorrelator } from './ResponseCorrelator';
import { EndpointMatcher } from './routing/EndpointMatcher';

import type { JTAGResponsePayload } from './ResponseTypes';
import type { ConsolePayload } from '../daemons/console-daemon/shared/ConsoleDaemon';
import type { RouterResult, TransportSendResult, RequestResult, EventResult, LocalRoutingResult } from './RouterTypes';

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

/**
 * JTAG Transport Interface
 * 
 * Abstraction for cross-context message delivery mechanisms.
 * Implementations include WebSocket, HTTP, and in-memory transports.
 */
export interface JTAGTransport {
  name: string;
  send(message: JTAGMessage): Promise<TransportSendResult>;
  isConnected(): boolean;
  disconnect(): Promise<void>;
  reconnect?(): Promise<void>;
  setMessageHandler?(handler: (message: JTAGMessage) => void): void;
}

export interface EventsInterface {
  emit(eventName: string, data?: any): void;
  on(eventName: string, listener: (data?: any) => void): () => void;
  waitFor?(eventName: string, timeout?: number): Promise<any>;
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
}

export class JTAGRouter extends JTAGModule {
  private endpointMatcher = new EndpointMatcher<MessageSubscriber>();
  public crossContextTransport: JTAGTransport | null = null;
  
  // Built-in event system - no circular dependencies
  private eventListeners = new Map<string, Array<(data?: any) => void>>();
  
  // Bus-level enhancements
  private messageQueue: JTAGMessageQueue;
  private healthManager: ConnectionHealthManager;
  private responseCorrelator: ResponseCorrelator;
  private isInitialized = false;

  constructor(context: JTAGContext, config: { enableQueuing?: boolean; enableHealthMonitoring?: boolean } = {}) {
    super('universal-router', context);
    
    // Initialize modular bus-level features
    this.messageQueue = new JTAGMessageQueue(context, {
      enableDeduplication: true,
      deduplicationWindow: 60000, // 1 minute for console error deduplication
      maxSize: 1000,
      maxRetries: 3,
      flushInterval: 500
    });
    this.healthManager = new ConnectionHealthManager(context, this.events);
    this.responseCorrelator = new ResponseCorrelator(30000); // 30 second timeout for commands
    
    console.log(`🚀 JTAGRouter[${context.environment}]: Initialized with request-response correlation and queuing`);
  }

  async initialize(): Promise<void> {
    if (this.isInitialized) return;
    
    console.log(`🔧 ${this.toString()}: Initializing with transport and health monitoring...`);
    
    // Initialize transport
    await this.initializeTransport();
    
    // Start health monitoring
    if (this.crossContextTransport) {
      this.healthManager.setTransport(this.crossContextTransport);
      this.healthManager.startMonitoring();
    }
    
    // Start message queue processing
    this.messageQueue.startProcessing(this.flushQueuedMessages.bind(this));
    
    this.isInitialized = true;
    console.log(`✅ ${this.toString()}: Initialization complete`);
  }

  /**
   * Events interface - same pattern as commands
   */
  get events() {
    return {
      emit: (eventName: string, data?: any): void => {
        const listeners = this.eventListeners.get(eventName) || [];
        listeners.forEach(listener => {
          try {
            listener(data);
          } catch (error) {
            console.error(`Event listener error for ${eventName}:`, error);
          }
        });
      },

      on: (eventName: string, listener: (data?: any) => void): (() => void) => {
        if (!this.eventListeners.has(eventName)) {
          this.eventListeners.set(eventName, []);
        }
        this.eventListeners.get(eventName)!.push(listener);
        
        // Return unsubscribe function
        return () => {
          const listeners = this.eventListeners.get(eventName);
          if (listeners) {
            const index = listeners.indexOf(listener);
            if (index > -1) {
              listeners.splice(index, 1);
            }
          }
        };
      },

      waitFor: async (eventName: string, timeout: number = 10000): Promise<any> => {
        return new Promise((resolve, reject) => {
          const timer = setTimeout(() => {
            unsubscribe();
            reject(new Error(`Event ${eventName} timeout after ${timeout}ms`));
          }, timeout);

          const unsubscribe = this.events.on(eventName, (data) => {
            clearTimeout(timer);
            unsubscribe();
            resolve(data);
          });
        });
      }
    };
  }

  /**
   * Compatibility layer for existing eventSystem usage
   */
  get eventSystem() {
    return this.events;
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
    if (!this.crossContextTransport) {
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
      throw new Error(`Unknown message type: ${(message as any).messageType || 'undefined'}`);
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
      // Send message immediately (requests need immediate delivery)
      await this.crossContextTransport!.send(message);
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
      console.log(`🚫 ${this.toString()}: Message deduplicated (prevents flooding)`);
      return { success: true, deduplicated: true };
    }

    // For critical messages, attempt immediate delivery if healthy
    const health = this.healthManager.getHealth();
    if (priority <= MessagePriority.HIGH && health.isHealthy) {
      try {
        await this.crossContextTransport!.send(message);
        return { success: true, delivered: true };
      } catch (error) {
        console.warn(`⚠️ ${this.toString()}: Immediate delivery failed, queued for retry`);
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
      await this.crossContextTransport!.send(message);
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
    if (message.origin.includes('console') && (message.payload as ConsolePayload)?.level === 'error') {
      return MessagePriority.HIGH;
    }

    return MessagePriority.NORMAL;
  }

  /**
   * Flush queued messages (called by JTAGMessageQueue)
   */
  private async flushQueuedMessages(messages: QueuedItem<JTAGMessage>[]): Promise<QueuedItem<JTAGMessage>[]> {
    if (!this.crossContextTransport || messages.length === 0) {
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
        await this.crossContextTransport.send(queuedItem.item);
        console.log(`✅ ${this.toString()}: Delivered queued message ${queuedItem.id}`);
      } catch (error) {
        console.warn(`❌ ${this.toString()}: Failed to deliver ${queuedItem.id}`);
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


  private extractEnvironment(endpoint: string): string {
    if (endpoint.startsWith('browser/')) return 'browser';
    if (endpoint.startsWith('server/')) return 'server';
    if (endpoint.startsWith('remote/')) return 'remote';
    return this.context.environment;
  }

  /**
   * Initialize transport (called by initialize())
   */
  private async initializeTransport(): Promise<void> {
    console.log(`🔗 ${this.toString()}: Initializing transport with base64 encoding`);
    
    const transportConfig: TransportConfig = { 
      preferred: 'websocket', 
      fallback: true,
      eventSystem: this.events
    };
    this.crossContextTransport = await TransportFactory.createTransport(this.context.environment, transportConfig);
    
    // Connect transport to router for incoming messages
    if (this.crossContextTransport.setMessageHandler) {
      this.crossContextTransport.setMessageHandler((message: JTAGMessage) => {
        this.postMessage(message).catch(console.error);
      });
    }
    
    console.log(`✅ ${this.toString()}: Transport ready: ${this.crossContextTransport.name}`);
  }

  /**
   * Legacy method for compatibility
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
    
    // Disconnect transport
    if (this.crossContextTransport) {
      await this.crossContextTransport.disconnect();
      this.crossContextTransport = null;
    }
    
    this.endpointMatcher.clear();
    
    console.log(`✅ ${this.toString()}: Shutdown complete`);
  }

  /**
   * Get enhanced router status
   */
  get status(): RouterStatus {
    return {
      environment: this.context.environment,
      initialized: this.isInitialized,
      subscribers: this.endpointMatcher.size(),
      transport: this.crossContextTransport ? {
        name: this.crossContextTransport.name,
        connected: this.crossContextTransport.isConnected()
      } : null,
      queue: this.messageQueue.getStatus(),
      health: this.healthManager.getHealth()
    };
  }
}