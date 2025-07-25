/**
 * SessionTypes - Core session type definitions for JTAG session daemon
 * 
 * Migrated from legacy /src/daemons/session-manager/shared/SessionManagerTypes.ts
 * Preserves excellent design patterns while adapting to JTAG architecture.
 * 
 * Key concepts:
 * - Session isolation and lifecycle management
 * - Multi-context session access (browser/server)
 * - Process tracking and cleanup automation
 * - Identity-based session organization
 */

// ==================== CORE SESSION TYPES ====================

/**
 * Session types for different use cases
 */
export type SessionType = 
  | 'development'    // Developer sessions
  | 'production'     // Production usage
  | 'test'          // Testing scenarios
  | 'portal'        // Portal/public access
  | 'git-hook'      // Git validation
  | 'persona'       // AI persona training
  | 'academy';      // Academy training contexts

/**
 * Connection identity for session access control
 */
export interface ConnectionIdentity {
  type: 'portal' | 'validation' | 'user' | 'persona' | 'system';
  name: string;           // portal, git-hook, username, persona-name
  sessionContext?: string; // additional context like branch name for git hooks
  capabilities?: string[]; // what this connection can do
}

/**
 * Session information - core session data
 */
export interface SessionInfo {
  id: string;
  type: SessionType;
  owner: string;
  created: number;        // timestamp
  lastActive: number;     // timestamp
  isActive: boolean;
  
  // Process tracking - what's running in this session
  processes: {
    browserPid?: number;
    devtoolsPid?: number;
    serverPid?: number;
    additionalPids?: number[];
  };
  
  // Session configuration
  config: {
    autoCleanup: boolean;
    cleanupAfterMs: number;
    maxIdleTime: number;
    isolationLevel: 'strict' | 'moderate' | 'minimal';
  };
  
  // Connection tracking
  connections: {
    active: number;
    identities: ConnectionIdentity[];
    lastConnectionAt?: number;
  };
  
  // Session state
  state: {
    phase: 'initializing' | 'active' | 'idle' | 'cleanup' | 'stopped';
    health: 'healthy' | 'warning' | 'error';
    lastHealthCheck?: number;
  };
  
  // Session metadata
  metadata: {
    description?: string;
    tags?: string[];
    source?: string;        // where this session was created from
    context?: string;       // additional context data
    // Academy-specific metadata
    academyId?: string;
    personaId?: string;
    trainingMode?: boolean;
    // P2P metadata
    nodeId?: string;
    clusterId?: string;
  };
}

/**
 * Session request - how sessions are created/joined
 */
export interface SessionRequest {
  type: SessionType;
  owner: string;
  preference?: 'current' | 'new' | string; // specific session ID or 'fork:sessionId'
  context?: string;
  source?: string;
  capabilities?: string[];
  focus?: boolean;
  killZombies?: boolean;
  
  // Session configuration overrides
  config?: Partial<SessionInfo['config']>;
  
  // Connection identity
  identity?: ConnectionIdentity;
  
  // Academy-specific request data
  academyContext?: {
    academyId: string;
    personaId: string;
    trainingMode: boolean;
    sessionScope: 'private' | 'shared' | 'collaborative';
  };
}

/**
 * Session response - result of session creation/join
 */
export interface SessionResponse {
  sessionId: string;
  action: 'joined_existing' | 'created_new' | 'forked_from';
  session: SessionInfo;
  interface: string;       // how to connect to this session
  version: string;
  
  // What was launched/activated
  launched: {
    browser: boolean;
    webserver: boolean;
    newLogFiles: boolean;
    devtools: boolean;
  };
  
  // Available commands for this session
  commands: {
    stop: string;
    fork: string;
    info: string;
    health: string;
    cleanup: string;
  };
  
  // Connection information
  connection: {
    websocketUrl?: string;
    httpUrl?: string;
    devtoolsUrl?: string;
  };
}

// ==================== SESSION LIFECYCLE TYPES ====================

/**
 * Session lifecycle phases
 */
export type SessionPhase = 
  | 'initializing'  // Setting up processes and resources
  | 'active'        // Normal operation
  | 'idle'          // No recent activity but still alive
  | 'cleanup'       // Cleaning up resources
  | 'stopped';      // Fully stopped

/**
 * Session health status
 */
export type SessionHealth = 
  | 'healthy'       // Everything working normally
  | 'warning'       // Some issues but functional
  | 'error';        // Serious problems

/**
 * Session creation configuration
 */
