/**
 * Session Daemon - Identity Service Only
 * 
 * Microarchitecture approach: Create session UUID + track basic lifecycle
 * Single responsibility: Session identity management and basic metadata
 * Size: ~100-150 lines maximum per microarchitecture principles
 * 
 * What it does NOT do:
 * - Directory creation → DirectoryDaemon
 * - File/artifact management → ArtifactDaemon  
 * - WebSocket handling → Router
 * - Browser processes → BrowserDaemon
 * - Message routing → RouterDaemon
 */

import { DaemonBase } from '@shared/DaemonBase';
import type { JTAGContext, JTAGMessage, JTAGPayload } from '@shared/JTAGTypes';
import { createPayload, JTAGMessageFactory } from '@shared/JTAGTypes';
import { JTAGRouter } from '@shared/JTAGRouter';
import { createSessionSuccessResponse, createSessionErrorResponse, type SessionResponse } from '@shared/ResponseTypes';
import { generateUUID, type UUID } from '@shared/CrossPlatformUUID';
import { type SessionCategory } from '@shared/SystemScopes';
import { JTAG_DAEMON_ENDPOINTS, JTAG_ENDPOINTS } from '@shared/JTAGEndpoints';

/**
 * Session Metadata - Core identity information only
 */
export interface SessionMetadata {
  id: UUID;
  category: SessionCategory; // user | persona | agent | system
  userId: UUID; // Reference to actual user record (via UserDaemon)
  displayName: string; // "Claude", "Joel", etc. - passed from connect()
  created: Date;
  lastActive: Date;
  isActive: boolean;
}

/**
 * Session Payloads
 */
export interface CreateSessionPayload extends JTAGPayload {
  readonly type: 'create';
  readonly category: SessionCategory;
  readonly displayName: string;
  readonly userId?: UUID; // Optional - will generate if not provided
}

export interface GetSessionPayload extends JTAGPayload {
  readonly type: 'get';
  readonly sessionId: UUID;
}

export interface ListSessionsPayload extends JTAGPayload {
  readonly type: 'list';
  readonly filter?: {
    category?: SessionCategory;
    isActive?: boolean;
  };
}

export interface ActivateSessionPayload extends JTAGPayload {
  readonly type: 'activate';
  readonly sessionId: UUID;
}

export interface DeactivateSessionPayload extends JTAGPayload {
  readonly type: 'deactivate';
  readonly sessionId: UUID;
}

export interface EndSessionPayload extends JTAGPayload {
  readonly type: 'end';
  readonly sessionId: UUID;
}

export type SessionPayload = CreateSessionPayload | GetSessionPayload | ListSessionsPayload | 
                            ActivateSessionPayload | DeactivateSessionPayload | EndSessionPayload;

/**
 * Session Creation Response - standardized response structure
 */
export interface SessionCreationResponse {
  readonly sessionId: UUID;  // The new session ID created by server
  readonly metadata: SessionMetadata;
}

/**
 * Factory functions for session payloads
 */
export const createCreateSessionPayload = (
  context: JTAGContext,
  sessionId: UUID,
  data: { category: SessionCategory; displayName: string; userId?: UUID }
): CreateSessionPayload => createPayload(context, sessionId, {
  type: 'create' as const,
  category: data.category,
  displayName: data.displayName,
  userId: data.userId
});

export const createGetSessionPayload = (
  context: JTAGContext,
  sessionId: UUID,
  targetSessionId: UUID
): GetSessionPayload => createPayload(context, sessionId, {
  type: 'get' as const,
  sessionId: targetSessionId
});

/**
 * Session Daemon - Identity Service Only
 * 
 * Core responsibility: Create session UUID + track basic lifecycle
 * Does NOT handle directories, files, WebSockets, browsers, routing, etc.
 */
export abstract class SessionDaemon extends DaemonBase {
  public readonly subpath: string = JTAG_DAEMON_ENDPOINTS.SESSION;
  private sessions = new Map<UUID, SessionMetadata>();

  constructor(context: JTAGContext, router: JTAGRouter) {
    super('session-daemon', context, router);
  }

  /**
   * Initialize session daemon
   */
  protected async initialize(): Promise<void> {
    console.log(`🏷️ ${this.toString()}: Session daemon initialized - identity service ready`);
  }

