/**
 * Modular Session Manager Daemon - Process-based session lifecycle management
 * Pure session management following ProcessBasedDaemon architecture
 */

import { ProcessBasedDaemon } from '../../base/ProcessBasedDaemon';
import { DaemonResponse, DaemonMessage } from '../../base/DaemonProtocol';
import { DaemonType } from '../../base/DaemonTypes';
import { ContinuumContext } from '../../../types/shared/core/ContinuumTypes';
import { 
  SessionMessage, 
  SessionCreateRequest,
  SessionJoinRequest,
  SessionCloseRequest,
  SessionCleanupRequest,
  SessionListRequest,
  BrowserSession,
  ConnectionIdentity,
  SessionEvent
} from '../shared/SessionMessageTypes';
import { SessionExtractionRequest, SessionExtractionResponse } from '../../../types/shared/SessionTypes';
import { DirectoryService } from '../services/DirectoryService';
import { CleanupService } from '../services/CleanupService';
import { DAEMON_EVENT_BUS } from '../../base/DaemonEventBus';
import { SystemEventType } from '../../base/EventTypes';

export class ModularSessionManagerDaemon extends ProcessBasedDaemon<SessionMessage> {
  readonly name = 'session-manager';
  readonly version = '1.0.0';
  readonly daemonType: DaemonType = 'session-manager' as DaemonType;

  private sessions = new Map<string, BrowserSession>();
  private connectionIdentities = new Map<string, ConnectionIdentity>();
  private cleanupInterval?: NodeJS.Timeout;
  private eventListeners = new Set<(event: SessionEvent) => void>();
  private directoryService: DirectoryService;
  private cleanupService: CleanupService;
  
  // SEMAPHORE: Prevent race conditions in session creation
  private sessionCreationLock = new Map<string, Promise<BrowserSession>>();

  constructor(context?: ContinuumContext, artifactRoot: string = '.continuum/sessions') {
    super(context, {
      queueSize: 1000,
      batchSize: 10,
      processTimeoutMs: 5000
    });

    this.directoryService = new DirectoryService(context || { sessionId: 'default' }, artifactRoot);
    this.cleanupService = new CleanupService(context || { sessionId: 'default' }, this.sessions, this.log.bind(this));
    
    // Start cleanup interval
    this.startCleanupInterval();
  }

  protected async processMessage(message: DaemonMessage<SessionMessage>): Promise<DaemonResponse> {
    try {
      switch (message.data.type) {
        case 'create':
          return await this.handleCreateSession(message.data.payload as SessionCreateRequest);
        
        case 'join':
          return await this.handleJoinSession(message.data.payload as SessionJoinRequest);
        
        case 'close':
          return await this.handleCloseSession(message.data.payload as SessionCloseRequest);
        
        case 'cleanup':
          return await this.handleCleanupSessions(message.data.payload as SessionCleanupRequest);
        
        case 'extract':
          return await this.handleExtractSession(message.data.payload as SessionExtractionRequest);
        
        case 'list':
          return await this.handleListSessions(message.data.payload as SessionListRequest);
        
        default:
          return { success: false, error: `Unknown session message type: ${message.data.type}` };
      }
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Unknown error';
      this.log(`Error processing session message: ${errorMessage}`, 'error');
      return { success: false, error: errorMessage };
    }
  }

  private async handleCreateSession(request: SessionCreateRequest): Promise<DaemonResponse> {
    const sessionId = request.sessionId || crypto.randomUUID();
    
    // Prevent race conditions in session creation
    if (this.sessionCreationLock.has(sessionId)) {
      const existingSession = await this.sessionCreationLock.get(sessionId)!;
      return { success: true, data: { session: existingSession } };
    }

    const creationPromise = this.createSessionInternal(sessionId, request);
    this.sessionCreationLock.set(sessionId, creationPromise);

    try {
      const session = await creationPromise;
      this.sessionCreationLock.delete(sessionId);
      
      // Emit session created event
      this.emitSessionEvent({
        type: 'session_created',
        sessionId: session.id,
        session,
        timestamp: new Date()
      });

      return { success: true, data: { session } };
    } catch (error) {
      this.sessionCreationLock.delete(sessionId);
      throw error;
    }
  }

  private async createSessionInternal(sessionId: string, request: SessionCreateRequest): Promise<BrowserSession> {
    // Check if session already exists
    if (this.sessions.has(sessionId)) {
      return this.sessions.get(sessionId)!;
    }

    // Create session directories using DirectoryService
    const storageStructure = await this.directoryService.createSessionStorage(
      sessionId, 
      request.sessionType, 
      request.owner
    );
    
    const session: BrowserSession = {
      id: sessionId,
      type: request.sessionType,
      owner: request.owner,
      created: new Date(),
      lastActive: new Date(),
      processes: {},
      artifacts: storageStructure,
      isActive: true,
      shouldAutoCleanup: request.sessionType !== 'development', // Keep dev sessions around
      cleanupAfterMs: 24 * 60 * 60 * 1000 // 24 hours
    };

    this.sessions.set(sessionId, session);
    this.log(`Session created: ${sessionId} (${request.sessionType}) for ${request.owner}`);
    
    return session;
  }

