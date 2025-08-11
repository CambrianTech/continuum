// ISSUES: 2 open, last updated 2025-07-30 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking

/**
 * JTAGClientBrowser - Browser-specific JTAG client with dynamic command discovery
 * 
 * Extends shared JTAGClient base class with browser-specific local system integration.
 * Provides identical command interface whether connecting locally (direct) or remotely (transport).
 * 
 * COMPLETED FEATURES:
 * ✅ Dynamic command discovery via 'list' command - No more hardcoded commands!
 * ✅ Connection abstraction pattern (LocalConnection vs RemoteConnection) - Moved to shared base
 * ✅ Strongly-typed command interface with full TypeScript safety
 * ✅ Zero duplication - reuses shared command/correlation system
 * ✅ Identical API regardless of connection type (local vs remote)
 * 
 * ISSUES: (look for TODOs)
 * - TODO: Implement proper factory pattern for browser vs server client creation
 * - TODO: Add local system detection (auto-choose local vs remote based on availability)
 * 
 * CORE ARCHITECTURE:
 * - Inherits from shared JTAGClient base class
 * - Only implements browser-specific differences (getLocalSystem, connectLocal)
 * - Dynamic commands interface generated from server's 'list' command response
 * - Uses JTAGSystemBrowser for local connections
 * 
 * TESTING REQUIREMENTS:
 * - Unit tests: Browser-specific local system integration
 * - Integration tests: Local vs remote connection behavior parity
 * - Type tests: Dynamic command interface type safety
 * - Performance tests: Local connection overhead vs direct calls
 */

import { type UUID } from '../../types/CrossPlatformUUID';
import { JTAGSystemBrowser } from '../../system/browser/JTAGSystemBrowser';
import { JTAGClient, type JTAGClientConnectOptions, type ICommandCorrelator, LocalConnection } from '../shared/JTAGClient';
import type { ListResult } from '../../../../commands/list/shared/ListTypes';
import type { ITransportFactory} from '../../../transports/shared/ITransportFactory';
import { TransportFactoryBrowser } from '../../../transports/browser/TransportFactoryBrowser';
import type { JTAGSystem } from '../../system/shared/JTAGSystem';
import type { JTAGPayload, JTAGContext } from '../../types/JTAGTypes';

// NOTE: Command types are now dynamically discovered, no need for hardcoded imports

// Commands interface now provided by shared JTAGClient base class via dynamic discovery

// Connection classes now provided by shared JTAGClient base class

/**
 * Connection configuration for remote connections
 */
export interface RemoteConnectionConfig {
  readonly serverUrl: string;
  readonly sessionId?: UUID;
  readonly maxRetries?: number;
  readonly retryDelay?: number;
  readonly transportType?: 'websocket' | 'http';
}

/**
 * JTAGClientBrowser - Browser-specific JTAG client with dynamic command discovery
 * Extends shared JTAGClient with browser-specific local system integration
 */
export class JTAGClientBrowser extends JTAGClient {
  private static readonly SESSION_STORAGE_KEY = 'jtag_session_id';
  private _sessionId: UUID;
  
  constructor(context: JTAGContext) {
    super(context);
    // Initialize session ID from sessionStorage or fallback to system session
    this._sessionId = this.initializeSessionId();
  }

  /**
   * Initialize session ID from sessionStorage or use system session fallback
   */
  private initializeSessionId(): UUID {
    try {
      const storedSessionId = sessionStorage.getItem(JTAGClientBrowser.SESSION_STORAGE_KEY);
      if (storedSessionId) {
        console.log(`🏷️ JTAGClientBrowser: Loaded session from sessionStorage: ${storedSessionId}`);
        return storedSessionId as UUID;
      } else {
        // Fallback to system session ID
        const systemSessionId = '00000000-0000-0000-0000-000000000000' as UUID;
        console.log(`🏷️ JTAGClientBrowser: Using system session fallback: ${systemSessionId}`);
        return systemSessionId;
      }
    } catch (error) {
      console.warn(`⚠️ JTAGClientBrowser: Failed to read from sessionStorage, using system session fallback`, error);
      return '00000000-0000-0000-0000-000000000000' as UUID;
    }
  }