  /**
   * Smart session creation with intelligent routing
   * Works from any environment - routes to appropriate handlers
   */
  async getOrCreateSession(): Promise<UUID> {
    console.log(`🏷️ ${this.toString()}: Getting or creating session with smart routing`);
    
    // 1. Check if session exists (delegate to browser if needed for sessionStorage)
    const existingSession = await this.checkExistingSession();
    if (existingSession) {
      console.log(`🏷️ ${this.toString()}: Found existing session: ${existingSession}`);
      return existingSession;
    }

    // 2. Generate new session (delegate to server if needed for security)
    console.log(`🏷️ ${this.toString()}: No existing session, generating new one`);
    const newSession = await this.generateNewSession();
    
    // 3. Store session (delegate to appropriate environment)
    await this.storeSession(newSession);
    
    console.log(`✅ ${this.toString()}: Session ready: ${newSession}`);
    return newSession;
  }

  /**
   * Check for existing session - override in environment-specific classes
   */
  protected async checkExistingSession(): Promise<UUID | null> {
    // Environment-specific implementation in browser/server classes
    return null; // Default: no existing session
  }

  /**
   * Generate new session - smart routing to server for secure generation
   */
  private async generateNewSession(): Promise<UUID> {
    if (this.context.environment === 'server') {
      // Server can generate directly
      const newSessionId = generateUUID() as UUID;
      console.log(`🛡️ ${this.toString()}: Generated secure session on server: ${newSessionId}`);
      return newSessionId;
    } else {
      // Browser must request from server via router
      return await this.requestSessionFromServer();
    }
  }

  /**
   * Store session - override in environment-specific classes
   */
  protected async storeSession(sessionId: UUID): Promise<void> {
    // Environment-specific implementation in browser/server classes
    console.log(`💾 ${this.toString()}: Storing session: ${sessionId}`);
  }

  /**
   * Request session from server (browser environment only)
   */
  private async requestSessionFromServer(): Promise<UUID> {
    console.log(`🛡️ ${this.toString()}: Requesting secure session from server`);
    
    const createSessionPayload = createCreateSessionPayload(
      this.context,
      this.context.uuid as UUID,
      { category: 'user', displayName: 'Session User' }
    );
    
    const createMessage = JTAGMessageFactory.createRequest(
      this.context,
      `${this.context.environment}/${this.subpath}`,
      JTAG_ENDPOINTS.SESSION.SERVER,
      createSessionPayload,
      JTAGMessageFactory.generateCorrelationId()
    );
    
    const result = await this.router.postMessage(createMessage);
    console.log(`🛡️ ${this.toString()}: Server response:`, result);
    
    // Extract session ID from response using standardized type
    if (result && typeof result === 'object' && 'response' in result) {
      const response = (result as any).response;
      
      // Current server response format has responseSessionId at top level
      if (response && response.responseSessionId) {
        console.log(`✅ ${this.toString()}: Extracted session ID: ${response.responseSessionId}`);
        return response.responseSessionId as UUID;
      }
      
      // Try standardized SessionCreationResponse format
      if (response && response.data && response.data.sessionId) {
        const sessionData = response.data as SessionCreationResponse;
        console.log(`✅ ${this.toString()}: Extracted session ID (standard): ${sessionData.sessionId}`);
        return sessionData.sessionId;
      }
      
      // Fallback to other possible formats for compatibility
      if (response && response.sessionId) {
        console.log(`✅ ${this.toString()}: Extracted session ID (fallback): ${response.sessionId}`);
        return response.sessionId as UUID;
      }
    }
    
    throw new Error(`Failed to get session from server: ${JSON.stringify(result)}`);
  }

  /**
   * Smart disconnect - retains session by default
   */
  async disconnect(clearSession = false): Promise<void> {
    console.log(`🔌 ${this.toString()}: Disconnect (clearSession=${clearSession})`);
    
    if (clearSession) {
      await this.clearSession();
    } else {
      // Smart retention based on environment
      if (this.context.environment === 'browser') {
        console.log(`🔌 Browser disconnect - session retained in sessionStorage`);
      } else if (this.context.environment === 'server') {
        console.log(`🔌 Server disconnect - session retained in memory`);
      }
    }
  }

  /**
   * Explicit session clearing - override in environment-specific classes
   */
  protected async clearSession(): Promise<void> {
    console.log(`🗑️ ${this.toString()}: Clearing session`);
    // Environment-specific implementation in browser/server classes
  }