  private async handleJoinSession(request: SessionJoinRequest): Promise<DaemonResponse> {
    const session = this.sessions.get(request.sessionId);
    if (!session) {
      return { success: false, error: `Session not found: ${request.sessionId}` };
    }

    // Register connection identity
    this.connectionIdentities.set(request.connectionId, request.identity);
    
    // Update session activity
    session.lastActive = new Date();
    
    // Emit session joined event
    this.emitSessionEvent({
      type: 'session_joined',
      sessionId: session.id,
      connectionId: request.connectionId,
      identity: request.identity,
      session,
      timestamp: new Date()
    });

    return { success: true, data: { session, connectionId: request.connectionId } };
  }

  private async handleCloseSession(request: SessionCloseRequest): Promise<DaemonResponse> {
    const session = this.sessions.get(request.sessionId);
    if (!session) {
      return { success: false, error: `Session not found: ${request.sessionId}` };
    }

    session.isActive = false;
    session.lastActive = new Date();

    // Emit session closed event
    this.emitSessionEvent({
      type: 'session_closed',
      sessionId: session.id,
      session,
      timestamp: new Date(),
      metadata: { reason: request.reason }
    });

    if (request.force) {
      await this.cleanupService.cleanupSession(session.id);
      this.sessions.delete(request.sessionId);
    }

    return { success: true, data: { sessionId: request.sessionId } };
  }

  private async handleCleanupSessions(request: SessionCleanupRequest): Promise<DaemonResponse> {
    const cleanedSessions: string[] = [];

    if (request.sessionId) {
      // Cleanup specific session
      const session = this.sessions.get(request.sessionId);
      if (session && (!session.isActive || request.force)) {
        await this.cleanupService.cleanupSession(session.id);
        this.sessions.delete(request.sessionId);
        cleanedSessions.push(request.sessionId);
      }
    } else {
      // Cleanup all inactive sessions
      for (const [sessionId, session] of this.sessions.entries()) {
        if (!session.isActive || request.force) {
          const shouldCleanup = session.shouldAutoCleanup &&
            (Date.now() - session.lastActive.getTime()) > session.cleanupAfterMs;
          
          if (shouldCleanup || request.force) {
            await this.cleanupService.cleanupSession(session.id);
            this.sessions.delete(sessionId);
            cleanedSessions.push(sessionId);
          }
        }
      }
    }

    return { success: true, data: { cleanedSessions } };
  }

  private async handleExtractSession(_request: any): Promise<DaemonResponse> {
    try {
      const sessions = Array.from(this.sessions.values());
      const response: SessionExtractionResponse = {
        sessionId: sessions.length > 0 ? sessions[0].id : '',
        extractedFrom: 'none',
        sessionExists: sessions.length > 0
      };
      
      return { success: true, data: response };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Session extraction failed';
      return { success: false, error: errorMessage };
    }
  }

  private async handleListSessions(request: SessionListRequest): Promise<DaemonResponse> {
    let filteredSessions = Array.from(this.sessions.values());

    if (request.owner) {
      filteredSessions = filteredSessions.filter(s => s.owner === request.owner);
    }

    if (request.sessionType) {
      filteredSessions = filteredSessions.filter(s => s.type === request.sessionType);
    }

    if (request.activeOnly) {
      filteredSessions = filteredSessions.filter(s => s.isActive);
    }

    return { success: true, data: { sessions: filteredSessions } };
  }

  private emitSessionEvent(event: SessionEvent): void {
    // Emit to event bus with proper payload structure
    DAEMON_EVENT_BUS.emitEvent(SystemEventType.SESSION_CREATED, {
      sessionId: event.sessionId || '',
      serverLogPath: event.session?.artifacts?.logs?.server?.[0] || '',
      sessionType: event.session?.type || 'unknown',
      owner: event.session?.owner || 'unknown'
    });
    
    // Emit to local listeners
    this.eventListeners.forEach(listener => {
      try {
        listener(event);
      } catch (error) {
        this.log(`Error in session event listener: ${error}`, 'error');
      }
    });
  }

  private startCleanupInterval(): void {
    this.cleanupInterval = setInterval(async () => {
      try {
        await this.handleCleanupSessions({ force: false });
      } catch (error) {
        this.log(`Error in session cleanup: ${error}`, 'error');
      }
    }, 60 * 60 * 1000); // Run every hour
  }

  async onStop(): Promise<void> {
    if (this.cleanupInterval) {
      clearInterval(this.cleanupInterval);
    }
    await super.onStop();
  }

  // Public API for other daemons
  getSession(sessionId: string): BrowserSession | undefined {
    return this.sessions.get(sessionId);
  }

  getConnectionIdentity(connectionId: string): ConnectionIdentity | undefined {
    return this.connectionIdentities.get(connectionId);
  }

  addSessionEventListener(listener: (event: SessionEvent) => void): void {
    this.eventListeners.add(listener);
  }

  removeSessionEventListener(listener: (event: SessionEvent) => void): void {
    this.eventListeners.delete(listener);
  }
}