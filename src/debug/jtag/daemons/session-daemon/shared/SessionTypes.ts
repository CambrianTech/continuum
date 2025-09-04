import type { JTAGContext, JTAGPayload } from '../../../system/core/types/JTAGTypes';
import { type UUID } from '../../../system/core/types/CrossPlatformUUID';
import type { BaseResponsePayload } from '../../../system/core/types/ResponseTypes';

/**
 * Entity type and directory categories - passed in from connect() or caller
 */
export type SessionCategory = 'user' | 'persona' | 'agent' | 'system';

export type SessionOperation = 'create' | 'get' | 'list' | 'destroy';

export interface SessionIdentity {
  category: SessionCategory; // user | persona | agent | system
  userId: UUID; // Reference to actual user record (via UserDaemon)
  displayName: string; // "Claude", "Joel", etc. - passed from connect()
}

/**
 * Session Metadata - Core identity information only
 */
export interface SessionMetadata extends SessionIdentity {
  sessionId: UUID;
  created: Date;
  isActive: boolean; // Whether session is currently active
  lastActive: Date; // Last time session was active
  sourceContext: JTAGContext;
  isShared: boolean;
}

 /**
 * Session Payloads
 */
export interface SessionParams extends JTAGPayload {
  readonly operation: SessionOperation;
}

export interface SessionResult extends BaseResponsePayload {
  readonly operation: SessionOperation
}

export interface SessionErrorResponse extends BaseResponsePayload {
  readonly operation?: SessionOperation
  error: string;
}

//CREATE method definitions
export interface CreateSessionParams extends SessionParams, SessionIdentity {
  readonly operation: 'create';
  readonly isShared: boolean; // Whether session is shared across users
}

export interface CreateSessionResult extends SessionResult {
  session: SessionMetadata;
}

//GET method definitions
export interface GetSessionParams extends SessionParams {
  readonly operation: 'get';
}

export interface GetSessionResult extends SessionResult {
  session: SessionMetadata | undefined;
}

//LIST method definitions
export interface ListSessionsParams extends JTAGPayload {
  readonly operation: 'list';
  readonly filter?: Partial<SessionIdentity> & { isActive?: boolean };
}

export interface ListSessionsResult extends SessionResult {
  sessions: SessionMetadata[];
}

export interface DestroySessionParams extends SessionParams {
  reason?: string; // Reason for destruction (e.g., 'client_disconnect', 'timeout', 'cleanup')
}

export interface DestroySessionResult extends SessionResult {
  destroyedSessionId?: UUID;
}

/**
 * Session Context - Replaces primitive sessionId strings with rich session objects
 * 
 * This provides session resolution capability throughout the system to prevent
 * session ID tracking issues where original client sessionIds are used instead
 * of resolved shared session IDs.
 */
export interface SessionContext {
  /** The original sessionId from the client request (may be bootstrap session) */
  readonly originalSessionId: UUID;
  /** The resolved sessionId (may be different from original if resolved to shared session) */
  readonly resolvedSessionId: UUID;
  /** Full session metadata if available (may be undefined during resolution) */
  readonly session?: SessionMetadata;
  /** Whether this session context has been resolved through SessionDaemon */
  readonly isResolved: boolean;
}

/**
 * Create an unresolved session context from a sessionId
 */
export const createSessionContext = (sessionId: UUID): SessionContext => ({
  originalSessionId: sessionId,
  resolvedSessionId: sessionId,
  session: undefined,
  isResolved: false
});

/**
 * Create a resolved session context from session metadata
 */
export const createResolvedSessionContext = (
  originalSessionId: UUID, 
  session: SessionMetadata
): SessionContext => ({
  originalSessionId,
  resolvedSessionId: session.sessionId,
  session,
  isResolved: true
});

/**
 * Get the effective sessionId that should be used for directory creation, logging, etc.
 * This is always the resolved sessionId, never the original client sessionId.
 */
export const getEffectiveSessionId = (sessionContext: SessionContext): UUID => {
  return sessionContext.resolvedSessionId;
};

/**
 * Get the session category for directory organization
 * Defaults to 'user' if session metadata is not available
 */
export const getSessionCategory = (sessionContext: SessionContext): SessionCategory => {
  return sessionContext.session?.category ?? 'user';
};

//ALL expected results of this daemon's operations
export type SessionResponse =  SessionErrorResponse | CreateSessionResult | GetSessionResult | ListSessionsResult | DestroySessionResult;