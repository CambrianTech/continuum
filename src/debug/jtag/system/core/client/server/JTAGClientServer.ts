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

import { JTAGClient, type JTAGClientConnectOptions, type ICommandCorrelator } from '../shared/JTAGClient';
import type { ITransportFactory} from '../../../transports/shared/ITransportFactory';
import { TransportFactoryServer } from '../../../transports/server/TransportFactoryServer';
import type { ListResult } from '../../../../commands/list/shared/ListTypes';
import type { JTAGSystem } from '../../system/shared/JTAGSystem';
import { JTAGSystemServer } from '../../system/server/JTAGSystemServer';
import { ResponseCorrelator } from '../../shared/ResponseCorrelator';
import type { JTAGPayload } from '../../types/JTAGTypes';

export class JTAGClientServer extends JTAGClient {
  
  protected async getLocalSystem(): Promise<JTAGSystem | null> {
    // Try local system first (same process)
    if (JTAGSystemServer.instance) {
      console.log('🏠 JTAGClientServer: Found existing local system instance');
      return JTAGSystemServer.instance;
    }
    
    // Try to connect to local system (may create if needed in development)
    try {
      console.log('🔄 JTAGClientServer: Attempting to connect to local JTAGSystemServer...');
      const localSystem = await JTAGSystemServer.connect();
      console.log('✅ JTAGClientServer: Connected to local system');
      return localSystem;
    } catch (error) {
      console.log('⚠️ JTAGClientServer: Local system connect failed:', error instanceof Error ? error.message : String(error));
    }
    
    // Check if server is running on expected port (npm start scenario)
    if (await this.isServerRunning()) {
      console.log('🌐 JTAGClientServer: Server detected on port 9001, using remote connection');
      return null; // Use remote connection to existing server
    }
    
    console.log('❌ JTAGClientServer: No JTAG system found - run "npm start" first');
    throw new Error('No JTAG system available. Please run "npm start" to start the system.');
  }

  private async isServerRunning(): Promise<boolean> {
    try {
      // Quick check if WebSocket server is running on 9001
      const net = await import('net');
      const socket = new net.Socket();
      
      return new Promise((resolve) => {
        socket.setTimeout(1000);  
        socket.on('connect', () => {
          socket.destroy();
          resolve(true);
        });
        socket.on('timeout', () => {
          socket.destroy();
          resolve(false);
        });
        socket.on('error', () => {
          resolve(false);
        });
        socket.connect(9001, 'localhost');
      });
    } catch {
      return false;
    }
  }
  
  /**
   * Get server-specific transport factory
   */
  protected async getTransportFactory(): Promise<ITransportFactory> {
    return new TransportFactoryServer();
  }

  /**
   * Get server-specific command correlator
   */
  protected getCommandCorrelator(): ICommandCorrelator {
    return {
      waitForResponse: async <TResult extends JTAGPayload>(correlationId: string, timeoutMs?: number): Promise<TResult> => {
        return await this.responseCorrelator.createRequest(correlationId, timeoutMs) as TResult;
      }
    };
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