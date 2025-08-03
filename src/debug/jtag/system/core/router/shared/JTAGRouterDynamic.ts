/**
 * JTAGRouterDynamic - Experimental Dynamic Transport Router
 * 
 * PURPOSE: Test dynamic transport capabilities alongside working JTAGRouter
 * FEATURES: P2P discovery, intelligent transport selection, health monitoring
 * EVOLUTION: Will become the main router once P2P features are proven
 */

import { JTAGRouterBase } from './JTAGRouterBase';
import { DynamicTransportStrategy } from './DynamicTransportStrategy';
import type { JTAGContext, JTAGMessage } from '../../types/JTAGTypes';
import { JTAGMessageTypes } from '../../types/JTAGTypes';
import type { JTAGResponsePayload } from '../../types/ResponseTypes';
import type { ITransportFactory, TransportConfig } from '../../../transports';
import type { JTAGRouterConfig } from './JTAGRouterTypes';

/**
 * Basic transport info from ITransportStrategy interface
 */
interface BasicTransportInfo {
  readonly name: string;
  readonly connected: boolean;
  readonly type: string;
}

/**
 * Extended transport info from DynamicTransportStrategy
 */
interface ExtendedTransportInfo extends BasicTransportInfo {
  readonly health: {
    readonly latency: number;
    readonly errorCount: number;
    readonly lastActivity: string;
  } | null;
}

/**
 * Dynamic router status interface - handles both basic and extended transport info
 */
interface DynamicRouterStatus {
  readonly environment: string;
  readonly transportStrategy: 'dynamic';
  readonly initialized: boolean;
  readonly transportCount: number;
  readonly transports: Array<BasicTransportInfo | ExtendedTransportInfo>;
  readonly p2pEnabled: boolean;
  readonly discovery: {
    readonly available: string[];
    readonly preferred: string;
    readonly fallbacks: string[];
    readonly p2pCapable: boolean;
  };
}

/**
 * JTAGRouterDynamic - Advanced Router with P2P and Dynamic Transport Selection
 * 
 * EXPERIMENTAL FEATURES:
 * - Dynamic transport discovery and selection
 * - P2P mesh networking capabilities  
 * - Health-based transport failover
 * - Load balancing across multiple transports
 * - Real-time transport status monitoring
 */
export class JTAGRouterDynamic extends JTAGRouterBase {
  
  // All properties inherited from JTAGRouterBase: endpointMatcher, transports, 
  // transportStrategy, enhancementStrategy, config, eventManager, messageQueue, 
  // healthManager, responseCorrelator, isInitialized

  constructor(context: JTAGContext, config: JTAGRouterConfig = {}) {
    super('dynamic-router', context, config);
    // All initialization is now handled by JTAGRouterBase constructor
    // Dynamic strategy is automatically selected based on configuration
    console.log(`🚀 JTAGRouterDynamic[${context.environment}]: Initialized via base constructor with dynamic capabilities`);
  }

  /**
   * Initialize the dynamic router with advanced features
   */
  async initialize(): Promise<void> {
    if (this.isInitialized) return;
    
    console.log(`🔧 ${this.toString()}: Initializing dynamic router with P2P discovery...`);
    
    // Initialize dynamic transport strategy
    await this.initializeTransport();
    
    this.isInitialized = true;
    console.log(`✅ ${this.toString()}: Dynamic initialization complete`);
    
    // Log enhanced status
    this.logEnhancedStatus();
  }

