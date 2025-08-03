/**
 * JTAG Router Dynamic Browser - Browser-specific dynamic router implementation
 * 
 * Extends JTAGRouterDynamic with browser-only transport factory.
 * Provides intelligent routing, P2P capabilities, and health-based transport selection.
 */

import { JTAGRouterDynamic } from '../shared/JTAGRouterDynamic';
import type { ITransportFactory } from '../../../transports';
import { TransportFactoryBrowser } from '../../../transports/browser/TransportFactoryBrowser';
import type { JTAGContext } from '../../types/JTAGTypes';
import type { JTAGRouterConfig } from '../shared/JTAGRouterTypes';

export class JTAGRouterDynamicBrowser extends JTAGRouterDynamic {
  
  constructor(context: JTAGContext, config: JTAGRouterConfig = {}) {
    super(context, config);
    console.log(`🚀 JTAGRouterDynamicBrowser: Initialized with intelligent routing capabilities`);
  }

  /**
   * Get browser-specific transport factory
   */
  protected async getTransportFactory(): Promise<ITransportFactory> {
    console.log(`🏭 JTAGRouterDynamicBrowser: Creating browser transport factory`);
    return new TransportFactoryBrowser();
  }
}