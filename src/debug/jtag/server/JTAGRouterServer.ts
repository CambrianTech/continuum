/**
 * JTAG Router Server - Server-specific router implementation
 * 
 * Extends shared JTAGRouter with server-only transport factory.
 * Creates TransportFactoryServer for server environment.
 */

import { JTAGRouter } from '@shared/JTAGRouter';
import type { ITransportFactory } from '@systemTransports';
import { TransportFactoryServer } from '../system/transports/server/TransportFactoryServer';

export class JTAGRouterServer extends JTAGRouter {
  /**
   * Get server-specific transport factory
   */
  protected async getTransportFactory(): Promise<ITransportFactory> {
    return new TransportFactoryServer();
  }
}