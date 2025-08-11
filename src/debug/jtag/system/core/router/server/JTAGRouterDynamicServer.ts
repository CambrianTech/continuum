/**
 * JTAG Router Dynamic Server - Server-specific dynamic router implementation
 * 
 * Extends JTAGRouterDynamic with server-only transport factory.
 * Provides intelligent routing, P2P capabilities, and health-based transport selection.
 */

import { JTAGRouterDynamic } from '../shared/JTAGRouterDynamic';
import type { ITransportFactory } from '../../../transports';
import { TransportFactoryServer } from '../../../transports/server/TransportFactoryServer';
import type { JTAGContext } from '../../types/JTAGTypes';
import type { JTAGRouterConfig } from '../shared/JTAGRouterTypes';

export class JTAGRouterDynamicServer extends JTAGRouterDynamic {
  
  constructor(context: JTAGContext, config: JTAGRouterConfig) {
    super(context, config);
    console.log(`🚀 JTAGRouterDynamicServer: Initialized with intelligent routing capabilities`);
  }

  /**
   * Get server-specific transport factory
   */
  protected async getTransportFactory(): Promise<ITransportFactory> {
    console.log(`🏭 JTAGRouterDynamicServer: Creating server transport factory`);
    return new TransportFactoryServer();
  }
}