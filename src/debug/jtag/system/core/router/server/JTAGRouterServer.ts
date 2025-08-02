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

export class JTAGRouterServer extends JTAGRouter {
  
  // Extensible transport strategy - supports both hardcoded and dynamic (P2P ready)
  protected transportStrategy: HardcodedTransportStrategy | DynamicTransportStrategy;
  
  constructor(context: JTAGContext, config: JTAGRouterConfig = {}) {
    super(context, config);
    
    // EVOLUTION: Steering toward dynamic, P2P-ready transport strategy
    const useDynamicTransport = config.transport?.enableP2P || 
                               config.transport?.strategy === 'dynamic' ||
                               (typeof process !== 'undefined' && process.env?.JTAG_ENABLE_P2P === 'true');
    
    if (useDynamicTransport) {
      console.log(`🚀 ${this.toString()}: Using dynamic transport strategy (P2P ready)`);
      this.transportStrategy = new DynamicTransportStrategy(this.transports, config.transport?.enableP2P ?? true);
    } else {
      console.log(`📡 ${this.toString()}: Using hardcoded transport strategy (legacy)`);
      this.transportStrategy = new HardcodedTransportStrategy(this.transports);
    }
  }

  /**
   * Get server-specific transport factory
   */
  protected async getTransportFactory(): Promise<ITransportFactory> {
    return new TransportFactoryServer();
  }
}