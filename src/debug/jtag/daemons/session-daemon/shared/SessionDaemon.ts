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
import { createPayload } from '@shared/JTAGTypes';
import { JTAGRouter } from '@shared/JTAGRouter';
import { createSessionSuccessResponse, createSessionErrorResponse, type SessionResponse } from '@shared/ResponseTypes';
import { generateUUID, type UUID } from '@shared/CrossPlatformUUID';

/**
 * Session Types
 */
export type SessionType = 'development' | 'production' | 'test' | 'persona';
export type SessionOwner = 'human-joel' | 'ai-claude' | 'shared';

/**
 * Session Metadata - Core identity information only
 */
export interface SessionMetadata {
  id: UUID;
  type: SessionType;
  owner: SessionOwner;
  created: Date;
  lastActive: Date;
  isActive: boolean;
}

/**
 * Session Payloads
 */
export interface CreateSessionPayload extends JTAGPayload {
  readonly type: 'create';
  readonly sessionType: SessionType;
  readonly owner: SessionOwner;
}

export interface GetSessionPayload extends JTAGPayload {
  readonly type: 'get';
  readonly sessionId: UUID;
}

export interface ListSessionsPayload extends JTAGPayload {
  readonly type: 'list';
  readonly filter?: {
    owner?: SessionOwner;
    type?: SessionType;
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
 * Factory functions for session payloads
 */
export const createCreateSessionPayload = (
  context: JTAGContext,
  sessionId: UUID,
  data: { sessionType: SessionType; owner: SessionOwner }
): CreateSessionPayload => createPayload(context, sessionId, {
  type: 'create' as const,
  sessionType: data.sessionType,
  owner: data.owner
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
  public readonly subpath: string = 'session';
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
    const session: SessionMetadata = {
      id: realSessionId,
      type: payload.sessionType,
      owner: payload.owner,
      created: new Date(),
      lastActive: new Date(),
      isActive: true
    };
    
    this.sessions.set(session.id, session);
    
    console.log(`🏷️ ${this.toString()}: Created session ${session.id} (${session.type}/${session.owner})`);
    console.log(`🔄 ${this.toString()}: Bootstrap session ${payload.sessionId} → real session ${realSessionId}`);
    
    // Emit session_created event for other daemons to coordinate
    // Note: Event emission would be handled by the router's event system
    // this.router.eventSystem.emit('session_created', { sessionId: session.id, ...session });
    
    return createSessionSuccessResponse(
      { 
        sessionId: realSessionId,  // The real session ID for client to use going forward
        metadata: session 
      },
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
      if (payload.filter.owner) {
        sessionList = sessionList.filter(s => s.owner === payload.filter?.owner);
      }
      if (payload.filter.type) {
        sessionList = sessionList.filter(s => s.type === payload.filter?.type);
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