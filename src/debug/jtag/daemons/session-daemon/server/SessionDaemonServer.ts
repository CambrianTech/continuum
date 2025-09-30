/**
 * Session Daemon - Server Implementation
 * 
 * Server-specific session daemon that handles session identity management.
 * Follows the sparse override pattern - minimal server-specific logic.
 */

import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import { SessionDaemon} from '../shared/SessionDaemon';
import { HumanUser } from '../../../system/user/shared/HumanUser';
import { AgentUser } from '../../../system/user/shared/AgentUser';
import { PersonaUser } from '../../../system/user/shared/PersonaUser';
import { MemoryStateBackend } from '../../../system/user/storage/MemoryStateBackend';
import { UserEntity } from '../../../system/data/entities/UserEntity';
import { UserStateEntity } from '../../../system/data/entities/UserStateEntity';
import type { BaseUser } from '../../../system/user/shared/BaseUser';
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
} from '../shared/SessionTypes';
import { generateUUID } from '../../../system/core/types/CrossPlatformUUID';
import { createPayload } from '../../../system/core/types/JTAGTypes';
import { type JTAGMessage } from '../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import { isBootstrapSession } from '../../../system/core/types/SystemScopes';
import { WorkingDirConfig } from '../../../system/core/config/WorkingDirConfig';
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
  private sessionTimeouts: Map<UUID, ReturnType<typeof setTimeout>> = new Map(); // Timeout tracking
  private readonly SESSION_EXPIRY_MS = 30 * 60 * 1000; // 30 minutes for ephemeral sessions
  private readonly BROWSER_SESSION_EXPIRY_MS = 24 * 60 * 60 * 1000; // 24 hours for browser sessions
  private cleanupInterval?: ReturnType<typeof setInterval>;

  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);
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
        
        this.sessions = validSessions.map((session: SessionMetadata) => ({
          ...session,
          created: new Date(session.created),
          lastActive: new Date(session.lastActive)
        }));
        
        // Restore session timeouts for loaded sessions
        for (const session of this.sessions) {
          this.scheduleSessionExpiry(session.sessionId, session.isShared);
        }
        
        // Log cleanup if bootstrap sessions were filtered
        const filteredCount = metadata.sessions.length - validSessions.length;
        if (filteredCount > 0) {
          console.log(`🗑️ SessionDaemon: Filtered out ${filteredCount} bootstrap sessions during load`);
        }
        
        // console.debug(`📖 ${this.toString()}: Loaded ${this.sessions.length} sessions from ${metadataPath} with timeout restoration`);
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
      console.error(`❌ ${this.toString()}: Failed to save session metadata:`, error);
    }
  }

  /**
   * Initialize session daemon server with per-project session loading
   */
  protected async initialize(): Promise<void> {
    await super.initialize();
    await this.loadSessionsFromFile();
    
    // Start session cleanup interval - check every 5 minutes
    this.cleanupInterval = setInterval(() => {
      this.cleanupExpiredSessions().catch(error => {
        console.error(`❌ ${this.toString()}: Cleanup interval error:`, error);
      });
    }, 5 * 60 * 1000);
    
    // console.debug(`🏷️ ${this.toString()}: Session daemon server initialized with per-project persistence and expiry management`);
  }

  /**
   * Schedule session expiry timeout for a given session
   */
  private scheduleSessionExpiry(sessionId: UUID, isShared: boolean): void {
    // Clear existing timeout if any
    const existingTimeout = this.sessionTimeouts.get(sessionId);
    if (existingTimeout) {
      clearTimeout(existingTimeout);
    }

    // Determine expiry time based on session type
    const expiryMs = isShared ? this.BROWSER_SESSION_EXPIRY_MS : this.SESSION_EXPIRY_MS;
    
    const timeout = setTimeout(async () => {
      // console.debug(`⏰ ${this.toString()}: Session ${sessionId} expired due to timeout (${expiryMs}ms)`);
      await this.expireSession(sessionId, 'timeout');
    }, expiryMs);

    this.sessionTimeouts.set(sessionId, timeout);
    // console.debug(`⏲️ ${this.toString()}: Scheduled expiry for session ${sessionId} in ${expiryMs}ms`);
  }

  /**
   * Expire a session due to timeout or abandonment
   */
  private async expireSession(sessionId: UUID, _reason: string): Promise<void> {
    try {
      const sessionIndex = this.sessions.findIndex(s => s.sessionId === sessionId);
      if (sessionIndex !== -1) {
        // console.debug(`💀 ${this.toString()}: Expiring ${session.isShared ? 'shared' : 'ephemeral'} session ${sessionId} (${reason})`);
        
        // Mark as inactive and remove from memory
        this.sessions.splice(sessionIndex, 1);
        
        // Clear timeout tracking
        const timeout = this.sessionTimeouts.get(sessionId);
        if (timeout) {
          clearTimeout(timeout);
          this.sessionTimeouts.delete(sessionId);
        }
        
        // Update persistent storage
        await this.saveSessionsToFile();
        // console.debug(`✅ ${this.toString()}: Session ${sessionId} expired and cleaned up`);
      }
    } catch (error) {
      console.error(`❌ ${this.toString()}: Failed to expire session ${sessionId}:`, error);
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
   * Update session last active timestamp and reschedule expiry
   */
  private updateSessionActivity(sessionId: UUID): void {
    const session = this.sessions.find(s => s.sessionId === sessionId);
    if (session) {
      session.lastActive = new Date();
      this.scheduleSessionExpiry(sessionId, session.isShared);
      // console.debug(`🔄 ${this.toString()}: Updated activity for session ${sessionId}`);
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
   */
  public async cleanup(): Promise<void> {
    // console.debug(`🧹 ${this.toString()}: Starting cleanup...`);
    
    // Clear cleanup interval
    if (this.cleanupInterval) {
      clearInterval(this.cleanupInterval);
      this.cleanupInterval = undefined;
    }
    
    // Clear all session timeouts
    for (const timeout of this.sessionTimeouts.values()) {
      clearTimeout(timeout);
    }
    this.sessionTimeouts.clear();
    
    // Save current session state
    await this.saveSessionsToFile();
    
    // console.debug(`✅ ${this.toString()}: Cleanup completed`);
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

  // Only source of truth in all daemons is here:  handleMessage(message: JTAGMessage): Promise<JTAGResponsePayload>
  async handleMessage(message: JTAGMessage): Promise<SessionResponse> {
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
            console.warn(`⚠️ ${this.toString()}: Unknown session operation: ${operation}`);
            return createSessionErrorResponse(`Unknown session operation: ${operation}`, requestContext, requestSessionId);
        }
      } catch (error) {
        const errorMessage = (error && typeof error === 'object' && 'message' in error)
          ? (error as { message: string }).message
          : String(error);
        console.error(`❌ ${this.toString()}: Error processing session operation ${operation}:`, errorMessage);
        return createSessionErrorResponse(errorMessage, requestContext, requestSessionId);
      }
    }

    /**
     * Create User object with entity and state
     * Uses appropriate storage backend based on category
     */
    private async createUser(params: CreateSessionParams): Promise<BaseUser> {
      const userId = params.userId ?? generateUUID();
      const deviceId = `device-${generateUUID()}`;

      // Enhanced agent detection using connectionContext
      const agentInfo = params.connectionContext?.agentInfo;
      const isDetectedAgent = agentInfo?.detected && agentInfo.confidence > 0.5;

      // Determine actual user type (may override category if agent detected)
      let userType: 'human' | 'agent' | 'persona';
      if (params.category === 'persona') {
        userType = 'persona';
      } else if (params.category === 'agent' || isDetectedAgent) {
        userType = 'agent';
        console.log(`🤖 SessionDaemon: Detected AI agent: ${agentInfo?.name ?? 'Unknown'} (confidence: ${agentInfo?.confidence ?? 0})`);
      } else {
        userType = 'human';
      }

      // Create UserEntity
      const userEntity = new UserEntity();
      userEntity.id = userId;
      userEntity.displayName = params.displayName;
      userEntity.type = userType;

      // Create UserStateEntity with defaults appropriate for user type
      const userState = new UserStateEntity();
      userState.id = generateUUID();
      userState.userId = userId;
      userState.deviceId = deviceId;

      // Agent-specific preferences (lighter state, no persistence assumptions)
      if (userType === 'agent') {
        userState.preferences = {
          maxOpenTabs: 5, // Agents typically work with fewer contexts
          autoCloseAfterDays: 1, // Short retention for agent state
          rememberScrollPosition: false, // Agents don't need scroll positions
          syncAcrossDevices: false // Agents are ephemeral
        };
      } else {
        userState.preferences = {
          maxOpenTabs: 10,
          autoCloseAfterDays: 30,
          rememberScrollPosition: true,
          syncAcrossDevices: false
        };
      }

      userState.contentState = {
        openItems: [],
        lastUpdatedAt: new Date()
      };

      // Select storage backend based on user type
      // Agents: Always use MemoryStateBackend (ephemeral, no persistence)
      // Personas: TODO - Use SQLiteStateBackend with dedicated database
      // Humans: TODO - Browser clients use LocalStorageStateBackend
      const storage = new MemoryStateBackend();

      if (userType === 'agent') {
        console.log(`🧠 SessionDaemon: Using MemoryStateBackend for agent ${params.displayName}`);
      }

      // Create appropriate User subclass
      let user: BaseUser;
      if (userType === 'persona') {
        user = new PersonaUser(userEntity, userState, storage, userId);
      } else if (userType === 'agent') {
        user = new AgentUser(userEntity, userState, storage);
      } else {
        user = new HumanUser(userEntity, userState, storage);
      }

      // Load state from storage (will be empty for new users)
      await user.loadState();

      return user;
    }

    public async createOrGetSession(params: CreateSessionParams): Promise<CreateSessionResult | GetSessionResult> {
        if (params.isShared) {
          // Check for existing shared session
          const existingSession = this.sessions.find(s => s.isShared && s.isActive);
          if (existingSession) {
            console.log(`✅ SessionDaemon: Reusing existing valid shared session: ${existingSession.sessionId}`);
            return createPayload(params.context, existingSession.sessionId, {
              success: true,
              timestamp: new Date().toISOString(),
              operation: 'get',
              session: existingSession
            });
          }
          console.log(`🆕 SessionDaemon: No existing valid shared session found, creating new one`);
        }
        return await this.createSession(params);
    }

    private async createSession(params: CreateSessionParams): Promise<CreateSessionResult> {
      // console.debug(`⚡ ${this.toString()}: Creating new session:`, params);

      // Always generate a new UUID for actual sessions - never use bootstrap IDs
      const actualSessionId = isBootstrapSession(params.sessionId) ? generateUUID() : params.sessionId;

      // Create User object with entity and state
      const user = await this.createUser(params);

      const newSession: SessionMetadata = {
        sourceContext: params.context,
        sessionId: actualSessionId, // Use generated UUID, not bootstrap ID
        category: params.category,
        displayName: params.displayName,
        userId: user.id, // Use userId from User object
        created: new Date(),
        lastActive: new Date(),
        isActive: true,
        isShared: params.isShared, // Use original isShared request
        user: user // Add User object to session
      };

      // console.debug(`✅ ${this.toString()}: New session created:`, newSession);

      this.sessions.push(newSession);

      // Schedule expiry timeout for the new session
      this.scheduleSessionExpiry(newSession.sessionId, newSession.isShared);

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
      console.warn(`⚠️ ${this.toString()}: Session ${params.sessionId} not found for destruction`);
      return createPayload(params.context, params.sessionId, {
        success: false,
        timestamp: new Date().toISOString(),
        operation: 'destroy',
        error: `Session ${params.sessionId} not found`
      });
    }

    // Remove session from memory
    const destroyedSession = this.sessions.splice(sessionIndex, 1)[0];
    // console.debug(`✅ ${this.toString()}: Removed session ${params.sessionId} from memory`);
    
    // Clear session timeout if it exists
    const timeout = this.sessionTimeouts.get(params.sessionId);
    if (timeout) {
      clearTimeout(timeout);
      this.sessionTimeouts.delete(params.sessionId);
      // console.debug(`⏲️ ${this.toString()}: Cleared timeout for destroyed session ${params.sessionId}`);
    }
    
    // Update persistent storage
    try {
      await this.saveSessionsToFile();
      // console.debug(`✅ ${this.toString()}: Updated session metadata file`);
    } catch (error) {
      console.error(`❌ ${this.toString()}: Failed to update session metadata:`, error);
      // Session still destroyed from memory, but file sync failed
    }
    
    return createPayload(params.context, params.sessionId, {
      success: true,
      timestamp: new Date().toISOString(),
      operation: 'destroy',
      destroyedSessionId: destroyedSession.sessionId
    });
  }
}
