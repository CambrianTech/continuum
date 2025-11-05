/**
 * Session Manager Daemon - Server-side session management with ProcessBasedDaemon architecture
 * Token-efficient, focused session lifecycle management
 */

import { ProcessBasedDaemon } from '../../base/ProcessBasedDaemon';
import { DaemonResponse } from '../../base/DaemonProtocol';
import { DaemonType } from '../../base/DaemonTypes';
import { ContinuumContext, UUID, uuidValidator } from '../../../types/shared/core/ContinuumTypes';
import { 
  SessionDaemonMessage, 
  SessionInfo, 
  SessionRequest, 
  SessionResponse, 
  SessionCreationConfig,
  SessionListFilter,
  SessionStats,
  SessionEvents,
  SessionPaths,
  SessionValidation,
  SessionUtils,
  SessionType
} from '../shared/SessionManagerTypes';
import * as fs from 'fs/promises';
import * as path from 'path';

export class SessionManagerDaemon extends ProcessBasedDaemon<SessionDaemonMessage> {
  readonly name = 'session-manager';
  readonly version = '2.0.0';
  readonly daemonType: DaemonType = DaemonType.SESSION_MANAGER;

  private sessions = new Map<UUID, SessionInfo>();
  private artifactRoot: string;
  private cleanupInterval?: NodeJS.Timeout;

  // Session creation semaphore to prevent race conditions
  private sessionCreationLock = new Map<string, Promise<SessionInfo>>();

  constructor(context?: ContinuumContext, artifactRoot: string = '.continuum/sessions') {
    super(context, {
      queueSize: 5000,
      batchSize: 50,
      processTimeoutMs: 10000,
      resourceLimits: {
        memory: '32MB',
        cpu: '5%'
      }
    });

    this.artifactRoot = artifactRoot;
    this.startCleanupMonitoring();
  }

