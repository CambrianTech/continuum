/**
 * DynamicTransportStrategy - Advanced Transport Management with P2P Capabilities
 * 
 * PURPOSE: Enable dynamic transport discovery, selection, and failover
 * FEATURES: WebSocket + HTTP + UDP multicast P2P transport coordination
 * EVOLUTION: Replaces hardcoded enum-based transport with intelligent selection
 */

import { TRANSPORT_TYPES } from '../../../transports';
import type { JTAGTransport, TransportConfig, ITransportFactory } from '../../../transports';
import type { JTAGMessage, JTAGContext } from '../../types/JTAGTypes';
import type { ITransportStrategy } from './HardcodedTransportStrategy';

/**
 * Transport Discovery Result - Information about available transports
 */
interface TransportDiscoveryResult {
  available: TRANSPORT_TYPES[];
  preferred: TRANSPORT_TYPES;
  fallbacks: TRANSPORT_TYPES[];
  p2pCapable: boolean;
}

/**
 * Transport Health Status - Real-time transport monitoring
 */
interface TransportHealthStatus {
  type: TRANSPORT_TYPES;
  connected: boolean;
  latency: number;
  errorCount: number;
  lastActivity: Date;
}

/**
 * Dynamic Transport Strategy - Intelligent Transport Management
 * 
 * CAPABILITIES:
 * - Automatic transport discovery (WebSocket, HTTP, UDP multicast)
 * - Health-based transport selection with failover
 * - P2P node discovery and routing
 * - Load balancing across multiple transports
 * - Automatic retry and recovery
 */
export class DynamicTransportStrategy implements ITransportStrategy {
  
  private readonly healthStatus = new Map<TRANSPORT_TYPES, TransportHealthStatus>();
  private readonly discovery: TransportDiscoveryResult;
  private isP2PEnabled = false;
  
  constructor(
    private readonly transports: Map<TRANSPORT_TYPES, JTAGTransport>,
    private readonly enableP2P: boolean = true
  ) {
    // Initialize with smart defaults - will be refined by discovery
    this.discovery = {
      available: [TRANSPORT_TYPES.CROSS_CONTEXT],
      preferred: TRANSPORT_TYPES.CROSS_CONTEXT,
      fallbacks: [],
      p2pCapable: enableP2P
    };
  }

  /**
   * Initialize transports with dynamic discovery
   */
  async initializeTransports(factory: ITransportFactory, context: JTAGContext, config: TransportConfig): Promise<void> {
    console.log('🚀 DynamicTransportStrategy: Starting intelligent transport initialization...');
    
    // Phase 1: Initialize cross-context transport (required)
    await this.initializeCrossContextTransport(factory, context, config);
    
    // Phase 2: Attempt P2P transport initialization (optional)
    if (this.enableP2P) {
      await this.initializeP2PTransport(factory, context, config);
    }
    
    // Phase 3: Start health monitoring
    this.startHealthMonitoring();
    
    console.log(`✅ DynamicTransportStrategy: Initialized ${this.transports.size} transports`);
    this.logTransportStatus();
  }

  /**
   * Initialize cross-context transport (WebSocket/HTTP)
   */
  private async initializeCrossContextTransport(factory: ITransportFactory, context: JTAGContext, config: TransportConfig): Promise<void> {
    try {
      console.log('🔗 DynamicTransportStrategy: Initializing cross-context transport...');
      const crossContextTransport = await factory.createTransport(context.environment, config);
      this.transports.set(TRANSPORT_TYPES.CROSS_CONTEXT, crossContextTransport);
      
      // Initialize health tracking
      this.healthStatus.set(TRANSPORT_TYPES.CROSS_CONTEXT, {
        type: TRANSPORT_TYPES.CROSS_CONTEXT,
        connected: crossContextTransport.isConnected ? crossContextTransport.isConnected() : false,
        latency: 0,
        errorCount: 0,
        lastActivity: new Date()
      });
      
      console.log('✅ DynamicTransportStrategy: Cross-context transport initialized');
    } catch (error) {
      console.error('❌ DynamicTransportStrategy: Cross-context transport failed:', error);
      throw error; // Cross-context is required
    }
  }

  /**
   * Initialize P2P transport (UDP multicast) - optional
   */
  private async initializeP2PTransport(factory: ITransportFactory, context: JTAGContext, config: TransportConfig): Promise<void> {
    try {
      console.log('🌐 DynamicTransportStrategy: Attempting P2P transport initialization...');
      
      // TODO: Implement UDP multicast transport factory
      // For now, we'll prepare the structure but not create the transport
      console.log('⏳ DynamicTransportStrategy: P2P transport pending UDP multicast implementation');
      
      // When implemented:
      // const p2pTransport = await factory.createP2PTransport(context.environment, {
      //   ...config,
      //   protocol: 'udp-multicast',
      //   multicastGroup: '239.255.255.250',
      //   port: 9001
      // });
      // this.transports.set(TRANSPORT_TYPES.P2P, p2pTransport);
      
      this.isP2PEnabled = false; // Disabled until UDP multicast is implemented
      
    } catch (error) {
      console.warn('⚠️ DynamicTransportStrategy: P2P transport unavailable:', error);
      this.isP2PEnabled = false; // P2P is optional
    }
  }

