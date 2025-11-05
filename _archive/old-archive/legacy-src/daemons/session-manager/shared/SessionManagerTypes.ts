/**
 * Session Manager Types - Shared across all contexts
 * Token-efficient session management with clear responsibilities
 */

import { UUID } from '../../../types/shared/core/ContinuumTypes';

export type SessionType = 'development' | 'production' | 'test' | 'portal' | 'git-hook' | 'persona';

export interface SessionInfo {
  id: UUID;
  type: SessionType;
  owner: string;
  created: Date;
  lastActive: Date;
  isActive: boolean;
  
  // Lightweight session paths - no complex artifact tracking
  paths: {
    base: string;
    logs: string;
    screenshots: string;
    files: string;
  };
  
  // Simple process tracking
  processes: {
    browserPid?: number;
    devtoolsPid?: number;
  };
  
  // Cleanup configuration
  autoCleanup: boolean;
  cleanupAfterMs: number;
}

export interface SessionRequest {
  type: SessionType;
  owner: string;
  preference?: 'current' | 'new' | string; // specific session ID or 'fork:sessionId'
  context?: string;
  source?: string;
  capabilities?: string[];
  focus?: boolean;
  killZombies?: boolean;
}

export interface SessionResponse {
  sessionId: UUID;
  action: 'joined_existing' | 'created_new' | 'forked_from';
  paths: SessionInfo['paths'];
  interface: string;
  version: string;
  launched: {
    browser: boolean;
    webserver: boolean;
    newLogFiles: boolean;
  };
  commands: {
    stop: string;
    fork: string;
    info: string;
    otherClients: string;
  };
}

export interface SessionCreationConfig {
  type: SessionType;
  owner: string;
  autoCleanup?: boolean;
  cleanupAfterMs?: number;
  context?: string;
}

export interface SessionListFilter {
  type?: SessionType;
  owner?: string;
  active?: boolean;
}

export interface SessionStats {
  total: number;
  active: number;
  inactive: number;
  byType: Record<SessionType, number>;
}

// Message types for daemon communication
export type SessionMessageType = 
  | 'session.create'
  | 'session.get'
  | 'session.list'
  | 'session.connect'
  | 'session.stop'
  | 'session.fork'
  | 'session.cleanup'
  | 'session.stats';

export interface SessionMessage {
  type: SessionMessageType;
  payload: any;
  sessionId?: UUID;
  timestamp: Date;
}

export interface SessionDaemonMessage {
  id: UUID;
  type: SessionMessageType;
  data: SessionMessage;
  timestamp: Date;
}

// Elegant Event Architecture - Token-efficient session events
export type SessionEventType = 'created' | 'joined' | 'stopped' | 'forked' | 'cleanup';

export interface SessionEvent {
  type: SessionEventType;
  sessionId: UUID;
  timestamp: Date;
  // Optional context - only included when relevant
  sessionType?: SessionType;
  owner?: string;
  metadata?: Record<string, unknown>;
}

// Event factory for consistent event creation
export const SessionEvents = {
  /**
   * Create session event with minimal required data
   */
  create(type: SessionEventType, sessionId: UUID, options: {
    sessionType?: SessionType;
    owner?: string;
    metadata?: Record<string, unknown>;
  } = {}): SessionEvent {
    return {
      type,
      sessionId,
      timestamp: new Date(),
      ...options
    };
  },

  /**
   * Create session created event
   */
  created(sessionId: UUID, sessionType: SessionType, owner: string, metadata?: Record<string, unknown>): SessionEvent {
    return this.create('created', sessionId, { sessionType, owner, ...(metadata && { metadata }) });
  },

  /**
   * Create session joined event
   */
  joined(sessionId: UUID, metadata?: Record<string, unknown>): SessionEvent {
    return this.create('joined', sessionId, { ...(metadata && { metadata }) });
  },

  /**
   * Create session stopped event
   */
  stopped(sessionId: UUID, metadata?: Record<string, unknown>): SessionEvent {
    return this.create('stopped', sessionId, { ...(metadata && { metadata }) });
  },

  /**
   * Create session forked event
   */
  forked(sessionId: UUID, fromSessionId: UUID, metadata?: Record<string, unknown>): SessionEvent {
    return this.create('forked', sessionId, { 
      metadata: { fromSessionId, ...metadata } 
    });
  },

  /**
   * Create session cleanup event
   */
  cleanup(sessionId: UUID, metadata?: Record<string, unknown>): SessionEvent {
    return this.create('cleanup', sessionId, { ...(metadata && { metadata }) });
  }
};