export interface SessionCreationConfig {
  type: SessionType;
  owner: string;
  autoCleanup?: boolean;
  cleanupAfterMs?: number;
  maxIdleTime?: number;
  isolationLevel?: 'strict' | 'moderate' | 'minimal';
  context?: string;
  description?: string;
  tags?: string[];
}

/**
 * Session fork configuration
 */
export interface SessionForkConfig {
  fromSessionId: string;
  newOwner?: string;
  inheritProcesses?: boolean;
  inheritConnections?: boolean;
  forkType: 'copy' | 'reference' | 'lightweight';
}

// ==================== FILTERING AND QUERIES ====================

/**
 * Session list filter
 */
export interface SessionListFilter {
  type?: SessionType;
  owner?: string;
  active?: boolean;
  phase?: SessionPhase;
  health?: SessionHealth;
  hasProcesses?: boolean;
  createdAfter?: number;
  createdBefore?: number;
  tags?: string[];
  academyId?: string;
  personaId?: string;
}

/**
 * Session statistics
 */
export interface SessionStats {
  total: number;
  active: number;
  inactive: number;
  byType: Record<SessionType, number>;
  byPhase: Record<SessionPhase, number>;
  byHealth: Record<SessionHealth, number>;
  totalProcesses: number;
  totalConnections: number;
  averageAge: number;        // in milliseconds
  oldestSession?: {
    id: string;
    age: number;
  };
}

// ==================== PROCESS MANAGEMENT ====================

/**
 * Process information within a session
 */
export interface SessionProcess {
  pid: number;
  type: 'browser' | 'devtools' | 'server' | 'worker' | 'custom';
  command: string;
  args: string[];
  cwd: string;
  startTime: number;
  isAlive: boolean;
  memoryUsage?: number;
  cpuUsage?: number;
}

/**
 * Process management operations
 */
export interface ProcessManagementOps {
  listProcesses(sessionId: string): Promise<SessionProcess[]>;
  killProcess(sessionId: string, pid: number): Promise<boolean>;
  killAllProcesses(sessionId: string): Promise<number>; // returns count killed
  getProcessStatus(sessionId: string, pid: number): Promise<SessionProcess | null>;
}

// ==================== VALIDATION AND UTILITIES ====================

/**
 * Session validation result
 */
export interface SessionValidationResult {
  valid: boolean;
  errors: string[];
  warnings?: string[];
}

// ==================== CONSTANTS ====================

export const SESSION_CONSTANTS = {
  // Default configuration
  DEFAULT_CLEANUP_TIME: 3600000,        // 1 hour
  DEFAULT_MAX_IDLE_TIME: 1800000,       // 30 minutes
  DEFAULT_ISOLATION_LEVEL: 'moderate' as const,
  
  // Limits
  MAX_SESSIONS_PER_OWNER: 10,
  MAX_CONNECTIONS_PER_SESSION: 20,
  MAX_PROCESSES_PER_SESSION: 10,
  
  // Health check intervals
  HEALTH_CHECK_INTERVAL: 60000,         // 1 minute
  PROCESS_CHECK_INTERVAL: 30000,        // 30 seconds
  CLEANUP_CHECK_INTERVAL: 300000,       // 5 minutes
  
  // Session ID format
  SESSION_ID_LENGTH: 16,
  SESSION_ID_PREFIX: 'sess_',
  
  // Reserved session owners
  RESERVED_OWNERS: ['system', 'portal', 'validation', 'academy'],
  
  // Session timeouts
  INITIALIZATION_TIMEOUT: 60000,        // 1 minute to initialize
  CLEANUP_TIMEOUT: 30000,               // 30 seconds to cleanup
  PROCESS_KILL_TIMEOUT: 10000,          // 10 seconds to kill process
  
  // Path constants (for artifacts daemon coordination)
  DEFAULT_SESSION_ROOT: '.continuum/sessions',
  SESSION_INFO_FILE: 'session.json',
  HEALTH_LOG_FILE: 'health.log',
  
  // Event constants
  EVENT_TTL: 3600000,                   // 1 hour
  MAX_EVENTS_PER_SESSION: 1000,
  
  // Academy integration
  ACADEMY_SESSION_PREFIX: 'academy_',
  PERSONA_SESSION_PREFIX: 'persona_',
  TRAINING_SESSION_TIMEOUT: 7200000     // 2 hours for training sessions
} as const;

// ==================== UTILITY FUNCTIONS ====================

/**
 * Session validation utilities
 */
