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

import { type UUID, generateUUID } from '../../types/CrossPlatformUUID';
import { SYSTEM_SCOPES } from '../../types/SystemScopes';
import { JTAGSystemBrowser } from '../../system/browser/JTAGSystemBrowser';
import { JTAGClient, type JTAGClientConnectOptions, type ICommandCorrelator, LocalConnection } from '../shared/JTAGClient';
import type { ListResult } from '../../../../commands/list/shared/ListTypes';
import type { ITransportFactory} from '../../../transports/shared/ITransportFactory';
import { TransportFactoryBrowser } from '../../../transports/browser/TransportFactoryBrowser';
import type { JTAGSystem } from '../../system/shared/JTAGSystem';
import { JTAGMessageTypes } from '../../types/JTAGTypes';
import type { JTAGPayload, JTAGContext, JTAGMessage } from '../../types/JTAGTypes';
import type { JTAGResponsePayload } from '../../types/ResponseTypes';
import { ConsoleDaemon } from '../../../../daemons/console-daemon/shared/ConsoleDaemon';
import { Events } from '../../shared/Events';
import { startConnectionMonitoring } from './ConnectionMonitor';
import { initializeFaviconManager } from './FaviconManager';

// NOTE: Command types are now dynamically discovered, no need for hardcoded imports

// Commands interface now provided by shared JTAGClient base class via dynamic discovery

// Connection classes now provided by shared JTAGClient base class

/**
 * Connection configuration for remote connections
 */
