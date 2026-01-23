/**
 * Session Daemon - Server Implementation
 * 
 * Server-specific session daemon that handles session identity management.
 * Follows the sparse override pattern - minimal server-specific logic.
 */

import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import { SessionDaemon} from '../shared/SessionDaemon';
import type { BaseUser } from '../../../system/user/shared/BaseUser';
import type { UserCreateParams } from '../../../commands/user/create/shared/UserCreateTypes';
import { UserFactory } from '../../../system/user/shared/UserFactory';
import { HumanUser } from '../../../system/user/shared/HumanUser';
import { AgentUser } from '../../../system/user/shared/AgentUser';
import { PersonaUser } from '../../../system/user/server/PersonaUser';
import { MemoryStateBackend } from '../../../system/user/storage/MemoryStateBackend';
import { SQLiteStateBackend } from '../../../system/user/storage/server/SQLiteStateBackend';
import { DataDaemon } from '../../data-daemon/shared/DataDaemon';
import { COLLECTIONS } from '../../../system/data/config/DatabaseConfig';
import { UserEntity } from '../../../system/data/entities/UserEntity';
import { UserStateEntity } from '../../../system/data/entities/UserStateEntity';
import { UserIdentityResolver } from '../../../system/user/shared/UserIdentityResolver';
import { Logger } from '../../../system/core/logging/Logger';
import {
  type SessionMetadata,
  type CreateSessionParams,
  type CreateSessionResult,
  type SessionResponse,
  type SessionErrorResponse,
  type SessionOperation,
  type GetSessionParams,
  type GetSessionResult,
  type ListSessionsParams,
  type ListSessionsResult,
  type DestroySessionParams,
  type DestroySessionResult,
  type EnhancedConnectionContext,
  type ClientType,
} from '../shared/SessionTypes';
import { generateUUID } from '../../../system/core/types/CrossPlatformUUID';
import { createPayload } from '../../../system/core/types/JTAGTypes';
import { type JTAGMessage } from '../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import { isBootstrapSession } from '../../../system/core/types/SystemScopes';
import { WorkingDirConfig } from '../../../system/core/config/WorkingDirConfig';
import { SessionStateHelper } from './SessionStateHelper';
import fs from 'fs/promises';
import path from 'path';

const createSessionErrorResponse = (
  error: string,
  context: JTAGContext,
  sessionId: UUID,
  operation?: SessionOperation
): SessionErrorResponse => {
  return createPayload(context, sessionId, {
    operation,
    success: false,
    timestamp: new Date().toISOString(),
    error
  });
};

