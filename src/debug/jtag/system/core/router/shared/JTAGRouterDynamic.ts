/**
 * JTAGRouterDynamic - Experimental Dynamic Transport Router
 * 
 * PURPOSE: Test dynamic transport capabilities alongside working JTAGRouter
 * FEATURES: P2P discovery, intelligent transport selection, health monitoring
 * EVOLUTION: Will become the main router once P2P features are proven
 */

import { JTAGRouterBase } from './JTAGRouterBase';
import { DynamicTransportStrategy } from './DynamicTransportStrategy';
import { TRANSPORT_TYPES } from '../../../transports';
import type { JTAGContext, JTAGMessage } from '../../types/JTAGTypes';
import type { ITransportFactory, JTAGTransport, TransportConfig } from '../../../transports';
import type { JTAGRouterConfig } from './JTAGRouterTypes';
import { createJTAGRouterConfig } from './JTAGRouterTypes';

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
  
  //Use a map for transports (matches JTAGRouter pattern)
  protected readonly transports = new Map<TRANSPORT_TYPES, JTAGTransport>();
  
  // Dynamic transport strategy (enhanced vs HardcodedTransportStrategy)
  protected transportStrategy: DynamicTransportStrategy;
  
  private readonly config;
  private isInitialized = false;

  constructor(context: JTAGContext, config: JTAGRouterConfig = {}) {
    super('dynamic-router', context, config);
    
    // Apply configuration with defaults
    this.config = createJTAGRouterConfig(config);
    
    // Initialize dynamic transport strategy with P2P enabled
    this.transportStrategy = new DynamicTransportStrategy(this.transports, true);
    
    console.log(`🚀 JTAGRouterDynamic[${context.environment}]: Initialized with P2P capabilities`);
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
  async initializeTransport(config?: Partial<TransportConfig>): Promise<void> {
    console.log(`🔗 ${this.toString()}: Initializing dynamic transport system...`);
    
    // Create transport configuration (same pattern as JTAGRouter)
    const transportConfig: TransportConfig = { 
      protocol: this.config.transport.preferred,
      role: this.config.transport.role,
      sessionId: this.config.sessionId!,
      serverPort: this.config.transport.serverPort,
      serverUrl: this.config.transport.serverUrl,
      fallback: this.config.transport.fallback,
      handler: this // Assuming this implements ITransportHandler
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
  private async handleDynamicMessage(message: JTAGMessage): Promise<void> {
    console.log(`📨 JTAGRouterDynamic: Processing message with dynamic routing: ${message.endpoint}`);
    
    // TODO: Implement intelligent message routing
    // - Check if message should go to P2P network
    // - Select optimal transport based on health/latency
    // - Implement smart retry logic
    
    // For now, log the message for debugging
    console.log(`🎯 JTAGRouterDynamic: Message type: ${message.type}, origin: ${message.origin}`);
  }

  /**
   * Get router status with dynamic transport information
   */
  get dynamicStatus() {
    const transportStatus = this.transportStrategy.getTransportStatusInfo();
    
    return {
      environment: this.context.environment,
      initialized: this.isInitialized,
      transportStrategy: 'dynamic',
      ...transportStatus
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

  /**
   * Get environment-specific transport factory - to be implemented by concrete classes
   */
  protected async getTransportFactory(): Promise<ITransportFactory> {
    // This will be implemented by JTAGRouterDynamicServer/JTAGRouterDynamicBrowser
    // For now, we'll use the same pattern as the base JTAGRouter
    throw new Error('JTAGRouterDynamic: getTransportFactory must be implemented by concrete subclass');
  }
}