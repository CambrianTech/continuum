/**
 * JTAG Router Server - Server-specific router implementation
 * 
 * Extends shared JTAGRouter with server-only transport factory.
 * Creates TransportFactoryServer for server environment.
 */

import { JTAGRouter } from '../shared/JTAGRouter';
import type { ITransportFactory } from '../../../transports';
import { TransportFactoryServer } from '../../../transports/server/TransportFactoryServer';
import type { JTAGContext } from '../../types/JTAGTypes';
import type { JTAGRouterConfig } from '../shared/JTAGRouterTypes';

export class JTAGRouterServer extends JTAGRouter {
  
  constructor(context: JTAGContext, config: JTAGRouterConfig = {}) {
    super(context, config);
    // Strategy initialization is now handled by JTAGRouter.initializeStrategies()
  }

  /**
   * Get server-specific transport factory
   */
  protected async getTransportFactory(): Promise<ITransportFactory> {
    return new TransportFactoryServer();
  }
}