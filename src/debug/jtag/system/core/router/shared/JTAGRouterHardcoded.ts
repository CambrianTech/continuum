/**
 * JTAGRouterHardcoded - Current Implementation Using Base Class
 * 
 * INHERITANCE HIERARCHY:
 * JTAGRouterBase (abstract base - core routing logic)
 *    ├── JTAGRouterHardcoded (this file - current hardcoded transport)
 *    └── JTAGRouterDynamic (dynamic transport implementation)
 * 
 * PURPOSE: Isolate the hardcoded transport logic into transport strategy
 * PRESERVES: All existing functionality - drop-in compatible
 * EVOLVES: Makes transport layer pluggable via strategy pattern
 */

import { JTAGRouterBase, type ITransportStrategy } from './JTAGRouterBase';
import type { JTAGContext } from '../../types/JTAGTypes';
import type { JTAGRouterConfig } from './JTAGRouterTypes';
import type { ITransportFactory, TransportConfig, JTAGTransport } from '../../../transports/shared/TransportTypes';
import { TRANSPORT_TYPES } from '../../../transports';

// ============================================================================
// HARDCODED TRANSPORT STRATEGY - CURRENT IMPLEMENTATION
// ============================================================================

/**
 * Hardcoded Transport Strategy - Uses enum-based transport map
 * This is the current implementation extracted into strategy pattern
 */
class HardcodedTransportStrategy implements ITransportStrategy {
  private transports = new Map<TRANSPORT_TYPES, JTAGTransport>();
  private router: JTAGRouterHardcoded;
  private initialized = false;

  constructor(router: JTAGRouterHardcoded) {
    this.router = router;
  }

  async initializeTransports(config?: Partial<TransportConfig>): Promise<void> {
    console.log(`🔗 ${this.router.toString()}: Initializing hardcoded transports`);
    
    // Create cross-context transport (existing logic)
    const ctxTransportConfig: TransportConfig = { 
      protocol: this.router.config.transport.preferred,
      role: this.router.config.transport.role,
      eventSystem: this.router.eventManager.events,
      sessionId: this.router.config.sessionId!,
      serverPort: this.router.config.transport.serverPort,
      serverUrl: this.router.config.transport.serverUrl,
      fallback: this.router.config.transport.fallback,
      handler: this.router
    };
    
    const factory = await this.router.getTransportFactory();
    const crossContextTransport = await factory.createTransport(
      this.router.context.environment, 
      ctxTransportConfig
    );
    this.transports.set(TRANSPORT_TYPES.CROSS_CONTEXT, crossContextTransport);

    // P2P transport still disabled (preserve existing behavior)
    // Lines from original: 637-646 commented out
    
    this.initialized = true;
    await this.setupTransportMessageHandlers();
  }

  getCrossContextTransport(): JTAGTransport | undefined {
    return this.transports.get(TRANSPORT_TYPES.CROSS_CONTEXT);
  }

  getP2PTransport(): JTAGTransport | undefined {
    return this.transports.get(TRANSPORT_TYPES.P2P);
  }

  async setupTransportMessageHandlers(): Promise<void> {
    for (const transport of this.transports.values()) {
      if (transport.setMessageHandler) {
        transport.setMessageHandler((message) => {
          this.router.postMessage(message).catch(console.error);
        });
        console.log(`✅ ${this.router.toString()}: Transport ready: ${transport.name}`);
      }
    }
  }

  async shutdownAllTransports(): Promise<void> {
    console.log(`🔄 ${this.router.toString()}: Shutting down hardcoded transports...`);
    
    for (const transport of this.transports.values()) {
      await transport.disconnect();
    }
    this.transports.clear();
    
    console.log(`✅ ${this.router.toString()}: Hardcoded transport shutdown complete`);
  }

  getTransportStatusInfo() {
    return {
      initialized: this.initialized,
      transportCount: this.transports.size,
      transports: Array.from(this.transports.entries()).map(([type, transport]) => ({
        name: transport.name,
        connected: transport.isConnected(),
        type: type.toString()
      }))
    };
  }
}

// ============================================================================
// HARDCODED ROUTER IMPLEMENTATION
// ============================================================================

/**
 * JTAGRouterHardcoded - Current router using hardcoded transport strategy
 * 
 * SAME FUNCTIONALITY: Identical to original JTAGRouter
 * DIFFERENT STRUCTURE: Uses base class + strategy pattern
 * DROP-IN REPLACEMENT: Can replace original JTAGRouter
 */
export class JTAGRouterHardcoded extends JTAGRouterBase {
  protected transportStrategy: ITransportStrategy;

  constructor(context: JTAGContext, config: JTAGRouterConfig = {}) {
    super(context, config);
    
    // Use hardcoded transport strategy
    this.transportStrategy = new HardcodedTransportStrategy(this);
    
    console.log(`🚀 JTAGRouterHardcoded[${context.environment}]: Initialized with hardcoded transports`);
  }

  /**
   * Initialize transport strategy
   */
  protected async initializeTransportStrategy(): Promise<void> {
    await this.transportStrategy.initializeTransports();
  }

  /**
   * Get environment-specific transport factory (abstract method implementation)
   */
  protected abstract getTransportFactory(): Promise<ITransportFactory>;

  /**
   * Access to hardcoded transports for compatibility
   */
  get transports(): Map<TRANSPORT_TYPES, JTAGTransport> {
    const strategy = this.transportStrategy as HardcodedTransportStrategy;
    return (strategy as any).transports;
  }
}

// ============================================================================
// MIGRATION COMPATIBILITY
// ============================================================================

/**
 * Type alias for backward compatibility
 * Eventually JTAGRouter can point to this implementation
 */
export { JTAGRouterHardcoded as JTAGRouter };

/**
 * Factory function for creating hardcoded router
 */
export async function createHardcodedRouter(
  context: JTAGContext,
  config: JTAGRouterConfig = {}
): Promise<JTAGRouterHardcoded> {
  const router = new JTAGRouterHardcoded(context, config);
  await router.initialize();
  return router;
}

// ============================================================================
// EVOLUTION PATH DOCUMENTATION
// ============================================================================

export const EvolutionPath = {
  current: `
    // Current usage (unchanged):
    const router = new JTAGRouter(context, config);
    await router.initialize();
    
    // Screenshot → file/save promise chain works unchanged
    const result = await jtagSystem.commands.screenshot();
  `,
  
  step1: `
    // Step 1: Use base class architecture
    const router = new JTAGRouterHardcoded(context, config);
    await router.initialize();
    
    // Everything works exactly the same
    // But now transport layer is isolated
  `,
  
  step2: `
    // Step 2: Switch to dynamic router when ready
    const router = new JTAGRouterDynamic(context, config);
    await router.initialize();
    
    // Same API, but now supports unlimited transports
    // P2P mesh networking enabled
    // Runtime transport discovery
  `,
  
  step3: `
    // Step 3: Replace original JTAGRouter entirely (optional)
    // export { JTAGRouterDynamic as JTAGRouter };
    
    // All existing code continues to work
    // But now has dynamic capabilities
  `
};`