export class SessionDaemonServer extends SessionDaemon {
  private sessions: SessionMetadata[] = []; // In-memory active sessions for server
  private readonly SESSION_EXPIRY_MS = 30 * 60 * 1000; // 30 minutes for ephemeral sessions
  private readonly BROWSER_SESSION_EXPIRY_MS = 24 * 60 * 60 * 1000; // 24 hours for browser sessions

  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);

    // Initialize standardized logging (daemons/ subdirectory)
    const className = this.constructor.name;
    this.log = Logger.create(className, `daemons/${className}`);
  }

  /**
   * Get the sessions metadata file path for the current working directory context
   */
  private getSessionsMetadataPath(): string {
    const continuumPath = WorkingDirConfig.getContinuumPath();
    return path.join(continuumPath, 'jtag', 'sessions', 'metadata.json');
  }

  /**
   * Ensure session directories exist for the current working directory context
   */
  private async ensureSessionDirectories(): Promise<void> {
    const continuumPath = WorkingDirConfig.getContinuumPath();
    const sessionDir = path.join(continuumPath, 'jtag', 'sessions');
    const logsDir = path.join(continuumPath, 'jtag', 'logs');
    const screenshotsDir = path.join(continuumPath, 'jtag', 'screenshots');
    const signalsDir = path.join(continuumPath, 'jtag', 'signals');
    
    await Promise.all([
      fs.mkdir(sessionDir, { recursive: true }),
      fs.mkdir(logsDir, { recursive: true }),
      fs.mkdir(screenshotsDir, { recursive: true }),
      fs.mkdir(signalsDir, { recursive: true })
    ]);
  }

  /**
   * Load sessions from per-project metadata file
   * IMPORTANT: User objects must be reconstructed from database, not from stale JSON
   */
  private async loadSessionsFromFile(): Promise<void> {
    try {
      const metadataPath = this.getSessionsMetadataPath();
      const metadataContent = await fs.readFile(metadataPath, 'utf-8');
      const metadata = JSON.parse(metadataContent);

      if (metadata.sessions && Array.isArray(metadata.sessions)) {
        // Filter out bootstrap sessions that should never be shared
        const validSessions = metadata.sessions.filter((session: SessionMetadata) =>
          !isBootstrapSession(session.sessionId)
        );

        // Reconstruct sessions with proper user objects from database
        this.sessions = [];
        for (const session of validSessions) {
          try {
            // Reconstruct user from database (not stale JSON) to get proper BaseUser instance
            const user = await this.getUserById(session.userId);

            this.sessions.push({
              ...session,
              created: new Date(session.created),
              lastActive: new Date(session.lastActive),
              user: user // Fresh user with entity, state, and proper getters
            });
          } catch (error) {
            // User might have been deleted - skip this session
            this.log.warn(`Failed to load user for session ${session.sessionId}: ${error}`);
          }
        }

        // Silently filter bootstrap sessions - routine cleanup, no logging needed
      }
    } catch {
      // File doesn't exist or is invalid, start with empty sessions
      // console.debug(`📝 ${this.toString()}: No existing session metadata found, starting fresh`);
      this.sessions = [];
    }
  }

  /**
   * Save sessions to per-project metadata file
   */
  private async saveSessionsToFile(): Promise<void> {
    try {
      await this.ensureSessionDirectories();
      const metadataPath = this.getSessionsMetadataPath();
      
      const metadata = {
        projectContext: WorkingDirConfig.getWorkingDir(),
        sessions: this.sessions,
        lastUpdated: new Date().toISOString(),
        version: '1.0.0'
      };
      
      await fs.writeFile(metadataPath, JSON.stringify(metadata, null, 2));
      // console.debug(`💾 ${this.toString()}: Saved ${this.sessions.length} sessions to ${metadataPath}`);
    } catch (error) {
      this.log.error('Failed to save session metadata:', error);
    }
  }

  /**
   * Initialize session daemon server with per-project session loading
   */
  protected async initialize(): Promise<void> {
    await super.initialize();
    await this.loadSessionsFromFile();
    
    // Start session cleanup interval - check every 5 minutes
    this.registerInterval('session-cleanup', () => {
      this.cleanupExpiredSessions().catch(error => {
        this.log.error('Cleanup interval error:', error);
      });
    }, 5 * 60 * 1000);
    
    // console.debug(`🏷️ ${this.toString()}: Session daemon server initialized with per-project persistence and expiry management`);
  }

  /**
   * Expire a session due to timeout or abandonment
   */
  private async expireSession(sessionId: UUID, _reason: string): Promise<void> {
    try {
      const sessionIndex = this.sessions.findIndex(s => s.sessionId === sessionId);
      if (sessionIndex !== -1) {
        // Mark as inactive and remove from memory
        this.sessions.splice(sessionIndex, 1);

        // Update persistent storage
        await this.saveSessionsToFile();
      }
    } catch (error) {
      this.log.error(`Failed to expire session ${sessionId}:`, error);
    }
  }

  /**
   * Cleanup expired sessions (runs periodically)
   */
  private async cleanupExpiredSessions(): Promise<void> {
    const now = new Date();
    const expiredSessions: SessionMetadata[] = [];

    for (const session of this.sessions) {
      const sessionAge = now.getTime() - session.lastActive.getTime();
      const maxAge = session.isShared ? this.BROWSER_SESSION_EXPIRY_MS : this.SESSION_EXPIRY_MS;
      
      if (sessionAge > maxAge) {
        expiredSessions.push(session);
      }
    }

    if (expiredSessions.length > 0) {
      // console.debug(`🧹 ${this.toString()}: Found ${expiredSessions.length} expired sessions for cleanup`);
      
      for (const session of expiredSessions) {
        await this.expireSession(session.sessionId, 'periodic_cleanup');
      }
    }
  }

  /**
   * Update session last active timestamp
   */
  private updateSessionActivity(sessionId: UUID): void {
    const session = this.sessions.find(s => s.sessionId === sessionId);
    if (session) {
      session.lastActive = new Date();
    }
  }

  /**
   * Public method for external daemons to update session activity
   * This should be called whenever a session is used by any daemon
   */
  public trackSessionActivity(sessionId: UUID): void {
    this.updateSessionActivity(sessionId);
  }

  /**
   * Cleanup method for graceful shutdown
   * Base class handles interval cleanup automatically
   */
  protected async cleanup(): Promise<void> {
    // Save current session state
    await this.saveSessionsToFile();
  }

  /**
   * Extract session operation from endpoint path (similar to CommandDaemon.extractCommand)
   */
  private extractOperation(endpoint: string): SessionOperation {
    // endpoint format: "session-daemon/get-default" or "server/session-daemon/current"
    const parts = endpoint.split('/');
    
    // Find the 'session-daemon' segment and extract everything after it
    const sessionIndex = parts.findIndex(part => part === 'session-daemon');
    if (sessionIndex === -1 || sessionIndex === parts.length - 1) {
      // If no operation specified, default to create
      return 'create';
    }
    
    // Return everything after 'session-daemon' joined with '/'
    // e.g., "session-daemon/get-default" -> "get-default"
    return parts.slice(sessionIndex + 1).join('/') as SessionOperation;
  }

  // Only source of truth in all daemons is here: processMessage(message: JTAGMessage): Promise<JTAGResponsePayload>
  protected async processMessage(message: JTAGMessage): Promise<SessionResponse> {
      // console.debug(`📨 ${this.toString()}: Handling message to ${message.endpoint}`);
      
      // Extract session operation from endpoint (similar to CommandDaemon pattern)
      const operation = this.extractOperation(message.endpoint);
      const requestPayload = message.payload;
      const requestContext = requestPayload.context ?? this.context;
      const requestSessionId = requestPayload.sessionId;
      
      if (!requestSessionId) {
        return createSessionErrorResponse(`Missing sessionId for operation: ${operation}`, requestContext, requestSessionId);
      }
      
      // Update session activity for any operation (except create/destroy)
      if (operation !== 'create' && operation !== 'destroy') {
        this.updateSessionActivity(requestSessionId);
      }
      
      try {
        // Route based on extracted operation from endpoint
        switch (operation) {
          case 'create':
            return await this.createOrGetSession(requestPayload as CreateSessionParams);
          case 'get':
            return await this.getSession(requestPayload as GetSessionParams);
          case 'list':
            return await this.listSessions(requestPayload as ListSessionsParams);
          case 'destroy':
            return await this.destroySession(requestPayload as DestroySessionParams);
          default:
            this.log.warn(`Unknown session operation: ${operation}`);
            return createSessionErrorResponse(`Unknown session operation: ${operation}`, requestContext, requestSessionId);
        }
      } catch (error) {
        const errorMessage = (error && typeof error === 'object' && 'message' in error)
          ? (error as { message: string }).message
          : String(error);
        this.log.error(`Error processing session operation ${operation}:`, errorMessage);
        return createSessionErrorResponse(errorMessage, requestContext, requestSessionId);
      }
    }

    /**
     * Find existing user (citizen) by uniqueId (single source of truth for identity)
     */
    private async findUserByUniqueId(uniqueId: string): Promise<BaseUser | null> {
      // Query users by uniqueId (the single source of truth for citizen identity)
      const result = await DataDaemon.query<UserEntity>({
        collection: COLLECTIONS.USERS,
        filter: { uniqueId }
      });

      if (!result.success || !result.data || result.data.length === 0) {
        return null;
      }

      // Found existing citizen - load and return
      const userRecord = result.data[0];
      const userId = userRecord.id;

      return await this.getUserById(userId);
    }

    /**
     * Find existing user by deviceId (browser device identification)
     *
     * The deviceId → userId mapping is stored in session metadata.
     * If we find a session with this deviceId, return its user.
     */
    private async findUserByDeviceId(deviceId: string): Promise<BaseUser | null> {
      // Look for existing session with this deviceId
      const existingSession = this.sessions.find(s =>
        (s as any).deviceId === deviceId
      );

      if (existingSession) {
        try {
          return await this.getUserById(existingSession.userId);
        } catch {
          // User was deleted, session is stale
          return null;
        }
      }

      return null;
    }

    /**
     * Load existing user (citizen) by ID
     */
    private async getUserById(userId: UUID): Promise<BaseUser> {
      // DEBUG: Always log getUserById calls to trace identity bugs - BYPASS LOGGER
      console.error(`🔍🔍🔍 getUserById CALLED: userId=${JSON.stringify(userId)}, type=${typeof userId}, value=${userId}`);
      this.log.info(`🔍 getUserById: userId=${JSON.stringify(userId)}, type=${typeof userId}, stringified=${String(userId)}`);

      // CRITICAL: Validate userId is not the string "undefined" (indicates serialization bug upstream)
      if (!userId || userId === 'undefined' || (userId as any) === undefined) {
        console.error(`❌❌❌ getUserById VALIDATION FAILED: userId=${userId}`);
        this.log.error(`❌ getUserById called with invalid userId`);
        this.log.error(`Stack trace:`, new Error().stack);
        throw new Error(`Invalid userId: ${userId} - this indicates a bug in session creation or identity resolution`);
      }

      // Load UserEntity from database
      const userResult = await DataDaemon.read<UserEntity>(COLLECTIONS.USERS, userId);
      if (!userResult.success || !userResult.data) {
        throw new Error(`User ${userId} not found in database`);
      }

      // DataRecord has { id, collection, data, metadata }
      // Ensure id is present in the data (Rust adapter may not include it in data.data)
      const userEntity = userResult.data.data as UserEntity;
      if (!userEntity.id) {
        (userEntity as any).id = userResult.data.id;
      }

      // Load UserStateEntity from database
      const stateResult = await DataDaemon.read<UserStateEntity>(COLLECTIONS.USER_STATES, userId);
      if (!stateResult.success || !stateResult.data) {
        throw new Error(`UserState for ${userId} not found in database`);
      }

      // Ensure id is present in the state data
      const userState = stateResult.data.data as UserStateEntity;
      if (!userState.id) {
        (userState as any).id = stateResult.data.id;
      }

      // Create appropriate User subclass based on type
      let user: BaseUser;
      if (userEntity.type === 'persona') {
        const personaDatabasePath = `.continuum/personas/${userId}/state.sqlite`;
        const storage = new SQLiteStateBackend(personaDatabasePath);
        user = new PersonaUser(userEntity, userState, storage);
      } else if (userEntity.type === 'agent') {
        const storage = new MemoryStateBackend();
        user = new AgentUser(userEntity, userState, storage);
      } else {
        const storage = new MemoryStateBackend();
        user = new HumanUser(userEntity, userState, storage);
      }

      await user.loadState();

      return user;
    }

    /**
     * Create User object with entity and state
     * Uses UserIdentityResolver + UserFactory to detect, lookup, or create user
     */
    /**
     * Find the seeded human owner
     *
     * In single-user dev environment, prefer the seeded owner over creating anonymous users.
     * The owner is identified as a human user whose uniqueId does NOT start with "anon-".
     * This avoids hardcoding names and works with any seeded owner.
     */
    private async findSeededHumanOwner(): Promise<BaseUser | null> {
      // Look for all human users
      const result = await DataDaemon.query<UserEntity>({
        collection: COLLECTIONS.USERS,
        filter: { type: 'human' }
      });

      if (!result.success || !result.data || result.data.length === 0) {
        return null;
      }

      // Find the first non-anonymous human (uniqueId doesn't start with "anon-")
      // DataRecord<T> wraps entity in .data property
      const seededOwner = result.data.find(record => {
        const entity = record.data;
        return entity.uniqueId && !entity.uniqueId.startsWith('anon-');
      });
      if (!seededOwner) {
        return null;
      }

      return await this.getUserById(seededOwner.data.id);
    }

    /**
     * Create anonymous human user for browser-ui clients
     *
     * This is called when a browser connects for the first time without
     * an existing userId in localStorage. Creates a human user that can
     * later be upgraded with proper authentication (login, passkey, etc.)
     */
    private async createAnonymousHuman(params: CreateSessionParams, deviceId?: string): Promise<BaseUser> {
      const createParams: UserCreateParams = createPayload(this.context, generateUUID(), {
        type: 'human',
        displayName: 'Anonymous User',
        uniqueId: `anon-${generateUUID().slice(0, 8)}`,  // Unique but not meant for lookup
        shortName: 'anon',
        bio: 'Anonymous browser user',
        avatar: undefined,
        provider: 'browser'
      });

      const user = await UserFactory.create(createParams, this.context, this.router);
      this.log.info(`✅ SessionDaemon: Created anonymous human user: ${user.entity.id.slice(0, 8)}... (deviceId: ${deviceId?.slice(0, 12) || 'none'})`);

      return user;
    }

    private async createUser(params: CreateSessionParams): Promise<BaseUser> {
      // Use UserIdentityResolver to detect identity and lookup existing user BEFORE creating
      const resolvedIdentity = await UserIdentityResolver.resolve();

      // If user already exists, return it (prevent ghost users!)
      if (resolvedIdentity.exists && resolvedIdentity.userId) {
        return await this.getUserById(resolvedIdentity.userId);
      }

      // User doesn't exist - create new one with resolved identity

      const createParams: UserCreateParams = createPayload(this.context, generateUUID(), {
        type: resolvedIdentity.type,
        displayName: resolvedIdentity.displayName,
        uniqueId: resolvedIdentity.uniqueId, // Stable uniqueId from resolver
        shortName: resolvedIdentity.shortName,
        bio: resolvedIdentity.bio,
        avatar: resolvedIdentity.avatar,
        provider: resolvedIdentity.agentInfo.name // Pass agent name as provider
      });

      // UserFactory.create() handles everything:
      // - Creates UserEntity and UserStateEntity in database
      // - Sets up appropriate storage backend (SQLite for personas, Memory for agents/humans)
      // - Creates the appropriate User subclass (PersonaUser/AgentUser/HumanUser)
      // - Loads initial state
      const user: BaseUser = await UserFactory.create(createParams, this.context, this.router);

      this.log.info(`✅ SessionDaemon: Created ${resolvedIdentity.type} user: ${user.entity.displayName} (${user.entity.id.slice(0, 8)}...)`);

      return user;
    }

    public async createOrGetSession(params: CreateSessionParams): Promise<CreateSessionResult | GetSessionResult> {
        if (params.isShared) {
          // Extract identity from enhanced connection context
          const enhancedContext = params.connectionContext as EnhancedConnectionContext | undefined;
          const clientType = enhancedContext?.clientType;
          const deviceId = enhancedContext?.identity?.deviceId;


          // For browser-ui: Use deviceId to find existing session (server owns user identity)
          if (clientType === 'browser-ui') {
            if (deviceId) {
              // Look for existing session with this deviceId
              const existingSession = this.sessions.find(s =>
                s.isShared &&
                s.isActive &&
                (s as any).deviceId === deviceId  // Session stores deviceId
              );

              if (existingSession) {
                // CRITICAL: Verify the session's user is a HUMAN, not an AI agent
                // This can happen if the session was created when Claude Code was detected
                try {
                  const sessionUser = await this.getUserById(existingSession.userId);
                  const userType = sessionUser?.entity?.type;  // FIX: was 'userType', field is 'type'

                  // If user is an AI agent (not human), discard session and create new
                  if (userType === 'agent' || userType === 'persona' || userType === 'system') {
                    this.log.warn(`⚠️ SessionDaemon: Session for device ${deviceId.slice(0, 12)}... has AI user (${sessionUser.displayName}) - discarding and creating human session`);
                    // Mark old session as inactive
                    existingSession.isActive = false;
                    // Fall through to create new session
                  } else if (sessionUser?.entity?.uniqueId?.startsWith('anon-')) {
                    // Anonymous user - check if seeded owner exists and prefer them
                    const seededOwner = await this.findSeededHumanOwner();
                    if (seededOwner) {
                      this.log.info(`✅ SessionDaemon: Upgrading anonymous user to seeded owner: ${seededOwner.displayName}`);
                      existingSession.userId = seededOwner.id;
                      existingSession.user = seededOwner;
                      // Save updated session
                      await this.saveSessionsToFile();
                    } else {
                      existingSession.user = sessionUser;
                    }
                    return createPayload(params.context, existingSession.sessionId, {
                      success: true,
                      timestamp: new Date().toISOString(),
                      operation: 'get',
                      session: existingSession
                    });
                  } else {
                    this.log.info(`✅ SessionDaemon: Found existing session for device ${deviceId.slice(0, 12)}... with human user ${sessionUser.displayName}`);
                    existingSession.user = sessionUser;

                    return createPayload(params.context, existingSession.sessionId, {
                      success: true,
                      timestamp: new Date().toISOString(),
                      operation: 'get',
                      session: existingSession
                    });
                  }
                } catch (error) {
                  this.log.warn(`Failed to verify user for session: ${error} - creating new session`);
                }
              }
            }
            // No valid existing session for this device - create new with human identity
            this.log.info(`🆕 SessionDaemon: Creating new human session for device ${deviceId?.slice(0, 12) || 'unknown'}...`);
            return await this.createSession(params);
          }

          // For CLI/agent/persona: Use userId or uniqueId as before
          const requestedUserId = enhancedContext?.identity?.userId;
          const existingSession = requestedUserId
            ? this.sessions.find(s => s.isShared && s.isActive && s.userId === requestedUserId)
            : null;

          if (existingSession) {
            this.log.info(`✅ SessionDaemon: Reusing existing session for user ${existingSession.userId.slice(0, 8)}...`);
            try {
              existingSession.user = await this.getUserById(existingSession.userId);
            } catch (error) {
              this.log.warn(`Failed to refresh user for session: ${error}`);
            }

            return createPayload(params.context, existingSession.sessionId, {
              success: true,
              timestamp: new Date().toISOString(),
              operation: 'get',
              session: existingSession
            });
          }
          this.log.info(`🆕 SessionDaemon: No matching session found, creating new one`);
        }
        return await this.createSession(params);
    }

    private async createSession(params: CreateSessionParams): Promise<CreateSessionResult> {
      // Always generate a new UUID for actual sessions - never use bootstrap IDs
      const actualSessionId = isBootstrapSession(params.sessionId) ? generateUUID() : params.sessionId;

      // ========================================================================
      // CLIENT TYPE-AWARE IDENTITY RESOLUTION
      // ========================================================================
      // Key insight: clientType determines HOW to resolve identity
      // - browser-ui: Use identity.userId (human), NEVER use agent detection
      // - cli: Use identity.uniqueId (@cli)
      // - persona: Use identity.userId (must exist in DB)
      // - agent: Use identity.uniqueId (claude-code, gpt-4, etc.)
      // ========================================================================

      const enhancedContext = params.connectionContext as EnhancedConnectionContext | undefined;
      const clientType: ClientType = enhancedContext?.clientType || 'cli';
      const identity = enhancedContext?.identity;
      const assistant = enhancedContext?.assistant;

      // DEBUG: Log what we received
      this.log.info(`🔍 Session create: clientType=${clientType}, hasEnhancedContext=${!!enhancedContext}, hasIdentity=${!!identity}, userId=${params.userId}, deviceId=${identity?.deviceId?.slice(0, 12)}`);

      // Log assistant for attribution (NOT for identity resolution!)
      if (assistant) {
        this.log.info(`🤖 Session assisted by ${assistant.name} (confidence: ${assistant.confidence})`);
      }

      let user: BaseUser;

      switch (clientType) {
        case 'browser-ui': {
          // Browser identity: Use deviceId to find/create user
          // Server is source of truth - browser doesn't send userId
          this.log.info(`🌐 Browser-ui session: resolving human identity from deviceId`);

          const deviceId = identity?.deviceId;

          if (deviceId) {
            // Look for existing user associated with this device
            const existingUser = await this.findUserByDeviceId(deviceId);
            if (existingUser) {
              user = existingUser;
              this.log.info(`✅ Found existing user for device: ${user.displayName} (${user.id.slice(0, 8)}...)`);
            } else {
              // New device - check for seeded owner (human without anon- prefix)
              const seededOwner = await this.findSeededHumanOwner();
              if (seededOwner) {
                user = seededOwner;
                this.log.info(`✅ Associating new device with seeded owner: ${user.displayName}`);
              } else {
                this.log.info(`📝 New device ${deviceId.slice(0, 12)}... - creating anonymous human`);
                user = await this.createAnonymousHuman(params, deviceId);
              }
            }
          } else {
            // No deviceId - check for seeded owner first
            const seededOwner = await this.findSeededHumanOwner();
            if (seededOwner) {
              user = seededOwner;
              this.log.info(`✅ Using seeded owner: ${user.displayName} (no deviceId)`);
            } else {
              this.log.info(`📝 No deviceId - creating anonymous human`);
              user = await this.createAnonymousHuman(params, undefined);
            }
          }
          break;
        }

        case 'cli': {
          // CLI identity: Use uniqueId (@cli or env user)
          const cliUniqueId = identity?.uniqueId || '@cli';
          this.log.info(`💻 CLI session: resolving uniqueId=${cliUniqueId}`);

          const existingCli = await this.findUserByUniqueId(cliUniqueId);
          if (existingCli) {
            user = existingCli;
          } else {
            user = await this.createUser(params);
          }
          break;
        }

        case 'persona': {
          // Persona identity: userId must exist (created by user/create command)
          this.log.info(`🎭 Persona session: resolving userId=${identity?.userId?.slice(0, 8)}...`);

          if (!identity?.userId) {
            throw new Error('Persona client must have explicit userId');
          }
          user = await this.getUserById(identity.userId);
          break;
        }

        case 'agent': {
          // Agent identity: Use uniqueId from agent name
          const agentUniqueId = identity?.uniqueId;
          this.log.info(`🤖 Agent session: resolving uniqueId=${agentUniqueId}`);

          if (!agentUniqueId) {
            throw new Error('Agent client must have uniqueId');
          }

          const existingAgent = await this.findUserByUniqueId(agentUniqueId);
          if (existingAgent) {
            user = existingAgent;
          } else {
            user = await this.createUser(params);
          }
          break;
        }

        default: {
          // Fallback: use legacy resolution (for backward compatibility)
          this.log.warn(`⚠️ Unknown clientType '${clientType}', using legacy resolution`);
          const uniqueId = params.connectionContext?.uniqueId;
          const existingUser = uniqueId ? await this.findUserByUniqueId(uniqueId) : null;

          if (existingUser) {
            user = existingUser;
          } else if (params.userId) {
            try {
              user = await this.getUserById(params.userId);
            } catch {
              user = await this.createUser(params);
            }
          } else {
            user = await this.createUser(params);
          }
        }
      }

      // Enrich context with caller type based on User type (enables caller-adaptive output)
      const enrichedContext = params.context;
      switch (user.entity.type) {
        case 'persona':
          enrichedContext.callerType = 'persona';
          break;
        case 'human':
          enrichedContext.callerType = 'human';
          break;
        case 'agent':
        case 'system':
          // External agents and system users treated as scripts
          enrichedContext.callerType = 'script';
          break;
        default:
          // Unknown types default to script (safest fallback) - KEEP THIS WARNING
          this.log.warn(`SessionDaemon: Unknown user type '${user.entity.type}', defaulting callerType to 'script'`);
          enrichedContext.callerType = 'script';
      }

      // Extract deviceId for browser-ui clients (server owns device → user mapping)
      const enhancedCtx = params.connectionContext as EnhancedConnectionContext | undefined;
      const deviceId = enhancedCtx?.identity?.deviceId;

      const newSession: SessionMetadata & { deviceId?: string } = {
        sourceContext: enrichedContext, // Use enriched context with callerType
        sessionId: actualSessionId, // Use generated UUID, not bootstrap ID
        category: params.category,
        displayName: params.displayName,
        userId: user.id, // Use userId from User object
        created: new Date(),
        lastActive: new Date(),
        isActive: true,
        isShared: params.isShared, // Use original isShared request
        user: user, // Add User object to session
        deviceId: deviceId  // Store deviceId for browser-ui lookup
      };

      this.sessions.push(newSession);

      // Persist session to per-project metadata file
      await this.saveSessionsToFile();

      return createPayload(params.context, actualSessionId, {
        success: true,
        timestamp: new Date().toISOString(),
        operation: params.operation,
        session: newSession
      });
    }

    private async getSession(params: GetSessionParams): Promise<GetSessionResult> {
      // console.debug(`⚡ ${this.toString()}: Getting session with ID: ${params.sessionId}`);

      const session = this.sessions.find(s => s.sessionId === params.sessionId);
      
      return createPayload(params.context, params.sessionId, {
        success: true,
        timestamp: new Date().toISOString(),
        operation: 'get',
        session
      });
    }

  private async listSessions(payload: ListSessionsParams): Promise<ListSessionsResult> {
    // console.debug(`⚡ ${this.toString()}: Listing sessions with filter:`, payload.filter);
    
    let sessions = this.sessions;
    const filter = payload.filter;

    if (filter?.category) {
      sessions = sessions.filter(s => s.category === filter.category);
    }
    if (filter?.isActive !== undefined) {
      sessions = sessions.filter(s => s.isActive === filter.isActive);
    }
    
    return createPayload(payload.context, payload.sessionId, {
      success: true,
      timestamp: new Date().toISOString(),
      operation: 'list',
      sessions
    });
  }   

  private async destroySession(params: DestroySessionParams): Promise<DestroySessionResult> {
    // console.debug(`⚡ ${this.toString()}: Destroying session ${params.sessionId}, reason: ${params.reason || 'unknown'}`);
    
    // Find session in memory
    const sessionIndex = this.sessions.findIndex(s => s.sessionId === params.sessionId);
    
    if (sessionIndex === -1) {
      this.log.warn(`Session ${params.sessionId} not found for destruction`);
      return createPayload(params.context, params.sessionId, {
        success: false,
        timestamp: new Date().toISOString(),
        operation: 'destroy',
        error: `Session ${params.sessionId} not found`
      });
    }

    // Remove session from memory
    const destroyedSession = this.sessions.splice(sessionIndex, 1)[0];

    // Update persistent storage
    try {
      await this.saveSessionsToFile();
      // console.debug(`✅ ${this.toString()}: Updated session metadata file`);
    } catch (error) {
      this.log.error(`Failed to update session metadata:`, error);
      // Session still destroyed from memory, but file sync failed
    }
    
    return createPayload(params.context, params.sessionId, {
      success: true,
      timestamp: new Date().toISOString(),
      operation: 'destroy',
      destroyedSessionId: destroyedSession.sessionId
    });
  }

  /**
   * Get current room for a session's user
   * Convenience method that delegates to SessionStateHelper
   */
  public async getCurrentRoom(sessionId: UUID): Promise<UUID | null> {
    const session = this.sessions.find(s => s.sessionId === sessionId);
    if (!session) {
      this.log.warn(`Session ${sessionId} not found for getCurrentRoom`);
      return null;
    }

    return await SessionStateHelper.getCurrentRoom(session.userId);
  }

  /**
   * Get current content item for a session's user
   */
  public async getCurrentContentItem(sessionId: UUID) {
    const session = this.sessions.find(s => s.sessionId === sessionId);
    if (!session) {
      this.log.warn(`Session ${sessionId} not found for getCurrentContentItem`);
      return null;
    }

    return await SessionStateHelper.getCurrentContentItem(session.userId);
  }

  /**
   * Get open rooms for a session's user
   */
  public async getOpenRooms(sessionId: UUID) {
    const session = this.sessions.find(s => s.sessionId === sessionId);
    if (!session) {
      this.log.warn(`Session ${sessionId} not found for getOpenRooms`);
      return [];
    }

    if (!session.userId) {
      this.log.warn(`Session ${sessionId} has no userId - cannot get open rooms`);
      return [];
    }

    return await SessionStateHelper.getOpenRooms(session.userId);
  }

  /**
   * Get user state for a session's user
   */
  public async getUserState(sessionId: UUID) {
    const session = this.sessions.find(s => s.sessionId === sessionId);
    if (!session) {
      this.log.warn(`Session ${sessionId} not found for getUserState`);
      return null;
    }

    if (!session.userId) {
      this.log.warn(`Session ${sessionId} has no userId - cannot get user state`);
      return null;
    }

    return await SessionStateHelper.getUserState(session.userId);
  }
}
