/**
 * Session Manager Message Types - Typed messages for session lifecycle daemon
 * Uses DaemonMessage<T> generic pattern for type safety
 */

import { DaemonMessage } from '../../base/DaemonProtocol';

// Re-export unified types from existing SessionTypes
export type { SessionInfo } from '../../../types/shared/SessionTypes';
export type { SessionExtractionRequest, SessionExtractionResponse } from '../../../types/shared/SessionTypes';

export interface SessionMessage {
  type: 'create' | 'join' | 'close' | 'cleanup' | 'extract' | 'list';
  payload: SessionCreateRequest | SessionJoinRequest | SessionCloseRequest | SessionCleanupRequest | any | SessionListRequest;
}

export interface SessionCreateRequest {
  sessionId?: string;
  sessionType: SessionType;
  owner: string;
  identity?: ConnectionIdentity;
  metadata?: Record<string, any>;
}

export interface SessionJoinRequest {
  sessionId: string;
  connectionId: string;
  identity: ConnectionIdentity;
}

export interface SessionCloseRequest {
  sessionId: string;
  reason?: string;
  force?: boolean;
}

export interface SessionCleanupRequest {
  sessionId?: string; // If not provided, cleanup all inactive sessions
  force?: boolean;
}

export interface SessionListRequest {
  owner?: string;
  sessionType?: SessionType;
  activeOnly?: boolean;
}

export interface ConnectionIdentity {
  type: 'portal' | 'validation' | 'user' | 'persona';
  name: string; // portal, git-hook, username, persona-name
  sessionContext?: string; // additional context like branch name for git hooks
}

export interface BrowserSession {
  id: string;
  type: SessionType;
  owner: string;
  created: Date;
  lastActive: Date;
  
  // Process tracking
  processes: {
    browser?: { pid: number; url: string };
    devtools?: { pid: number; tabs: string[] };
  };
  
  // Artifact storage
  artifacts: {
    storageDir: string;
    logs: { server: string[]; client: string[] };
    screenshots: string[];
    files: string[];
    recordings: string[];
    devtools: string[];
  };
  
  // Session state
  isActive: boolean;
  shouldAutoCleanup: boolean;
  cleanupAfterMs: number;
}

export type SessionType = 'persona' | 'portal' | 'git-hook' | 'development' | 'test';

export interface SessionEvent {
  type: 'session_created' | 'session_joined' | 'session_closed' | 'connection_registered';
  sessionId?: string;
  connectionId?: string;
  identity?: ConnectionIdentity;
  session?: BrowserSession;
  timestamp: Date;
  metadata?: Record<string, any>;
}

// Typed daemon messages for session manager
export type SessionDaemonMessage = DaemonMessage<SessionMessage>;

// Helper function to create session daemon messages
export function createSessionMessage(type: SessionMessage['type'], payload: SessionMessage['payload']): SessionDaemonMessage {
  return {
    id: crypto.randomUUID(),
    from: 'session-manager-client',
    to: 'session-manager',
    type: 'daemon',
    timestamp: new Date(),
    data: {
      type,
      payload
    }
  };
}