export interface RemoteConnectionConfig {
  readonly serverUrl: string;
  readonly sessionId: UUID;
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
        // No session stored - will be assigned by server on connection
        console.log(`🏷️ JTAGClientBrowser: No session in storage, will be assigned by server`);
        return SYSTEM_SCOPES.UNKNOWN_SESSION; // Bootstrap session until server assigns real session
      }
    } catch (error) {
      console.warn(`⚠️ JTAGClientBrowser: Failed to read from sessionStorage, using bootstrap session`, error);
      return SYSTEM_SCOPES.UNKNOWN_SESSION;
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
      waitForResponse: async (correlationId: string, timeoutMs?: number): Promise<JTAGPayload> => {
        return await this.responseCorrelator.createRequest(correlationId, timeoutMs);
      }
    };
  }

  /**
   * Override event message handling to trigger local subscriptions
   * This makes Events.subscribe() work consistently for remote browser clients
   */
  async handleTransportMessage(message: JTAGMessage): Promise<JTAGResponsePayload> {
    // Handle event messages - trigger local subscriptions
    if (JTAGMessageTypes.isEvent(message)) {
      console.log(`📥 JTAGClientBrowser: Received event message, triggering local subscriptions`);

      // Extract event name and data from payload (EventBridgePayload structure)
      const payload = message.payload as any;
      const eventName = payload?.eventName;
      const eventData = payload?.data;

      if (eventName && eventData !== undefined) {
        // Trigger local subscriptions (wildcard, elegant, exact-match)
        Events.checkWildcardSubscriptions(eventName, eventData);
        console.log(`✅ JTAGClientBrowser: Triggered local subscriptions for ${eventName}`);
      } else {
        console.warn(`⚠️ JTAGClientBrowser: Event message missing eventName or data`, payload);
      }

      // Still return acknowledgment (clients don't route to other clients)
      return {
        success: true,
        delegated: true,
        timestamp: new Date().toISOString(),
        context: this.context,
        sessionId: this.sessionId
      } as JTAGResponsePayload;
    }

    // For all other messages, delegate to base class
    return super.handleTransportMessage(message);
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
      // Let server assign session ID instead of hardcoding SYSTEM session
    });
  }

  /**
   * Connect to remote JTAG system via transport
   * Uses shared base class connect() logic
   */
  static async connectRemote(config: RemoteConnectionConfig): Promise<{ client: JTAGClientBrowser; listResult: ListResult }> {
    const result = await JTAGClientBrowser.connect({
      targetEnvironment: 'server', // Remote system is typically server
      transportType: config.transportType ?? 'websocket',
      serverUrl: config.serverUrl,
      sessionId: config.sessionId,
      maxRetries: config.maxRetries ?? 30,
      retryDelay: config.retryDelay ?? 1000
    });

    // Initialize independent monitoring systems after successful connection
    startConnectionMonitoring();
    initializeFaviconManager();

    return result;
  }

  // Commands interface inherited from base JTAGClient - now uses dynamic discovery!

  /**
   * Get browser-specific transport factory
   */
  protected async getTransportFactory(): Promise<ITransportFactory> {
    return new TransportFactoryBrowser(this.context);
  }

  /**
   * Get stored userId from localStorage for citizen persistence
   */
  protected async getStoredUserId(): Promise<UUID | undefined> {
    try {
      const { BrowserDeviceIdentity } = await import('../../browser/BrowserDeviceIdentity');
      const identity = await BrowserDeviceIdentity.getOrCreateIdentity();
      console.log(`🔍 JTAGClientBrowser: Retrieved stored userId: ${identity.userId.slice(0, 8)}...`);
      return identity.userId;
    } catch (error) {
      console.error(`❌ JTAGClientBrowser: Failed to get stored userId:`, error);
      return undefined;
    }
  }

  /**
   * Store userId to localStorage for citizen persistence across sessions
   */
  protected async storeUserIdentity(userId: UUID): Promise<void> {
    try {
      const { BrowserDeviceIdentity } = await import('../../browser/BrowserDeviceIdentity');
      await BrowserDeviceIdentity.upgradeToAuthenticated(userId);
      console.log(`✅ JTAGClientBrowser: Stored citizen identity (userId: ${userId.slice(0, 8)}...) to localStorage`);
    } catch (error) {
      console.error(`❌ JTAGClientBrowser: Failed to store userId:`, error);
    }
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

    // Initialize UserState for browser clients (persistence for theme, preferences, etc.)
    this.initializeUserState().catch(error => {
      console.error('❌ JTAGClientBrowser: Failed to initialize UserState:', error);
    });
  }

  private userStateId: UUID | null = null;

  /**
   * Get UserState ID for this browser client
   * Used by widgets to update preferences without creating entities
   */
  public getUserStateId(): UUID | null {
    return this.userStateId;
  }

  /**
   * Initialize or load UserState for browser persistence
   * Called during connection after session is established
   */
  private async initializeUserState(): Promise<void> {
    try {
      console.log('🔧 JTAGClientBrowser: Initializing UserState...');

      // Get persistent device identity (encrypted in localStorage)
      const { BrowserDeviceIdentity } = await import('../../browser/BrowserDeviceIdentity');
      const identity = await BrowserDeviceIdentity.getOrCreateIdentity();

      console.log(`🔧 JTAGClientBrowser: Using device ${identity.deviceId.substring(0, 12)}... user ${identity.userId.substring(0, 8)}...`);

      // Try to load existing UserState from localStorage
      const { LocalStorageDataBackend } = await import('../../../../daemons/data-daemon/browser/LocalStorageDataBackend');

      // Get all UserState entities for this device
      const allKeys = Object.keys(localStorage).filter(key =>
        key.startsWith('continuum-entity-UserState:')
      );

      console.log(`🔧 JTAGClientBrowser: Found ${allKeys.length} UserState entities in localStorage`);

      // Try to find matching UserState for this deviceId
      let foundUserState = null;
      for (const key of allKeys) {
        try {
          const data = localStorage.getItem(key);
          if (data) {
            const parsed = JSON.parse(data);
            if (parsed.entity?.deviceId === identity.deviceId) {
              foundUserState = parsed.entity;
              console.log(`✅ JTAGClientBrowser: Found existing UserState ${foundUserState.id.substring(0, 8)}...`);
              break;
            }
          }
        } catch (e) {
          // Skip malformed entries
        }
      }

      if (foundUserState) {
        // Use existing UserState
        this.userStateId = foundUserState.id;
        const themeInfo = foundUserState.preferences?.theme || 'unknown';
        console.log(`✅ JTAGClientBrowser: Loaded UserState ${foundUserState.id.substring(0, 8)}... (theme: ${themeInfo})`);
      } else {
        // Create new UserState with defaults
        console.log('🔧 JTAGClientBrowser: No existing UserState found, creating new one');

        const { generateUUID } = await import('../../types/CrossPlatformUUID');
        this.userStateId = generateUUID();

        // Import UserStateEntity to create proper entity instance
        const { UserStateEntity } = await import('../../../data/entities/UserStateEntity');
        const newUserState = new UserStateEntity();
        newUserState.id = this.userStateId;
        newUserState.userId = identity.userId;
        newUserState.deviceId = identity.deviceId;
        newUserState.preferences = {
          maxOpenTabs: 10,
          autoCloseAfterDays: 30,
          rememberScrollPosition: true,
          syncAcrossDevices: false
        };
        newUserState.contentState = {
          openItems: [],
          lastUpdatedAt: new Date()
        };
        newUserState.createdAt = new Date();
        newUserState.updatedAt = new Date();

        const result = await LocalStorageDataBackend.create('UserState', newUserState);

        if (result.success) {
          console.log(`✅ JTAGClientBrowser: Created new UserState ${this.userStateId.substring(0, 8)}...`);
        } else {
          console.error(`❌ JTAGClientBrowser: Failed to create UserState:`, result.error);
          this.userStateId = null;
        }
      }
    } catch (error) {
      console.error('❌ JTAGClientBrowser: UserState initialization failed:', error);
      this.userStateId = null;
    }
  }

  /**
   * Update the local system's ConsoleDaemon to use client session ID
   * This ensures the ConsoleDaemon uses the client as the single source of truth for session IDs
   */
  private updateSystemConsoleDaemon(): void {
    const system = (this.connection as LocalConnection).localSystem;

    // Use proper public interface - JTAGSystem.systemDaemons getter
    if ('systemDaemons' in system) {
      // Find ConsoleDaemon using proper type checking
      const consoleDaemon = system.systemDaemons.find(daemon => daemon instanceof ConsoleDaemon);
      if (consoleDaemon) {
        // Type assertion is safe here since we verified instanceof ConsoleDaemon
        (consoleDaemon as ConsoleDaemon).setSessionIdProvider(() => this.sessionId);
        console.log(`🏷️ JTAGClientBrowser: Updated ConsoleDaemon to use client session: ${this.sessionId}`);
      } else {
        console.warn(`⚠️ JTAGClientBrowser: No ConsoleDaemon found in local system`);
      }
    } else {
      console.warn(`⚠️ JTAGClientBrowser: Local system has no systemDaemons property`);
    }
  }
}

/**
 * Cached WebSocket transport reference for ConnectionMonitor
 * Updated by JTAGClientBrowser when connection is established
 */
let cachedTransport: WebSocket | null = null;

/**
 * Get WebSocket transport for ConnectionMonitor
 * Exported for use by ConnectionMonitor to check connection health
 */
export function getWebSocketTransport(): WebSocket | null {
  return cachedTransport;
}

/**
 * Update cached WebSocket transport reference
 * Called internally by JTAGClientBrowser after connection established
 */
export function updateWebSocketTransport(transport: WebSocket | null): void {
  cachedTransport = transport;
}