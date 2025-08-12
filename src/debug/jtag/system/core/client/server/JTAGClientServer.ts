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
import { SYSTEM_SCOPES } from '../../types/SystemScopes';
import type { ITransportFactory} from '../../../transports/shared/ITransportFactory';
import { TransportFactoryServer } from '../../../transports/server/TransportFactoryServer';
import type { ListResult } from '../../../../commands/list/shared/ListTypes';
import type { JTAGSystem } from '../../system/shared/JTAGSystem';
import { JTAGSystemServer } from '../../system/server/JTAGSystemServer';
import type { JTAGPayload, JTAGContext } from '../../types/JTAGTypes';
import type { UUID } from '../../types/CrossPlatformUUID';

export class JTAGClientServer extends JTAGClient {
  private _sessionId: UUID;
  
  constructor(context: JTAGContext) {
    super(context);
    // Initialize with system session bootstrap, will be updated by session daemon
    this._sessionId = SYSTEM_SCOPES.SYSTEM;
  }

  /**
   * Get current session ID (updated dynamically by session daemon)
   */
  public get sessionId(): UUID {
    return this._sessionId;
  }

  /**
   * Update session ID and notify any systems using this client as single source of truth
   */
  public setSessionId(sessionId: UUID): void {
    this._sessionId = sessionId;
    console.log(`🏷️ JTAGClientServer: Updated session to: ${sessionId}`);
    
    // TODO: Update any local system daemons to use client session (like browser does)
    // if (this.connection instanceof LocalConnection) {
    //   this.updateSystemConsoleDaemon();
    // }
  }

  /**
   * Override to update server client session (called by base JTAGClient)
   */
  protected updateClientSessionStorage(sessionId: UUID): void {
    this.setSessionId(sessionId);
  }
  
  protected async getLocalSystem(): Promise<JTAGSystem | null> {
    // FIXED: Never auto-create systems - only connect to existing ones
    // This prevents server clients from automatically creating new JTAG systems
    // when they should connect to existing systems (like test-bench on port 9002)
    
    // Only return existing instance if it's already running in same process
    if (JTAGSystemServer.instance) {
      console.log('🏠 JTAGClientServer: Found existing local system instance');
      return JTAGSystemServer.instance;
    }
    
    // Force remote connection for all other cases
    console.log('🌐 JTAGClientServer: No local system - using remote connection');
    return null;
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
        const response = await this.responseCorrelator.createRequest(correlationId, timeoutMs);
        
        // Extract commandResult from the nested response structure
        // Response structure: { payload: { commandResult: { commands: [...], success: true, ... } } }
        // Client expects: { commands: [...], success: true, ... }
        if (response && typeof response === 'object' && 'commandResult' in response) {
          return response.commandResult as TResult;
        }
        
        // Fallback: return response as-is for non-command responses
        return response as TResult;
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
      sessionId: SYSTEM_SCOPES.SYSTEM, // Default to system session
      ...options
    });
  }
}