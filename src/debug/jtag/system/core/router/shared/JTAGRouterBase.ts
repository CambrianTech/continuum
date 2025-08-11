/**
 * JTAGRouterBase - Base Router with Abstract Transport Factory
 * 
 * Minimal base class that only abstracts transport factory selection.
 * All routing logic stays in concrete implementations.
 */

import { JTAGModule } from '../../shared/JTAGModule';
import type { JTAGContext, JTAGMessage } from '../../types/JTAGTypes';
import type { ITransportFactory, JTAGTransport } from '../../../transports';
import { TRANSPORT_TYPES } from '../../../transports';
import type { JTAGRouterConfig } from './JTAGRouterTypes';
import type { JTAGResponsePayload } from '../../types/ResponseTypes';
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

/**
 * Message Subscriber Interface - Core contract for message handling
 */
export interface MessageSubscriber {
  handleMessage(message: JTAGMessage): Promise<JTAGResponsePayload>;
  get endpoint(): string;
  get uuid(): string;
}

export abstract class JTAGRouterBase extends JTAGModule {
  
  // Core subscriber management (moved from JTAGRouter)  
  protected readonly endpointMatcher = new EndpointMatcher<MessageSubscriber>();
  
  // Transport management pattern (moved from JTAGRouter)
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
  
  constructor(name: string, context: JTAGContext, config: JTAGRouterConfig = {}) {
    super(name, context);
    
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

  /**
   * Shared strategy initialization logic - eliminates duplication
   */
  protected initializeStrategies(config: JTAGRouterConfig): void {
    // EVOLUTION: Dynamic is now DEFAULT - explicit opt-out to legacy
    const forceLegacy = config.transport?.forceLegacy === true ||
                       config.transport?.strategy === 'hardcoded' ||
                       (typeof process !== 'undefined' && process.env?.JTAG_FORCE_LEGACY === 'true');
    
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
   * Register subscriber for message routing (moved from JTAGRouter)
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
    return matchResult?.subscriber || null;
  }

  // Pure utility methods moved to RouterUtilities class
  // Object-specific methods that need 'this' context remain here

  /**
   * Get environment-specific transport factory
   */
  protected abstract getTransportFactory(): Promise<ITransportFactory>;

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
   * Initialize the router - to be implemented by concrete routers
   */
  abstract initialize(): Promise<void>;

  /**
   * Shutdown the router - to be implemented by concrete routers  
   */
  abstract shutdown(): Promise<void>;
}