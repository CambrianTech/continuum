// ISSUES: 2 open, last updated 2025-07-31 - Architecture needs local system support
/**
 * JTAG Client Server - Server-specific client implementation
 * 
 * Extends shared JTAGClient with server-only transport factory.
 * Uses TransportFactoryServer for server environment.
 * 
 * ISSUES:
 * 🚨 ISSUE 1: NO LOCAL SYSTEM ACCESS
 *    - getLocalSystem() always returns null, forcing transport connections
 *    - During npm start, should access local JTAGSystemServer instance
 *    - Need local vs remote detection logic
 * 
 * 🚨 ISSUE 2: TRANSPORT FACTORY ROLE MISMATCH  
 *    - Uses TransportFactoryServer but needs 'client' role transports
 *    - TransportFactoryServer only supports 'server' role (listening)
 *    - Need client transport support for remote connections
 * 
 * TODO: Add local JTAGSystemServer access for development mode
 * TODO: Fix transport factory to support client connections
 */

import { JTAGClient, type JTAGClientConnectOptions } from '@shared/JTAGClient';
import type { ITransportFactory} from '@systemTransports';
import { TransportFactoryServer } from '../system/transports/server/TransportFactoryServer';
import type { ListResult } from '../commands/list/shared/ListTypes';
import type { JTAGSystem } from '../shared/JTAGSystem';
import { JTAGSystemServer } from './JTAGSystemServer';

export class JTAGClientServer extends JTAGClient {
  
  protected async getLocalSystem(): Promise<JTAGSystem | null> {
    // TODO: Implement proper local vs remote detection:
    // - Check if we're in development mode (npm start)
    // - Check for local JTAG system availability  
    // - Add configuration override (--local, --remote flags)
    // - Auto-detect based on environment variables
    
    // TEMPORARY: Hard-code local system access for development
    try {
      // Try to connect to or create local JTAGSystemServer instance
      return await JTAGSystemServer.connect();
    } catch (error) {
      console.warn(`⚠️ JTAGClientServer: Local system not available, will use transport:`, error);
      return null; // Fall back to transport connection
    }
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