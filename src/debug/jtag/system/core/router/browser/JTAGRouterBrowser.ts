/**
 * JTAG Router Browser - Browser-specific router implementation
 * 
 * Extends shared JTAGRouter with browser-only transport factory.
 * Creates TransportFactoryBrowser for browser environment.
 */

import { JTAGRouter } from '../shared/JTAGRouter';
import type { ITransportFactory } from '../../../transports';
import { TransportFactoryBrowser } from '../../../transports/browser/TransportFactoryBrowser';
import type { JTAGContext } from '../../types/JTAGTypes';
import type { JTAGRouterConfig } from '../shared/JTAGRouterTypes';

export class JTAGRouterBrowser extends JTAGRouter {
  
  constructor(context: JTAGContext, config: JTAGRouterConfig = {}) {
    super(context, config);
    // Strategy initialization is now handled by JTAGRouterBase.initializeStrategies()
  }

  /**
   * Get browser-specific transport factory
   */
  protected async getTransportFactory(): Promise<ITransportFactory> {
    return new TransportFactoryBrowser();
  }
}