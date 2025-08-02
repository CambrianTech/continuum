/**
 * HardcodedTransportStrategy - Wraps Existing Transport Map Pattern
 * 
 * PURPOSE: Extract transport logic from monolithic JTAGRouter into strategy
 * PRESERVES: Exact same behavior as existing hardcoded transport handling
 * ENABLES: Future replacement with DynamicTransportStrategy for P2P
 */

import { TRANSPORT_TYPES } from '../../../transports';
import type { JTAGTransport, TransportConfig, ITransportFactory } from '../../../transports';
import type { JTAGMessage, JTAGContext } from '../../types/JTAGTypes';

/**
 * Transport Strategy Interface - Abstracts transport management
 */
export interface ITransportStrategy {
  initializeTransports(factory: ITransportFactory, context: JTAGContext, config: TransportConfig): Promise<void>;
  getCrossContextTransport(): JTAGTransport | undefined;
  getP2PTransport(): JTAGTransport | undefined;
  setupMessageHandlers(messageHandler: (message: JTAGMessage) => void): Promise<void>;
  shutdownAllTransports(): Promise<void>;
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

/**
 * Hardcoded Transport Strategy - Preserves Existing Enum-Based Pattern
 */
export class HardcodedTransportStrategy implements ITransportStrategy {
  
  constructor(private readonly transports: Map<TRANSPORT_TYPES, JTAGTransport>) {
    // Wraps existing transports map - zero behavior change
  }

  /**
   * Initialize transports - preserves exact existing pattern from JTAGRouter.initializeTransport()
   */
  async initializeTransports(factory: ITransportFactory, context: JTAGContext, config: TransportConfig): Promise<void> {
    console.log('🔗 HardcodedTransportStrategy: Initializing cross-context transport');
    
    // Create cross-context transport using exact same logic as original
    const crossContextTransport = await factory.createTransport(context.environment, config);
    this.transports.set(TRANSPORT_TYPES.CROSS_CONTEXT, crossContextTransport);

    // P2P transport disabled (preserves original comment/logic)
    // See ENHANCEMENT issue in JTAGRouter for UDP multicast requirements
    console.log('✅ HardcodedTransportStrategy: Cross-context transport initialized');
  }

  /**
   * Get cross-context transport - direct enum lookup
   */
  getCrossContextTransport(): JTAGTransport | undefined {
    return this.transports.get(TRANSPORT_TYPES.CROSS_CONTEXT);
  }

  /**
   * Get P2P transport - direct enum lookup (currently undefined)
   */
  getP2PTransport(): JTAGTransport | undefined {
    return this.transports.get(TRANSPORT_TYPES.P2P);
  }

  /**
   * Setup message handlers - preserves exact pattern from original
   */
  async setupMessageHandlers(messageHandler: (message: JTAGMessage) => void): Promise<void> {
    for (const transport of this.transports.values()) {
      if (transport.setMessageHandler) {
        transport.setMessageHandler(messageHandler);
        console.log(`✅ HardcodedTransportStrategy: Handler set for: ${transport.name}`);
      }
    }
  }

  /**
   * Shutdown all transports - preserves exact pattern
   */
  async shutdownAllTransports(): Promise<void> {
    console.log('🔄 HardcodedTransportStrategy: Shutting down transports...');
    
    for (const transport of this.transports.values()) {
      if (transport.disconnect) {
        await transport.disconnect();
      }
    }
    this.transports.clear();
    
    console.log('✅ HardcodedTransportStrategy: Shutdown complete');
  }

  /**
   * Get transport status - preserves exact format
   */
  getTransportStatusInfo() {
    return {
      initialized: this.transports.size > 0,
      transportCount: this.transports.size,
      transports: Array.from(this.transports.entries()).map(([type, transport]) => ({
        name: transport.name || type,
        connected: transport.isConnected ? transport.isConnected() : false,
        type: type
      }))
    };
  }
}