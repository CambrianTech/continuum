/**
 * JTAG Client Server - Server-specific client implementation
 * 
 * Extends shared JTAGClient with server-only transport factory.
 * Uses TransportFactoryServer for server environment.
 */

import { JTAGClient, type JTAGClientConnectOptions } from '@shared/JTAGClient';
import type { ITransportFactory} from '@systemTransports';
import { TransportFactoryServer } from '../system/transports/server/TransportFactoryServer';
import type { ListResult } from '../commands/list/shared/ListTypes';
import type { JTAGSystem } from '../shared/JTAGSystem';

export class JTAGClientServer extends JTAGClient {
  
  protected getLocalSystem(): JTAGSystem | null {
    // Server clients cannot access local system
    return null;
  }
  
  /**
   * Get server-specific transport factory
   */
  protected async getTransportFactory(): Promise<ITransportFactory> {
    return new TransportFactoryServer();
  }


  /**
   * Connect to remote JTAG system
   * Uses shared base class connect() logic
   */
  static async connectRemote(options?: JTAGClientConnectOptions): Promise<{ client: JTAGClientServer; listResult: ListResult }> {
    return await JTAGClientServer.connect({
      targetEnvironment: 'server',
      ...options
    });
  }
}