  /**
   * Handle incoming session messages
   */
  async handleMessage(message: JTAGMessage): Promise<SessionResponse> {
    const sessionPayload = message.payload as SessionPayload;
    
    try {
      switch (sessionPayload.type) {
        case 'create':
          return await this.createSession(sessionPayload as CreateSessionPayload);
        case 'get':
          return this.getSession(sessionPayload as GetSessionPayload);
        case 'list':
          return this.listSessions(sessionPayload as ListSessionsPayload);
        case 'activate':
          return this.setActive(sessionPayload as ActivateSessionPayload, true);
        case 'deactivate':
          return this.setActive(sessionPayload as DeactivateSessionPayload, false);
        case 'end':
          return await this.endSession(sessionPayload as EndSessionPayload);
        default:
          console.warn(`⚠️ ${this.toString()}: Unknown session message type`);
          return createSessionErrorResponse('Unknown session message type', (sessionPayload as any).context, (sessionPayload as any).sessionId);
      }
    } catch (error: any) {
      console.error(`❌ ${this.toString()}: Error processing session message:`, error.message);
      return createSessionErrorResponse(error.message, sessionPayload.context, sessionPayload.sessionId);
    }
  }

  /**
   * Create session identity (core responsibility)
   * 
   * Bootstrap protocol: Client sends with context.uuid as temporary sessionId,
   * server creates real session and returns both for proper routing.
   */
  private async createSession(payload: CreateSessionPayload): Promise<SessionResponse> {
    const realSessionId = generateUUID();
    const userId = payload.userId || generateUUID(); // Generate userId if not provided
    
    const session: SessionMetadata = {
      id: realSessionId,
      category: payload.category,
      userId: userId,
      displayName: payload.displayName,
      created: new Date(),
      lastActive: new Date(),
      isActive: true
    };
    
    this.sessions.set(session.id, session);
    
    console.log(`🏷️ ${this.toString()}: Created session ${session.id} (${session.category}/${session.displayName})`);
    console.log(`🔄 ${this.toString()}: Bootstrap session ${payload.sessionId} → real session ${realSessionId}`);
    
    // Emit session_created event for other daemons to coordinate
    // Note: Event emission would be handled by the router's event system
    // this.router.eventSystem.emit('session_created', { sessionId: session.id, ...session });
    
    const sessionResponse: SessionCreationResponse = {
      sessionId: realSessionId,  // The real session ID for client to use going forward
      metadata: session 
    };
    
    return createSessionSuccessResponse(
      sessionResponse,
      payload.context,
      payload.sessionId  // Route back using bootstrap sessionId (context.uuid)
    );
  }

  /**
   * Simple session lookup
   */
  private getSession(payload: GetSessionPayload): SessionResponse {
    const session = this.sessions.get(payload.sessionId);
    return session 
      ? createSessionSuccessResponse({ sessionId: session.id, metadata: session }, payload.context, payload.sessionId)
      : createSessionErrorResponse('Session not found', payload.context, payload.sessionId);
  }

  /**
   * List sessions with optional filtering
   */
  private listSessions(payload: ListSessionsPayload): SessionResponse {
    let sessionList = Array.from(this.sessions.values());
    
    if (payload.filter) {
      if (payload.filter.category) {
        sessionList = sessionList.filter(s => s.category === payload.filter?.category);
      }
      if (payload.filter.isActive !== undefined) {
        sessionList = sessionList.filter(s => s.isActive === payload.filter?.isActive);
      }
    }
    
    return createSessionSuccessResponse(
      { sessions: sessionList },
      payload.context,
      payload.sessionId
    );
  }

  /**
   * Activate/deactivate session
   */
  private setActive(payload: ActivateSessionPayload | DeactivateSessionPayload, isActive: boolean): SessionResponse {
    const session = this.sessions.get(payload.sessionId);
    if (!session) {
      return createSessionErrorResponse('Session not found', payload.context, payload.sessionId);
    }
    
    session.isActive = isActive;
    session.lastActive = new Date();
    
    console.log(`🏷️ ${this.toString()}: Session ${session.id} ${isActive ? 'activated' : 'deactivated'}`);
    
    return createSessionSuccessResponse(
      { sessionId: session.id, metadata: session },
      payload.context,
      payload.sessionId
    );
  }

  /**
   * Clean session cleanup
   */
  private async endSession(payload: EndSessionPayload): Promise<SessionResponse> {
    const session = this.sessions.get(payload.sessionId);
    if (!session) {
      return createSessionErrorResponse('Session not found', payload.context, payload.sessionId);
    }
    
    session.isActive = false;
    
    // Emit session_ended event for other daemons to coordinate cleanup
    // this.router.eventSystem.emit('session_ended', { sessionId: payload.sessionId });
    
    this.sessions.delete(payload.sessionId);
    
    console.log(`🏷️ ${this.toString()}: Ended session ${payload.sessionId}`);
    
    return createSessionSuccessResponse(
      { sessionId: payload.sessionId },
      payload.context,
      payload.sessionId
    );
  }
}