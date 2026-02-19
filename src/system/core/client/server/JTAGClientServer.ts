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
import { JTAGMessageTypes } from '../../types/JTAGTypes';
import type { JTAGPayload, JTAGContext, JTAGMessage } from '../../types/JTAGTypes';
import type { JTAGResponsePayload } from '../../types/ResponseTypes';
import type { UUID } from '../../types/CrossPlatformUUID';
import { Events } from '../../shared/Events';
import type { ClientType, ConnectionIdentity } from '../../../../daemons/session-daemon/shared/SessionTypes';
import { DEFAULT_USER_UNIQUE_IDS } from '../../../data/domains/DefaultEntities';

export class JTAGClientServer extends JTAGClient {
  // Explicit client type and identity for persona connections
  // Set by UserDaemon when creating client for PersonaUser
  private explicitClientType?: ClientType;
  private explicitUserId?: UUID;

  constructor(context: JTAGContext) {
    super(context);
    // Session ID managed by base class - SessionDaemon will assign proper session
  }

  /**
   * Set explicit client type for persona connections
   * Called by UserDaemon when creating client for PersonaUser
   */
  public setExplicitClientType(clientType: ClientType, userId?: UUID): void {
    this.explicitClientType = clientType;
    this.explicitUserId = userId;
  }

  /**
   * Get current session ID (inherited from base class - updated by SessionDaemon)
   * Base class returns: this._session?.sessionId ?? SYSTEM_SCOPES.UNKNOWN_SESSION
   */
  // Removed override - use base class sessionId getter that gets updated by SessionDaemon

  /**
   * Session ID is now managed by base class via SessionDaemon
   * This method is no longer needed - base class handles session updates
   */

  /**
   * Server clients don't need session storage - base class handles session management
   * Base class already updates this._session which is used by sessionId getter
   */
  protected updateClientSessionStorage(_sessionId: UUID): void {
    // No-op for server clients - session already updated by base class
  }

  protected async getLocalSystem(): Promise<JTAGSystem | null> {
    // Never auto-create systems - only connect to existing ones.
    // This prevents server clients from creating new JTAG systems
    // when they should connect to existing ones.

    // Only return existing instance if it's already running in same process
    if (JTAGSystemServer.instance) {
      return JTAGSystemServer.instance;
    }

    // Force remote connection for all other cases
    return null;
  }
  
  /**
   * Get server-specific transport factory
   */
  protected async getTransportFactory(): Promise<ITransportFactory> {
    return new TransportFactoryServer(this.context);
  }

  // ========================================================================
  // CLIENT TYPE & IDENTITY (Unified Client Identity Architecture)
  // ========================================================================

  /**
   * Get client type for identity resolution
   *
   * Server clients can be:
   * - persona: Set explicitly by UserDaemon for PersonaUser instances
   * - agent: Detected via agentInfo (Claude Code, GPT, etc.)
   * - cli: Default for human CLI users
   */
  protected getClientType(): ClientType {
    // Explicit type set by UserDaemon for personas
    if (this.explicitClientType) {
      return this.explicitClientType;
    }

    // Agent detection for autonomous AI agents
    const agentInfo = this.connectionContext?.agentInfo;
    if (agentInfo?.detected && agentInfo.confidence > 0.5) {
      return 'agent';
    }

    // Default: CLI user (human or AI-assisted)
    return 'cli';
  }

  /**
   * Get identity for server client types
   *
   * - persona: Returns explicit userId set by UserDaemon
   * - agent: Returns uniqueId derived from agent name
   * - cli: Returns CLI uniqueId constant
   */
  protected async getIdentityForClientType(clientType: ClientType): Promise<ConnectionIdentity> {
    switch (clientType) {
      case 'persona':
        // Persona identity is set explicitly by UserDaemon
        return {
          userId: this.explicitUserId
        };

      case 'agent': {
        // Agent identity from agent name
        const agentName = this.connectionContext?.agentInfo?.name || 'unknown-agent';
        const uniqueId = agentName.toLowerCase().replace(/\s+/g, '-');
        return {
          uniqueId
        };
      }

      case 'cli':
      default:
        // CLI uses constant uniqueId
        return {
          uniqueId: DEFAULT_USER_UNIQUE_IDS.CLI_CLIENT
        };
    }
  }

  /**
   * Get server-specific command correlator
   */
  protected getCommandCorrelator(): ICommandCorrelator {
    return {
      waitForResponse: async (correlationId: string, timeoutMs?: number): Promise<JTAGPayload> => {
        const response = await this.responseCorrelator.createRequest(correlationId, timeoutMs);

        // Extract commandResult from the nested response structure
        // Response structure: { payload: { commandResult: { commands: [...], success: true, ... } } }
        // Client expects: { commands: [...], success: true, ... }
        if (response && typeof response === 'object' && 'commandResult' in response) {
          return response.commandResult as JTAGPayload;
        }

        // Fallback: return response as-is for non-command responses
        return response;
      }
    };
  }

  /**
   * Override event message handling to trigger local subscriptions
   * This makes Events.subscribe() work consistently for remote server clients
   */
  async handleTransportMessage(message: JTAGMessage): Promise<JTAGResponsePayload> {
    // Handle event messages - trigger local subscriptions
    if (JTAGMessageTypes.isEvent(message)) {
      // console.log(`📥 JTAGClientServer: Received event message, triggering local subscriptions`);

      // Extract event name and data from payload (EventBridgePayload structure)
      const payload = message.payload as any;
      const eventName = payload?.eventName;
      const eventData = payload?.data;

      if (eventName && eventData !== undefined) {
        // Trigger local subscriptions (wildcard, elegant, exact-match)
        Events.checkWildcardSubscriptions(eventName, eventData);
        // console.log(`✅ JTAGClientServer: Triggered local subscriptions for ${eventName}`);
      } else {
        console.warn(`⚠️ JTAGClientServer: Event message missing eventName or data`, payload);
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


  /**
   * Connect to remote JTAG system
   * Uses shared base class connect() logic
   */
  static async connectRemote(options?: JTAGClientConnectOptions): Promise<{ client: JTAGClientServer; listResult: ListResult }> {
    return await JTAGClientServer.connect({
      targetEnvironment: 'server',
      // NO sessionId specified - let SessionDaemon assign shared session
      ...options
    });
  }
}