  /**
   * Update session ID and store in sessionStorage
   */
  public setSessionId(sessionId: UUID): void {
    try {
      this._sessionId = sessionId;
      sessionStorage.setItem(JTAGClientBrowser.SESSION_STORAGE_KEY, sessionId);
      console.log(`🏷️ JTAGClientBrowser: Updated session and stored in sessionStorage: ${sessionId}`);
    } catch (error) {
      console.warn(`⚠️ JTAGClientBrowser: Failed to save session to sessionStorage`, error);
      // Still update the in-memory value even if sessionStorage fails
      this._sessionId = sessionId;
    }
  }

  /**
   * Get current session ID
   */
  public get sessionId(): UUID {
    return this._sessionId;
  }

  /**
   * Get browser-specific command correlator
   */
  protected getCommandCorrelator(): ICommandCorrelator {
    return {
      waitForResponse: async <TResult extends JTAGPayload>(correlationId: string, timeoutMs?: number): Promise<TResult> => {
        return await this.responseCorrelator.createRequest(correlationId, timeoutMs) as TResult;
      }
    };
  }
  
  protected async getLocalSystem(): Promise<JTAGSystem | null> {
    // TODO: Implement proper local vs remote browser detection:
    // - Check if we're in same-origin context with local JTAG system
    // - Add configuration for remote browser connections
    // - Handle embedded vs standalone browser scenarios
    
    // TEMPORARY: Hard-code local system access for development
    // Browser can access local system - connect() creates instance if needed
    try {
      return await JTAGSystemBrowser.connect();
    } catch (error) {
      console.warn(`⚠️ JTAGClientBrowser: Local system not available:`, error);
      return null;
    }
  }
  
  /**
   * Connect to local JTAGSystemBrowser instance (direct calls)
   * Uses shared base class connect() logic
   */
  static async connectLocal(): Promise<{ client: JTAGClientBrowser; listResult: ListResult }> {
    return await JTAGClientBrowser.connect({
      targetEnvironment: 'browser'
    });
  }

  /**
   * Connect to remote JTAG system via transport
   * Uses shared base class connect() logic
   */
  static async connectRemote(config: RemoteConnectionConfig): Promise<{ client: JTAGClientBrowser; listResult: ListResult }> {
    return await JTAGClientBrowser.connect({
      targetEnvironment: 'server', // Remote system is typically server
      transportType: config.transportType ?? 'websocket',
      serverUrl: config.serverUrl,
      sessionId: config.sessionId,
      maxRetries: config.maxRetries ?? 30,
      retryDelay: config.retryDelay ?? 1000
    });
  }

  // Commands interface inherited from base JTAGClient - now uses dynamic discovery!

  /**
   * Get browser-specific transport factory
   */
  protected async getTransportFactory(): Promise<ITransportFactory> {
    return new TransportFactoryBrowser();
  }

  /**
   * Update browser sessionStorage with new session ID and notify local system
   */
  protected updateClientSessionStorage(sessionId: UUID): void {
    this.setSessionId(sessionId);
    
    // If using local connection, update the local system's ConsoleDaemon to use client session
    if (this.connection instanceof LocalConnection) {
      this.updateSystemConsoleDaemon();
    }
  }

  /**
   * Update the local system's ConsoleDaemon to use client session ID
   * This ensures the ConsoleDaemon uses the client as the single source of truth for session IDs
   */
  private updateSystemConsoleDaemon(): void {
    const system = (this.connection as LocalConnection).localSystem;
    if (system && 'daemons' in system) {
      // Find ConsoleDaemon and set provider to use client session
      const consoleDaemon = (system as any).daemons?.find((d: any) => d.constructor.name === 'ConsoleDaemonBrowser');
      if (consoleDaemon && 'setSessionIdProvider' in consoleDaemon) {
        consoleDaemon.setSessionIdProvider(() => this.sessionId);
        console.log(`🏷️ JTAGClientBrowser: Updated ConsoleDaemon to use client session: ${this.sessionId}`);
      }
    }
  }


  /**
   * Browser-specific connection metadata
   */
  get isLocal(): boolean {
    return this.connection instanceof LocalConnection;
  }
}