// Session path utilities
export const SessionPaths = {
  /**
   * Generate base session path following organizational structure
   */
  generateBasePath(artifactRoot: string, type: SessionType, owner: string, sessionId: UUID): string {
    switch (type) {
      case 'portal':
        return `${artifactRoot}/portal/${sessionId}`;
      case 'git-hook':
        return `${artifactRoot}/validation/${sessionId}`;
      case 'development':
      case 'test':
        return `${artifactRoot}/user/${owner}/${sessionId}`;
      case 'persona':
        return `${artifactRoot}/personas/${owner}/${sessionId}`;
      default:
        return `${artifactRoot}/misc/${sessionId}`;
    }
  },

  /**
   * Generate session paths structure
   */
  generatePaths(basePath: string): SessionInfo['paths'] {
    return {
      base: basePath,
      logs: `${basePath}/logs`,
      screenshots: `${basePath}/screenshots`,
      files: `${basePath}/files`
    };
  },

  /**
   * Get subdirectories to create
   */
  getSubdirectories(): string[] {
    return ['logs', 'screenshots', 'files', 'recordings', 'devtools'];
  }
};

// Session validation utilities
export const SessionValidation = {
  /**
   * Validate session request
   */
  validateSessionRequest(request: SessionRequest): { valid: boolean; error?: string } {
    if (!request.type || !request.owner) {
      return { valid: false, error: 'type and owner are required' };
    }
    
    if (!['development', 'production', 'test', 'portal', 'git-hook', 'persona'].includes(request.type)) {
      return { valid: false, error: 'invalid session type' };
    }
    
    return { valid: true };
  },

  /**
   * Validate session ID format
   */
  validateSessionId(sessionId: string): { valid: boolean; error?: string } {
    if (!sessionId || typeof sessionId !== 'string') {
      return { valid: false, error: 'sessionId must be a string' };
    }
    
    if (sessionId.length < 10) {
      return { valid: false, error: 'sessionId must be at least 10 characters' };
    }
    
    return { valid: true };
  }
};

// Session utilities
export const SessionUtils = {
  /**
   * Generate session metadata for file storage
   */
  generateMetadata(session: SessionInfo): Record<string, unknown> {
    return {
      sessionId: session.id,
      type: session.type,
      owner: session.owner,
      created: session.created.toISOString(),
      structure: {
        logs: 'Server and client logs',
        screenshots: 'Browser screenshots and visual artifacts',
        files: 'Downloaded files and exports',
        recordings: 'Screen recordings and interactions',
        devtools: 'DevTools dumps and debugging artifacts'
      }
    };
  },

  /**
   * Generate session start log message
   */
  generateStartMessage(session: SessionInfo): string {
    const timestamp = new Date().toISOString();
    return `# Continuum Session Log
# Session: ${session.id}
# Created: ${timestamp}
# Type: ${session.type}
# Owner: ${session.owner}
#
# Session started at ${timestamp}

`;
  },

  /**
   * Check if session needs cleanup
   */
  needsCleanup(session: SessionInfo): boolean {
    if (!session.autoCleanup || !session.isActive) return false;
    
    const age = Date.now() - session.lastActive.getTime();
    return age > session.cleanupAfterMs;
  },

  /**
   * Get session age in minutes
   */
  getAgeInMinutes(session: SessionInfo): number {
    const age = Date.now() - session.lastActive.getTime();
    return Math.round(age / 60000);
  }
};