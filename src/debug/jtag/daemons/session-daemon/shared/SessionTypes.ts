import type { JTAGContext, JTAGPayload } from '../../../system/core/types/JTAGTypes';
import { type UUID } from '../../../system/core/types/CrossPlatformUUID';
import type { BaseResponsePayload } from '../../../system/core/types/ResponseTypes';
import type { BaseUser } from '../../../system/user/shared/BaseUser';
import type { JTAGConnectionContextInput } from '../../../system/core/client/shared/JTAGClient';

// ============================================================================
// Client Identity Types
// ============================================================================

/**
 * Client type - WHERE the connection comes from (not WHO operates it)
 *
 * - browser-ui: Human in browser (may have AI assisting)
 * - cli: Terminal ./jtag commands (may be run by AI or human)
 * - persona: Internal AI citizen (PersonaUser instances)
 * - agent: External AI using JTAG as tool (Claude Code, GPT, etc.)
 * - remote: Future authenticated remote user
 */
export type ClientType = 'browser-ui' | 'cli' | 'persona' | 'agent' | 'remote';

/**
 * Connection identity - WHO the session belongs to
 *
 * Varies by client type:
 * - browser-ui: userId from localStorage, deviceId for fingerprint
 * - cli: uniqueId from @cli or env user
 * - persona: userId from PersonaUser.id
 * - agent: uniqueId from agent name (e.g., 'claude-code')
 * - remote: Determined by auth claims
 */
export interface ConnectionIdentity {
  userId?: UUID;        // browser-ui: from localStorage, persona: from entity
  uniqueId?: string;    // cli/agent: stable identifier like '@cli' or 'claude-code'
  deviceId?: string;    // browser-ui: device fingerprint
}

/**
 * Assistant context - WHO is helping operate the session
 *
 * This is METADATA ONLY, not for identity resolution!
 * Used for debugging, attribution, and capability detection.
 */
export interface AssistantContext {
  name: string;         // "Claude Code", "GPT-4", etc.
  confidence: number;   // Detection confidence (0-1)
  source: string;       // "env", "header", "plugin", etc.
}

/**
 * Auth context - for future authentication
 *
 * Will support:
 * - none: No authentication (current default)
 * - token: API keys with scoped permissions
 * - passkey: WebAuthn passkey auth
 * - webauthn: Full WebAuthn with hardware keys
 */
export interface AuthContext {
  method: 'none' | 'token' | 'passkey' | 'webauthn';
  token?: string;
  claims?: {
    sub: string;        // Subject (user ID)
    iat: number;        // Issued at
    exp: number;        // Expiry
    scope: string[];    // Permissions
  };
}

/**
 * Enhanced connection context with client type and identity separation
 */
export interface EnhancedConnectionContext {
  /** Client type - WHERE the connection comes from */
  clientType: ClientType;
  /** Identity - WHO the session belongs to */
  identity: ConnectionIdentity;
  /** Assistant - WHO is helping (metadata only) */
  assistant?: AssistantContext;
  /** Auth - future authentication context */
  auth?: AuthContext;
  /** Legacy agent info for backward compatibility */
  agentInfo?: JTAGConnectionContextInput['agentInfo'];
  /** Output preferences */
  outputPreferences?: JTAGConnectionContextInput['outputPreferences'];
  /** Capabilities */
  capabilities?: JTAGConnectionContextInput['capabilities'];
}

// ============================================================================
// Session Category Types
// ============================================================================

/**
 * Entity type and directory categories - passed in from connect() or caller
 */
export type SessionCategory = 'user' | 'persona' | 'agent' | 'system';

export type SessionOperation = 'create' | 'get' | 'list' | 'destroy';

export interface SessionIdentity {
  category: SessionCategory; // user | persona | agent | system
  userId?: UUID; // Optional - server resolves from connectionContext for browser-ui clients
  displayName: string; // "Claude", "Joel", etc. - passed from connect()
}

/**
 * Session Metadata - Core identity information only
 * Note: userId is REQUIRED in metadata because server always resolves it during creation
 */
export interface SessionMetadata extends Omit<SessionIdentity, 'userId'> {
  sessionId: UUID;
  userId: UUID;  // Required in metadata - server resolves from connectionContext during creation
  created: Date;
  isActive: boolean; // Whether session is currently active
  lastActive: Date; // Last time session was active
  sourceContext: JTAGContext;
  isShared: boolean;
  user?: BaseUser; // User object with entity and state (optional for backwards compatibility)
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
  readonly connectionContext?: JTAGConnectionContextInput; // Connection context with agentInfo for detection
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