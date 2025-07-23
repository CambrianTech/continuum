/**
 * JTAG Universal Router - Context-Aware Message Routing with Bus-Level Queuing
 */

import { JTAGModule } from './JTAGModule';
import { JTAGContext, JTAGMessage, JTAGPayload, JTAGMessageUtils, JTAGMessageTypes, JTAGMessageFactory } from './JTAGTypes';
import { TransportFactory, TransportConfig } from '../transports/TransportFactory';
import { JTAGEventSystem } from './JTAGEventSystem';
import { JTAGMessageQueue, MessagePriority } from './queuing/JTAGMessageQueue';
import { QueuedItem } from './queuing/PriorityQueue';
import { ConnectionHealthManager, ConnectionState } from './ConnectionHealthManager';
import { ResponseCorrelator } from './ResponseCorrelator';

export interface MessageSubscriber {
  handleMessage(message: JTAGMessage): Promise<any>;
  get endpoint(): string;
  get uuid(): string;
}

export interface JTAGTransport {
  name: string;
  send(message: JTAGMessage): Promise<any>;
  isConnected(): boolean;
  disconnect(): Promise<void>;
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
  private subscribers = new Map<string, MessageSubscriber>();
  private crossContextTransport: JTAGTransport | null = null;
  public eventSystem: JTAGEventSystem;
  
  // Bus-level enhancements
  private messageQueue: JTAGMessageQueue;
  private healthManager: ConnectionHealthManager;
  private responseCorrelator: ResponseCorrelator;
  private isInitialized = false;

  constructor(context: JTAGContext, config: { enableQueuing?: boolean; enableHealthMonitoring?: boolean } = {}) {
    super('universal-router', context);
    this.eventSystem = new JTAGEventSystem(context, this);
    
    // Initialize modular bus-level features
    this.messageQueue = new JTAGMessageQueue(context, {
      enableDeduplication: true,
      deduplicationWindow: 60000, // 1 minute for console error deduplication
      maxSize: 1000,
      maxRetries: 3,
      flushInterval: 500
    });
    this.healthManager = new ConnectionHealthManager(context, this.eventSystem);
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

  registerSubscriber(endpoint: string, subscriber: MessageSubscriber): void {
    const fullEndpoint = `${this.context.environment}/${endpoint}`;
    this.subscribers.set(fullEndpoint, subscriber);
    this.subscribers.set(endpoint, subscriber);
    console.log(`📋 ${this.toString()}: Registered subscriber at ${fullEndpoint}`);
  }

  async postMessage(message: JTAGMessage): Promise<any> {
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
  private async routeRemotelyWithQueue(message: JTAGMessage): Promise<any> {
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
      // RESPONSE PATTERN: Should not happen here, but handle gracefully
      throw new Error('Response messages should not be routed remotely');
    } else {
      throw new Error(`Unknown message type: ${(message as any).messageType}`);
    }
  }

  /**
   * Handle request messages that need responses (screenshot, etc.)
   */
  private async handleRequestMessage(message: JTAGMessage): Promise<any> {
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
      return response;

    } catch (error) {
      console.error(`❌ ${this.toString()}: Request failed:`, error);
      throw error;
    }
  }

  /**
   * Handle event messages (fire-and-forget: console logs, notifications, etc.)
   */
  private async handleEventMessage(message: JTAGMessage): Promise<any> {
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
    if (message.origin.includes('console') && (message.payload as any)?.level === 'error') {
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

  private async routeLocally(message: JTAGMessage): Promise<any> {
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

    // Regular message routing for events and requests
    const subscriber = this.subscribers.get(message.endpoint);
    if (!subscriber) {
      throw new Error(`No subscriber found for endpoint: ${message.endpoint}`);
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
    
    return result;
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
    
    const transportConfig: TransportConfig = { preferred: 'websocket', fallback: true };
    this.crossContextTransport = await TransportFactory.createTransport(this.context.environment, transportConfig);
    
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
    
    this.subscribers.clear();
    
    console.log(`✅ ${this.toString()}: Shutdown complete`);
  }

  /**
   * Get enhanced router status
   */
  get status(): RouterStatus {
    return {
      environment: this.context.environment,
      initialized: this.isInitialized,
      subscribers: this.subscribers.size,
      transport: this.crossContextTransport ? {
        name: this.crossContextTransport.name,
        connected: this.crossContextTransport.isConnected()
      } : null,
      queue: this.messageQueue.getStatus(),
      health: this.healthManager.getHealth()
    };
  }
}