  /**
   * Process a single session message
   */
  protected async processMessage(message: any): Promise<DaemonResponse> {
    try {
      const { type, payload } = message.data;

      switch (type) {
        case 'session.create':
          return await this.handleCreateSession(payload);
        case 'session.get':
          return this.handleGetSession(payload);
        case 'session.list':
          return this.handleListSessions(payload);
        case 'session.connect':
          return await this.handleConnectSession(payload);
        case 'session.stop':
          return await this.handleStopSession(payload);
        case 'session.fork':
          return await this.handleForkSession(payload);
        case 'session.cleanup':
          return await this.handleCleanupSession(payload);
        case 'session.stats':
          return this.handleGetStats();
        default:
          throw new Error(`Unknown session message type: ${type}`);
      }
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error),
        messageId: message.id
      };
    }
  }

  /**
   * Handle session creation request
   */
  private async handleCreateSession(payload: SessionCreationConfig): Promise<DaemonResponse> {
    const validation = SessionValidation.validateSessionRequest(payload);
    if (!validation.valid) {
      return { success: false, error: validation.error || 'Invalid session request' };
    }

    try {
      const session = await this.createSession(payload);
      
      // Emit session created event
      this.emitSessionEvent(SessionEvents.created(
        session.id, 
        session.type, 
        session.owner, 
        { paths: session.paths }
      ));

      return {
        success: true,
        data: { session }
      };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }

  /**
   * Handle get session request
   */
  private handleGetSession(payload: { sessionId: UUID }): DaemonResponse {
    const { sessionId } = payload;
    const validation = SessionValidation.validateSessionId(sessionId);
    if (!validation.valid) {
      return { success: false, error: validation.error || 'Invalid session ID' };
    }

    const session = this.sessions.get(sessionId);
    if (!session) {
      return { success: false, error: `Session ${sessionId} not found` };
    }

    // Update last active time
    session.lastActive = new Date();

    return {
      success: true,
      data: { session }
    };
  }

  /**
   * Handle list sessions request
   */
  private handleListSessions(payload: { filter?: SessionListFilter }): DaemonResponse {
    const { filter = {} } = payload;
    
    let sessions = Array.from(this.sessions.values());
    
    // Apply filters
    if (filter.type) {
      sessions = sessions.filter(s => s.type === filter.type);
    }
    if (filter.owner) {
      sessions = sessions.filter(s => s.owner === filter.owner);
    }
    if (filter.active !== undefined) {
      sessions = sessions.filter(s => s.isActive === filter.active);
    }

    return {
      success: true,
      data: {
        sessions,
        total: sessions.length,
        filter
      }
    };
  }

  /**
   * Handle session connect request - intelligent session routing
   */
  private async handleConnectSession(payload: SessionRequest): Promise<DaemonResponse> {
    const validation = SessionValidation.validateSessionRequest(payload);
    if (!validation.valid) {
      return { success: false, error: validation.error || 'Invalid session request' };
    }

    try {
      let session: SessionInfo;
      let action: 'joined_existing' | 'created_new' | 'forked_from' = 'joined_existing';

      // Intelligent session routing
      if (payload.preference === 'new') {
        session = await this.createSession(payload);
        action = 'created_new';
      } else if (payload.preference?.startsWith('fork:')) {
        const forkFromId = payload.preference.split(':')[1] as UUID;
        session = await this.forkSession(forkFromId, payload);
        action = 'forked_from';
      } else if (payload.preference && payload.preference.length > 10) {
        // Specific session ID requested
        const existingSession = this.sessions.get(payload.preference as UUID);
        if (!existingSession) {
          return { success: false, error: `Session ${payload.preference} not found` };
        }
        session = existingSession;
        session.lastActive = new Date();
      } else {
        // Find or create shared session
        session = await this.findOrCreateSharedSession(payload);
        action = session.created.getTime() > Date.now() - 1000 ? 'created_new' : 'joined_existing';
      }

      // Emit appropriate event
      if (action === 'created_new') {
        this.emitSessionEvent(SessionEvents.created(session.id, session.type, session.owner));
      } else {
        this.emitSessionEvent(SessionEvents.joined(session.id, { source: payload.source }));
      }

      // Build response
      const response: SessionResponse = {
        sessionId: session.id,
        action,
        paths: session.paths,
        interface: 'http://localhost:9000',
        version: this.version,
        launched: {
          browser: payload.capabilities?.includes('browser') || false,
          webserver: true,
          newLogFiles: action === 'created_new' || action === 'forked_from'
        },
        commands: {
          stop: `continuum session-stop ${session.id}`,
          fork: `continuum session-fork ${session.id}`,
          info: `continuum session-info ${session.id}`,
          otherClients: `continuum session-clients ${session.id}`
        }
      };

      return {
        success: true,
        data: response
      };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }

  /**
   * Handle session stop request
   */
  private async handleStopSession(payload: { sessionId: UUID; preserveArtifacts?: boolean }): Promise<DaemonResponse> {
    const { sessionId, preserveArtifacts = true } = payload;
    
    const session = this.sessions.get(sessionId);
    if (!session) {
      return { success: false, error: `Session ${sessionId} not found` };
    }

    try {
      // Mark as inactive
      session.isActive = false;
      session.lastActive = new Date();

      // Kill processes if they exist
      if (session.processes.browserPid) {
        try {
          process.kill(session.processes.browserPid);
        } catch (error) {
          // Process might already be dead
        }
      }

      if (session.processes.devtoolsPid) {
        try {
          process.kill(session.processes.devtoolsPid);
        } catch (error) {
          // Process might already be dead
        }
      }

      // Cleanup artifacts if requested
      if (!preserveArtifacts) {
        await this.cleanupSessionArtifacts(session);
        this.sessions.delete(sessionId);
      }

      // Emit stopped event
      this.emitSessionEvent(SessionEvents.stopped(sessionId, { preserveArtifacts }));

      return {
        success: true,
        data: { sessionId, preserveArtifacts }
      };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }

  /**
   * Handle session fork request
   */
  private async handleForkSession(payload: { fromSessionId: UUID; config: SessionCreationConfig }): Promise<DaemonResponse> {
    const { fromSessionId, config } = payload;
    
    try {
      const newSession = await this.forkSession(fromSessionId, config);
      
      // Emit forked event
      this.emitSessionEvent(SessionEvents.forked(newSession.id, fromSessionId));

      return {
        success: true,
        data: { session: newSession, fromSessionId }
      };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }

  /**
   * Handle session cleanup request
   */
  private async handleCleanupSession(payload: { sessionId?: UUID }): Promise<DaemonResponse> {
    const { sessionId } = payload;
    
    try {
      if (sessionId) {
        // Clean specific session
        const session = this.sessions.get(sessionId);
        if (session && SessionUtils.needsCleanup(session)) {
          await this.cleanupSession(session);
        }
      } else {
        // Clean all sessions that need cleanup
        const sessionsToClean = Array.from(this.sessions.values())
          .filter(SessionUtils.needsCleanup);
        
        for (const session of sessionsToClean) {
          await this.cleanupSession(session);
        }
      }

      return {
        success: true,
        data: { cleanedSessions: sessionId ? 1 : 'all_eligible' }
      };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }

  /**
   * Handle get stats request
   */
  private handleGetStats(): DaemonResponse {
    const sessions = Array.from(this.sessions.values());
    const activeSessions = sessions.filter(s => s.isActive);
    
    const byType = sessions.reduce((acc, session) => {
      acc[session.type] = (acc[session.type] || 0) + 1;
      return acc;
    }, {} as Record<SessionType, number>);

    const stats: SessionStats = {
      total: sessions.length,
      active: activeSessions.length,
      inactive: sessions.length - activeSessions.length,
      byType
    };

    return {
      success: true,
      data: stats
    };
  }

  /**
   * Core session creation with semaphore protection
   */
  private async createSession(config: SessionCreationConfig): Promise<SessionInfo> {
    const lockKey = `${config.type}-${config.owner}`;
    
    // Prevent race conditions
    if (this.sessionCreationLock.has(lockKey)) {
      return await this.sessionCreationLock.get(lockKey)!;
    }

    const sessionPromise = this.doCreateSession(config);
    this.sessionCreationLock.set(lockKey, sessionPromise);
    
    try {
      return await sessionPromise;
    } finally {
      this.sessionCreationLock.delete(lockKey);
    }
  }

  /**
   * Actually create the session
   */
  private async doCreateSession(config: SessionCreationConfig): Promise<SessionInfo> {
    const sessionId = uuidValidator.generate();
    const basePath = SessionPaths.generateBasePath(this.artifactRoot, config.type, config.owner, sessionId);
    const paths = SessionPaths.generatePaths(basePath);

    // Create session info
    const session: SessionInfo = {
      id: sessionId,
      type: config.type,
      owner: config.owner,
      created: new Date(),
      lastActive: new Date(),
      isActive: true,
      paths,
      processes: {},
      autoCleanup: config.autoCleanup ?? (config.type !== 'development'),
      cleanupAfterMs: config.cleanupAfterMs ?? (2 * 60 * 60 * 1000) // 2 hours
    };

    // Create directories
    await this.createSessionDirectories(session);

    // Store session
    this.sessions.set(sessionId, session);

    return session;
  }

  /**
   * Find or create shared session
   */
  private async findOrCreateSharedSession(config: SessionCreationConfig): Promise<SessionInfo> {
    // Look for existing shared session
    const existingSession = Array.from(this.sessions.values())
      .find(s => s.type === config.type && s.owner === 'shared' && s.isActive);

    if (existingSession) {
      existingSession.lastActive = new Date();
      return existingSession;
    }

    // Create new shared session
    return await this.createSession({
      ...config,
      owner: 'shared'
    });
  }

  /**
   * Fork session from existing session
   */
  private async forkSession(fromSessionId: UUID, config: SessionCreationConfig): Promise<SessionInfo> {
    const sourceSession = this.sessions.get(fromSessionId);
    if (!sourceSession) {
      throw new Error(`Source session ${fromSessionId} not found`);
    }

    // Create new session with forked context
    const newSession = await this.createSession({
      ...config,
      context: `forked-from-${fromSessionId}`
    });

    // TODO: Copy relevant artifacts from source session
    return newSession;
  }

  /**
   * Create session directories
   */
  private async createSessionDirectories(session: SessionInfo): Promise<void> {
    // Create base path
    await fs.mkdir(session.paths.base, { recursive: true });

    // Create subdirectories
    const subdirs = SessionPaths.getSubdirectories();
    for (const subdir of subdirs) {
      await fs.mkdir(path.join(session.paths.base, subdir), { recursive: true });
    }

    // Create session metadata
    const metadata = SessionUtils.generateMetadata(session);
    await fs.writeFile(
      path.join(session.paths.base, 'session-info.json'),
      JSON.stringify(metadata, null, 2)
    );

    // Create initial log files
    const startMessage = SessionUtils.generateStartMessage(session);
    await fs.writeFile(path.join(session.paths.logs, 'server.log'), startMessage);
  }

  /**
   * Clean up session
   */
  private async cleanupSession(session: SessionInfo): Promise<void> {
    session.isActive = false;
    
    // Kill processes
    if (session.processes.browserPid) {
      try {
        process.kill(session.processes.browserPid);
      } catch (error) {
        // Process might already be dead
      }
    }

    if (session.processes.devtoolsPid) {
      try {
        process.kill(session.processes.devtoolsPid);
      } catch (error) {
        // Process might already be dead
      }
    }

    // Clean up artifacts
    await this.cleanupSessionArtifacts(session);
    
    // Remove from sessions
    this.sessions.delete(session.id);
    
    // Emit cleanup event
    this.emitSessionEvent(SessionEvents.cleanup(session.id));
  }

  /**
   * Clean up session artifacts
   */
  private async cleanupSessionArtifacts(session: SessionInfo): Promise<void> {
    try {
      await fs.rm(session.paths.base, { recursive: true, force: true });
    } catch (error) {
      // Ignore cleanup errors
    }
  }

  /**
   * Start cleanup monitoring
   */
  private startCleanupMonitoring(): void {
    this.cleanupInterval = setInterval(async () => {
      const sessionsToClean = Array.from(this.sessions.values())
        .filter(SessionUtils.needsCleanup);

      for (const session of sessionsToClean) {
        await this.cleanupSession(session);
      }
    }, 5 * 60 * 1000); // Check every 5 minutes
  }

  /**
   * Emit session event
   */
  private emitSessionEvent(event: ReturnType<typeof SessionEvents.created>): void {
    // TODO: Integrate with proper event system
    console.log(`📡 Session event: ${event.type} for ${event.sessionId}`);
  }

  /**
   * Clean shutdown
   */
  async stop(): Promise<void> {
    if (this.cleanupInterval) {
      clearInterval(this.cleanupInterval);
    }

    // Close all active sessions gracefully
    const activeSessions = Array.from(this.sessions.values()).filter(s => s.isActive);
    for (const session of activeSessions) {
      session.isActive = false;
    }

    await super.stop();
  }
}