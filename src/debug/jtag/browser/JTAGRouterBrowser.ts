/**
 * JTAG Router Browser - Browser-specific router implementation
 * 
 * Extends shared JTAGRouter with browser-only transport factory.
 * Creates TransportFactoryBrowser for browser environment.
 */

import { JTAGRouter } from '@shared/JTAGRouter';
import type { ITransportFactory } from '@systemTransports';
import { TransportFactoryBrowser } from '../system/transports/browser/TransportFactoryBrowser';

export class JTAGRouterBrowser extends JTAGRouter {
  /**
   * Get browser-specific transport factory
   */
  protected async getTransportFactory(): Promise<ITransportFactory> {
    return new TransportFactoryBrowser();
  }
}