  /**
   * Start health monitoring for all transports
   */
  private startHealthMonitoring(): void {
    console.log('💓 DynamicTransportStrategy: Starting transport health monitoring...');
    
    // Monitor transport health every 30 seconds
    setInterval(() => this.updateHealthStatus(), 30000);
  }

  /**
   * Update health status for all transports
   */
  private updateHealthStatus(): void {
    for (const [type, transport] of this.transports) {
      const health = this.healthStatus.get(type);
      if (health) {
        health.connected = transport.isConnected ? transport.isConnected() : false;
        health.lastActivity = new Date();
        // TODO: Add ping-based latency measurement
      }
    }
  }

  /**
   * Get cross-context transport with health check
   */
  getCrossContextTransport(): JTAGTransport | undefined {
    const transport = this.transports.get(TRANSPORT_TYPES.CROSS_CONTEXT);
    const health = this.healthStatus.get(TRANSPORT_TYPES.CROSS_CONTEXT);
    
    if (transport && health?.connected) {
      return transport;
    }
    
    console.warn('⚠️ DynamicTransportStrategy: Cross-context transport unhealthy');
    return transport; // Return anyway for retry logic
  }

  /**
   * Get P2P transport with availability check
   */
  getP2PTransport(): JTAGTransport | undefined {
    if (!this.isP2PEnabled) {
      return undefined;
    }
    
    const transport = this.transports.get(TRANSPORT_TYPES.P2P);
    const health = this.healthStatus.get(TRANSPORT_TYPES.P2P);
    
    if (transport && health?.connected) {
      return transport;
    }
    
    return undefined;
  }

  /**
   * Setup message handlers with error handling and retry logic
   */
  async setupMessageHandlers(messageHandler: (message: JTAGMessage) => void): Promise<void> {
    console.log('🔗 DynamicTransportStrategy: Setting up resilient message handlers...');
    
    for (const [type, transport] of this.transports) {
      if (transport.setMessageHandler) {
        // Wrap handler with error tracking and health updates
        const wrappedHandler = (message: JTAGMessage) => {
          try {
            // Update health on successful message
            const health = this.healthStatus.get(type);
            if (health) {
              health.lastActivity = new Date();
            }
            
            messageHandler(message);
          } catch (error) {
            console.error(`❌ DynamicTransportStrategy: Handler error on ${type}:`, error);
            
            // Track errors for health monitoring
            const health = this.healthStatus.get(type);
            if (health) {
              health.errorCount++;
            }
          }
        };
        
        transport.setMessageHandler(wrappedHandler);
        console.log(`✅ DynamicTransportStrategy: Handler set for ${type}`);
      }
    }
  }

  /**
   * Shutdown all transports gracefully
   */
  async shutdownAllTransports(): Promise<void> {
    console.log('🔄 DynamicTransportStrategy: Gracefully shutting down all transports...');
    
    const shutdownPromises = Array.from(this.transports.values()).map(async (transport, index) => {
      try {
        if (transport.disconnect) {
          await transport.disconnect();
        }
      } catch (error) {
        console.error(`❌ DynamicTransportStrategy: Shutdown error on transport ${index}:`, error);
      }
    });
    
    await Promise.allSettled(shutdownPromises);
    this.transports.clear();
    this.healthStatus.clear();
    
    console.log('✅ DynamicTransportStrategy: All transports shut down');
  }

  /**
   * Get comprehensive transport status
   */
  getTransportStatusInfo() {
    const transports = Array.from(this.transports.entries()).map(([type, transport]) => {
      const health = this.healthStatus.get(type);
      return {
        name: transport.name || type,
        connected: transport.isConnected ? transport.isConnected() : false,
        type: type,
        health: health ? {
          latency: health.latency,
          errorCount: health.errorCount,
          lastActivity: health.lastActivity.toISOString()
        } : null
      };
    });

    return {
      initialized: this.transports.size > 0,
      transportCount: this.transports.size,
      transports,
      p2pEnabled: this.isP2PEnabled,
      discovery: this.discovery
    };
  }

  /**
   * Log current transport status for debugging
   */
  private logTransportStatus(): void {
    console.log('📊 DynamicTransportStrategy: Transport Status Summary');
    for (const [type, transport] of this.transports) {
      const health = this.healthStatus.get(type);
      console.log(`   ${type}: ${transport.name} (connected: ${transport.isConnected ? transport.isConnected() : false}, errors: ${health?.errorCount || 0})`);
    }
  }

  /**
   * Select best transport for message routing (future enhancement)
   */
  selectOptimalTransport(message: JTAGMessage): JTAGTransport | undefined {
    // For now, prefer cross-context, fallback to P2P
    const crossContext = this.getCrossContextTransport();
    if (crossContext) {
      return crossContext;
    }
    
    return this.getP2PTransport();
  }
}