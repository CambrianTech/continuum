/**
 * JTAG Router Server - Server-specific router implementation
 * 
 * Extends shared JTAGRouter with server-only transport factory.
 * Creates TransportFactoryServer for server environment.
 */

import { JTAGRouter } from '../shared/JTAGRouter';
import type { ITransportFactory } from '../../../transports';
import { TransportFactoryServer } from '../../../transports/server/TransportFactoryServer';
import { HardcodedTransportStrategy } from '../shared/HardcodedTransportStrategy';
import { DynamicTransportStrategy } from '../shared/DynamicTransportStrategy';
import type { JTAGContext } from '../../types/JTAGTypes';
import type { JTAGRouterConfig } from '../shared/JTAGRouterTypes';
import { MinimalEnhancementStrategy, LegacyEnhancementStrategy } from '../shared/enhancements/RouterEnhancementStrategy';

export class JTAGRouterServer extends JTAGRouter {
  
  // Extensible transport strategy - supports both hardcoded and dynamic (P2P ready)
  protected transportStrategy: HardcodedTransportStrategy | DynamicTransportStrategy;
  
  // Enhancement strategy - choose minimal (dynamic-style) or legacy (full features)
  protected enhancementStrategy: MinimalEnhancementStrategy | LegacyEnhancementStrategy;
  
  constructor(context: JTAGContext, config: JTAGRouterConfig = {}) {
    super(context, config);
    
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
      console.log(`📡 ${this.toString()}: Using hardcoded transport strategy (legacy - explicitly requested)`);
      console.warn(`⚠️ ${this.toString()}: DEPRECATION NOTICE - You've opted into legacy transport strategy. This will be removed in future versions. Migration guide: remove 'forceLegacy: true' from config.`);
      this.transportStrategy = new HardcodedTransportStrategy(this.transports);
      // Use legacy enhancements with hardcoded strategy (existing JTAGRouter pattern)
      this.enhancementStrategy = new LegacyEnhancementStrategy();
    }
  }

  /**
   * Get server-specific transport factory
   */
  protected async getTransportFactory(): Promise<ITransportFactory> {
    return new TransportFactoryServer();
  }
}