  /**
   * Initialize transport using dynamic strategy
   */
  async initializeTransport(_config?: Partial<TransportConfig>): Promise<void> {
    console.log(`🔗 ${this.toString()}: Initializing dynamic transport system...`);
    
    // Create transport configuration (same pattern as JTAGRouter)
    const transportConfig: TransportConfig = { 
      protocol: this.config.transport.preferred,
      role: this.config.transport.role,
      sessionId: this.config.sessionId!,
      serverPort: this.config.transport.serverPort,
      serverUrl: this.config.transport.serverUrl,
      fallback: this.config.transport.fallback,
      eventSystem: this.eventManager.events,
      handler: {
        handleTransportMessage: (message: JTAGMessage) => this.handleDynamicMessage(message),
        transportId: this.context.uuid
      }
    };
    
    // Get environment-specific transport factory
    const factory = await this.getTransportFactory();
    
    // Initialize transports using dynamic strategy
    await this.transportStrategy.initializeTransports(factory, this.context, transportConfig);

    // Setup enhanced message handlers
    await this.transportStrategy.setupMessageHandlers((message: JTAGMessage) => {
      this.handleDynamicMessage(message).catch(console.error);
    });
  }

  /**
   * Handle messages with dynamic routing logic
   */
  private async handleDynamicMessage(message: JTAGMessage): Promise<JTAGResponsePayload> {
    console.log(`📨 JTAGRouterDynamic: Processing message with dynamic routing: ${message.endpoint}`);
    
    // TODO: Implement intelligent message routing
    // - Check if message should go to P2P network
    // - Select optimal transport based on health/latency
    // - Implement smart retry logic
    
    // For now, log the message for debugging
    const messageType = JTAGMessageTypes.isRequest(message) ? 'request' : 
                       JTAGMessageTypes.isResponse(message) ? 'response' : 'event';
    console.log(`🎯 JTAGRouterDynamic: Message type: ${messageType}, origin: ${message.origin}`);
    
    // Return proper response payload for ITransportHandler compliance
    return {
      success: true,
      timestamp: new Date().toISOString(),
      context: this.context,
      sessionId: this.context.uuid
    };
  }

  /**
   * Get router status with dynamic transport information
   */
  get dynamicStatus(): DynamicRouterStatus {
    const transportStatus = this.transportStrategy.getTransportStatusInfo();
    
    // Type-safe check for DynamicTransportStrategy
    if (this.transportStrategy instanceof DynamicTransportStrategy) {
      // Use the DynamicTransportStrategy's extended getTransportStatusInfo method
      const extendedStatus = this.transportStrategy.getTransportStatusInfo();
      return {
        ...extendedStatus, // Already includes p2pEnabled and discovery
        environment: this.context.environment,
        transportStrategy: 'dynamic' as const
      };
    }
    
    // Fallback for non-dynamic strategies (shouldn't happen in JTAGRouterDynamic)
    return {
      ...transportStatus,
      environment: this.context.environment,
      transportStrategy: 'dynamic' as const,
      p2pEnabled: false,
      discovery: {
        available: [],
        preferred: 'websocket',
        fallbacks: [],
        p2pCapable: false
      }
    };
  }

  /**
   * Log enhanced status information
   */
  private logEnhancedStatus(): void {
    const status = this.dynamicStatus;
    console.log(`📊 ${this.toString()}: Enhanced Status Report`);
    console.log(`   Environment: ${status.environment}`);
    console.log(`   Transports: ${status.transportCount} active`);
    console.log(`   P2P Enabled: ${status.p2pEnabled}`);
    console.log(`   Discovery: ${JSON.stringify(status.discovery)}`);
  }

  /**
   * Shutdown dynamic router
   */
  async shutdown(): Promise<void> {
    console.log(`🔄 ${this.toString()}: Shutting down dynamic router...`);
    
    // Shutdown dynamic transport strategy
    await this.transportStrategy.shutdownAllTransports();
    
    this.isInitialized = false;
    console.log(`✅ ${this.toString()}: Dynamic shutdown complete`);
  }

  // getTransportStatus() inherited from JTAGRouterBase ✅
  // Can override if dynamic enhancements needed: this.transportStrategy.getTransportStatusInfo()

  /**
   * Get environment-specific transport factory - to be implemented by concrete classes
   */
  protected async getTransportFactory(): Promise<ITransportFactory> {
    // This will be implemented by JTAGRouterDynamicServer/JTAGRouterDynamicBrowser
    // For now, we'll use the same pattern as the base JTAGRouter
    throw new Error('JTAGRouterDynamic: getTransportFactory must be implemented by concrete subclass');
  }
}