export const SessionValidation = {
  /**
   * Validate session request
   */
  validateSessionRequest(request: SessionRequest): SessionValidationResult {
    const errors: string[] = [];
    
    if (!request.type || !request.owner) {
      errors.push('type and owner are required');
    }
    
    if (!['development', 'production', 'test', 'portal', 'git-hook', 'persona', 'academy'].includes(request.type)) {
      errors.push('invalid session type');
    }
    
    if (request.owner && SESSION_CONSTANTS.RESERVED_OWNERS.includes(request.owner as any)) {
      errors.push(`'${request.owner}' is a reserved owner name`);
    }
    
    return {
      valid: errors.length === 0,
      errors
    };
  },

  /**
   * Validate session ID format
   */
  validateSessionId(sessionId: string): SessionValidationResult {
    const errors: string[] = [];
    
    if (!sessionId || typeof sessionId !== 'string') {
      errors.push('sessionId must be a string');
    } else if (sessionId.length < 10) {
      errors.push('sessionId must be at least 10 characters');
    }
    
    return {
      valid: errors.length === 0,
      errors
    };
  }
};

/**
 * Session utility functions
 */
export const SessionUtils = {
  /**
   * Generate unique session ID
   */
  generateSessionId(type: SessionType): string {
    const timestamp = Date.now().toString(36);
    const random = Math.random().toString(36).substr(2, 8);
    const prefix = type === 'academy' ? SESSION_CONSTANTS.ACADEMY_SESSION_PREFIX : 
                  type === 'persona' ? SESSION_CONSTANTS.PERSONA_SESSION_PREFIX : 
                  SESSION_CONSTANTS.SESSION_ID_PREFIX;
    return `${prefix}${timestamp}_${random}`;
  },

  /**
   * Check if session needs cleanup
   */
  needsCleanup(session: SessionInfo): boolean {
    if (!session.config.autoCleanup || !session.isActive) return false;
    
    const age = Date.now() - session.lastActive;
    return age > session.config.cleanupAfterMs;
  },

  /**
   * Check if session is idle
   */
  isIdle(session: SessionInfo): boolean {
    const age = Date.now() - session.lastActive;
    return age > session.config.maxIdleTime;
  },

  /**
   * Get session age in milliseconds
   */
  getAge(session: SessionInfo): number {
    return Date.now() - session.created;
  },

  /**
   * Get session idle time in milliseconds
   */
  getIdleTime(session: SessionInfo): number {
    return Date.now() - session.lastActive;
  },

  /**
   * Format session age for display
   */
  formatAge(session: SessionInfo): string {
    const age = SessionUtils.getAge(session);
    const minutes = Math.floor(age / 60000);
    const hours = Math.floor(minutes / 60);
    const days = Math.floor(hours / 24);
    
    if (days > 0) return `${days}d ${hours % 24}h`;
    if (hours > 0) return `${hours}h ${minutes % 60}m`;
    return `${minutes}m`;
  },

  /**
   * Check if session is Academy-related
   */
  isAcademySession(session: SessionInfo): boolean {
    return session.type === 'academy' || 
           session.type === 'persona' || 
           !!session.metadata.academyId;
  },

  /**
   * Get session display name
   */
  getDisplayName(session: SessionInfo): string {
    if (session.metadata.description) {
      return session.metadata.description;
    }
    
    return `${session.type}:${session.owner}:${session.id.slice(-8)}`;
  },

  /**
   * Create default session configuration
   */
  createDefaultConfig(): SessionInfo['config'] {
    return {
      autoCleanup: true,
      cleanupAfterMs: SESSION_CONSTANTS.DEFAULT_CLEANUP_TIME,
      maxIdleTime: SESSION_CONSTANTS.DEFAULT_MAX_IDLE_TIME,
      isolationLevel: SESSION_CONSTANTS.DEFAULT_ISOLATION_LEVEL
    };
  }
};

/**
 * Session path utilities (for coordination with artifacts daemon)
 */
export const SessionPaths = {
  /**
   * Generate base session path following organizational structure
   */
  generateBasePath(artifactRoot: string, type: SessionType, owner: string, sessionId: string): string {
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
      case 'academy':
        return `${artifactRoot}/academy/${owner}/${sessionId}`;
      default:
        return `${artifactRoot}/misc/${sessionId}`;
    }
  },

  /**
   * Get expected subdirectories for session artifacts
   */
  getExpectedSubdirectories(): string[] {
    return ['logs', 'screenshots', 'files', 'recordings', 'devtools', 'training'];
  }
};