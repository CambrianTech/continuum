/**
 * User Daemon - Cross-Context User Lifecycle Management
 *
 * Handles user lifecycle management across all user types (Human/Agent/Persona).
 *
 * CRITICAL INSIGHT: Each user is a CLIENT!
 * - HumanUser = browser client (WebSocket connection, managed by SessionDaemon)
 * - AgentUser = external API client (Claude, GPT APIs connecting TO us, managed by SessionDaemon)
 * - PersonaUser = internal server-side client (WE manage them, they're our AI citizens)
 *
 * UserDaemon ensures:
 * 1. All User entities have UserState (especially Personas created via data/create)
 * 2. All PersonaUsers have active client instances
 * 3. PersonaUsers are subscribed to events they need (data:ChatMessage:created, etc.)
 * 4. State consistency through continuous monitoring loops
 */

import { DaemonBase } from '../../command-daemon/shared/DaemonBase';
import type { JTAGMessage, JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import { createBaseResponse, type BaseResponsePayload } from '../../../system/core/types/ResponseTypes';
import { JTAG_ENDPOINTS } from '../../../system/core/router/shared/JTAGEndpoints';
import type { BaseUser } from '../../../system/user/shared/BaseUser';

/**
 * User Daemon message payload
 */
export interface UserDaemonPayload {
  type: 'ensure-user-state' | 'list-clients' | 'stats';
  userId?: UUID;
}

/**
 * User Daemon response
 */
export interface UserDaemonResponse extends BaseResponsePayload {
  userCount?: number;
  clientCount?: number;
  stats?: {
    totalUsers: number;
    humanUsers: number;
    agentUsers: number;
    personaUsers: number;
    activeClients: number;
  };
}

/**
 * User Daemon - Handles user lifecycle management
 */
export abstract class UserDaemon extends DaemonBase {
  public readonly subpath: string = 'daemons/user';

  /**
   * Registry of active PersonaUser client instances
   * Key: userId, Value: PersonaUser client instance
   *
   * Note: HumanUser and AgentUser clients are managed by SessionDaemon
   * (they connect TO us, we don't create them)
   */
  protected personaClients: Map<UUID, BaseUser> = new Map();

  /**
   * Environment-specific persona client management
   */
  protected abstract ensurePersonaClients(): Promise<void>;
  protected abstract ensureUserHasState(userId: UUID): Promise<boolean>;
  protected abstract startMonitoringLoops(): void;
  protected abstract stopMonitoringLoops(): void;

  /**
   * Get PersonaUser instance by ID (for genome commands)
   * Returns null if not found or not a PersonaUser
   */
  public getPersonaUser(userId: UUID): BaseUser | null {
    return this.personaClients.get(userId) || null;
  }

  constructor(
    context: JTAGContext,
    router: JTAGRouter
  ) {
    super('UserDaemon', context, router);
  }

  /**
   * Initialize user daemon
   */
  protected async initialize(): Promise<void> {
    console.log(`👥 UserDaemon: Initializing user lifecycle management`);

    // Subscribe to user lifecycle events
    await this.subscribeToUserEvents();

    // Ensure all existing PersonaUsers have client instances
    // (non-critical during init, will be loaded on-demand)
    try {
      await this.ensurePersonaClients();
    } catch (error) {
      console.log(`ℹ️  UserDaemon: Deferring persona client initialization (DataDaemon not ready yet)`);
    }

    // Start continuous monitoring loops
    this.startMonitoringLoops();

    console.log(`✅ UserDaemon: Initialized`);
  }

  /**
   * Subscribe to user lifecycle events
   */
  private async subscribeToUserEvents(): Promise<void> {
    // Implementation will be in server-side subclass
    // Browser doesn't need to handle user lifecycle events
  }

  /**
   * Process user daemon messages
   */
  protected async processMessage(message: JTAGMessage): Promise<UserDaemonResponse> {
    const payload = message.payload as unknown as UserDaemonPayload;

    switch (payload.type) {
      case 'ensure-user-state':
        return await this.handleEnsureUserState(message);

      case 'list-clients':
        return await this.handleListClients(message);

      case 'stats':
        return await this.handleGetStats(message);

      default:
        console.error(`❌ UserDaemon: Unknown message type: ${payload.type}`);
        return createBaseResponse(false, message.context, message.payload.sessionId, {}) as UserDaemonResponse;
    }
  }

  /**
   * Ensure user has correct state
   */
  private async handleEnsureUserState(message: JTAGMessage): Promise<UserDaemonResponse> {
    const payload = message.payload as unknown as UserDaemonPayload;

    if (!payload.userId) {
      return createBaseResponse(false, message.context, message.payload.sessionId, {
        error: 'userId required'
      }) as UserDaemonResponse;
    }

    // Implementation will be in subclasses
    return createBaseResponse(true, message.context, message.payload.sessionId, {}) as UserDaemonResponse;
  }

  /**
   * List active persona clients
   */
  private async handleListClients(message: JTAGMessage): Promise<UserDaemonResponse> {
    return createBaseResponse(true, message.context, message.payload.sessionId, {
      clientCount: this.personaClients.size
    }) as UserDaemonResponse;
  }

  /**
   * Get user daemon statistics
   */
  private async handleGetStats(message: JTAGMessage): Promise<UserDaemonResponse> {
    // Implementation will be in subclasses
    return createBaseResponse(true, message.context, message.payload.sessionId, {
      stats: {
        totalUsers: 0,
        humanUsers: 0,
        agentUsers: 0,
        personaUsers: 0,
        activeConnections: 0
      }
    }) as UserDaemonResponse;
  }

  /**
   * Cleanup on shutdown
   */
  async shutdown(): Promise<void> {
    console.log(`👋 UserDaemon: Shutting down`);
    this.stopMonitoringLoops();
  }
}