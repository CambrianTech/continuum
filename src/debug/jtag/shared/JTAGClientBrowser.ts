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

import type { JTAGContext } from './JTAGTypes';
import { generateUUID, type UUID } from './CrossPlatformUUID';
import type { JTAGSystemBrowser } from '../browser/JTAGSystemBrowser';
import { JTAGClient, type JTAGClientConnectOptions, LocalConnection } from './JTAGClient';
import type { ListResult } from '../commands/list/shared/ListTypes';

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

  /**
   * Browser-specific: Get local JTAGSystemBrowser instance
   */
  protected getLocalSystem(): JTAGSystemBrowser {
    // This will be set during connectLocal()
    if (!this.localSystemInstance) {
      throw new Error('Local system not available. Use connectLocal() first.');
    }
    return this.localSystemInstance;
  }
  
  private localSystemInstance?: JTAGSystemBrowser;
  
  /**
   * Connect to local JTAGSystemBrowser instance (direct calls)
   * 🔄 BOOTSTRAP PATTERN: Returns list result for CLI integration
   */
  static async connectLocal(): Promise<{ client: JTAGClientBrowser; listResult: ListResult }> {
    // Dynamic import to avoid circular dependency
    const { JTAGSystemBrowser } = await import('../browser/JTAGSystemBrowser');
    
    const context: JTAGContext = {
      uuid: generateUUID(),
      environment: 'browser'
    };
    
    console.log('🔄 JTAGClientBrowser: Connecting to local browser system...');
    
    // Get direct reference to local system
    const localSystem = await JTAGSystemBrowser.connect();
    const client = new JTAGClientBrowser(context);
    client.localSystemInstance = localSystem;
    
    // Set up local connection
    client.connection = client.createLocalConnection();
    
    console.log('✅ JTAGClientBrowser: Local connection established');
    
    // 🔑 BOOTSTRAP: Call list() to discover commands and return result for CLI
    console.log('🔄 JTAGClientBrowser: Discovering available commands...');
    const listResult = await client.commands.list();
    
    console.log(`✅ JTAGClientBrowser: Bootstrap complete! Discovered ${listResult.totalCount} commands`);
    
    return { client, listResult };
  }

  /**
   * Connect to remote JTAG system via transport
   * Uses shared JTAGClient.connect() with browser-specific configuration
   */
  static async connectRemote(config: RemoteConnectionConfig): Promise<{ client: JTAGClientBrowser; listResult: ListResult }> {
    const clientOptions: JTAGClientConnectOptions = {
      targetEnvironment: 'server', // Remote system is typically server
      transportType: config.transportType ?? 'websocket',
      serverUrl: config.serverUrl,
      sessionId: config.sessionId,
      maxRetries: config.maxRetries ?? 30,
      retryDelay: config.retryDelay ?? 1000
    };

    // Use shared connect logic
    const { client, listResult } = await JTAGClient.connect(clientOptions);
    
    return { client: client as JTAGClientBrowser, listResult };
  }

  // Commands interface inherited from base JTAGClient - now uses dynamic discovery!

  /**
   * Browser-specific connection metadata
   */
  get isLocal(): boolean {
    return this.connection instanceof LocalConnection;
  }
}