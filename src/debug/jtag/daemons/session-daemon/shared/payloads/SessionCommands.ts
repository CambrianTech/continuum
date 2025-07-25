/**
 * SessionCommands - JTAG session command parameters and results
 * 
 * Pure session lifecycle management - NO file operations!
 * File storage handled by separate artifacts daemon.
 * 
 * Session Commands:
 * - CreateSession - Create new session context
 * - JoinSession - Join existing session
 * - ForkSession - Fork session from existing one
 * - StopSession - Stop session and cleanup processes
 * - GetSession - Get session information
 * - ListSessions - List sessions with filters
 * - UpdateSession - Update session metadata/config
 * - HealthCheck - Check session health status
 */

import { CommandParams, CommandResult } from '../../../../shared/JTAGTypes';
import type { JTAGContext } from '../../../../shared/JTAGTypes';
import type { 
  SessionInfo, 
  SessionRequest, 
  SessionListFilter, 
  SessionStats,
  SessionForkConfig,
  ConnectionIdentity
} from '../types/SessionTypes';

// ==================== CREATE SESSION COMMAND ====================

export class CreateSessionParams extends CommandParams {
  type: 'development' | 'production' | 'test' | 'portal' | 'git-hook' | 'persona' | 'academy';
  owner: string;
  context?: string;
  source?: string;
  identity?: ConnectionIdentity;
  config?: {
    autoCleanup?: boolean;
    cleanupAfterMs?: number;
    maxIdleTime?: number;
    isolationLevel?: 'strict' | 'moderate' | 'minimal';
  };
  metadata?: {
    description?: string;
    tags?: string[];
    academyId?: string;
    personaId?: string;
    trainingMode?: boolean;
  };

  constructor(data: Partial<CreateSessionParams> & { type: CreateSessionParams['type']; owner: string }) {
    super();
    this.type = data.type;
    this.owner = data.owner;
    this.context = data.context;
    this.source = data.source;
    this.identity = data.identity;
    this.config = data.config;
    this.metadata = data.metadata;
  }
}

export class CreateSessionResult extends CommandResult {
  success: boolean;
  sessionId?: string;
  session?: SessionInfo;
  action: 'created_new' | 'joined_existing';
  error?: string;
  environment: JTAGContext['environment'];
  timestamp: string;

  constructor(data: Partial<CreateSessionResult>) {
    super();
    this.success = data.success ?? false;
    this.sessionId = data.sessionId;
    this.session = data.session;
    this.action = data.action ?? 'created_new';
    this.error = data.error;
    this.environment = data.environment ?? 'server';
    this.timestamp = data.timestamp ?? new Date().toISOString();
  }
}

// ==================== JOIN SESSION COMMAND ====================

export class JoinSessionParams extends CommandParams {
  sessionId: string;
  identity: ConnectionIdentity;
  capabilities?: string[];
  focus?: boolean;

  constructor(data: Partial<JoinSessionParams> & { sessionId: string; identity: ConnectionIdentity }) {
    super();
    this.sessionId = data.sessionId;
    this.identity = data.identity;
    this.capabilities = data.capabilities;
    this.focus = data.focus;
  }
}

export class JoinSessionResult extends CommandResult {
  success: boolean;
  session?: SessionInfo;
  connectionCount?: number;
  error?: string;
  environment: JTAGContext['environment'];
  timestamp: string;

  constructor(data: Partial<JoinSessionResult>) {
    super();
    this.success = data.success ?? false;
    this.session = data.session;
    this.connectionCount = data.connectionCount;
    this.error = data.error;
    this.environment = data.environment ?? 'server';
    this.timestamp = data.timestamp ?? new Date().toISOString();
  }
}

// ==================== FORK SESSION COMMAND ====================

export class ForkSessionParams extends CommandParams {
  fromSessionId: string;
  newOwner?: string;
  forkType: 'copy' | 'reference' | 'lightweight';
  inheritProcesses?: boolean;
  inheritConnections?: boolean;
  metadata?: {
    description?: string;
    tags?: string[];
  };

  constructor(data: Partial<ForkSessionParams> & { fromSessionId: string; forkType: ForkSessionParams['forkType'] }) {
    super();
    this.fromSessionId = data.fromSessionId;
    this.newOwner = data.newOwner;
    this.forkType = data.forkType;
    this.inheritProcesses = data.inheritProcesses ?? false;
    this.inheritConnections = data.inheritConnections ?? false;
    this.metadata = data.metadata;
  }
}

export class ForkSessionResult extends CommandResult {
  success: boolean;
  newSessionId?: string;
  newSession?: SessionInfo;
  parentSession?: SessionInfo;
  error?: string;
  environment: JTAGContext['environment'];
  timestamp: string;

  constructor(data: Partial<ForkSessionResult>) {
    super();
    this.success = data.success ?? false;
    this.newSessionId = data.newSessionId;
    this.newSession = data.newSession;
    this.parentSession = data.parentSession;
    this.error = data.error;
    this.environment = data.environment ?? 'server';
    this.timestamp = data.timestamp ?? new Date().toISOString();
  }
}

// ==================== STOP SESSION COMMAND ====================

export class StopSessionParams extends CommandParams {
  sessionId: string;
  killProcesses?: boolean;
  gracefulShutdown?: boolean;
  timeoutMs?: number;

  constructor(data: Partial<StopSessionParams> & { sessionId: string }) {
    super();
    this.sessionId = data.sessionId;
    this.killProcesses = data.killProcesses ?? true;
    this.gracefulShutdown = data.gracefulShutdown ?? true;
    this.timeoutMs = data.timeoutMs ?? 30000;
  }
}

export class StopSessionResult extends CommandResult {
  success: boolean;
  processesKilled?: number;
  connectionsDropped?: number;
  error?: string;
  environment: JTAGContext['environment'];
  timestamp: string;

  constructor(data: Partial<StopSessionResult>) {
    super();
    this.success = data.success ?? false;
    this.processesKilled = data.processesKilled;
    this.connectionsDropped = data.connectionsDropped;
    this.error = data.error;
    this.environment = data.environment ?? 'server';
    this.timestamp = data.timestamp ?? new Date().toISOString();
  }
}

// ==================== GET SESSION COMMAND ====================

export class GetSessionParams extends CommandParams {
  sessionId: string;
  includeProcesses?: boolean;
  includeConnections?: boolean;

  constructor(data: Partial<GetSessionParams> & { sessionId: string }) {
    super();
    this.sessionId = data.sessionId;
    this.includeProcesses = data.includeProcesses ?? true;
    this.includeConnections = data.includeConnections ?? true;
  }
}

export class GetSessionResult extends CommandResult {
  success: boolean;
  session?: SessionInfo;
  error?: string;
  environment: JTAGContext['environment'];
  timestamp: string;

  constructor(data: Partial<GetSessionResult>) {
    super();
    this.success = data.success ?? false;
    this.session = data.session;
    this.error = data.error;
    this.environment = data.environment ?? 'server';
    this.timestamp = data.timestamp ?? new Date().toISOString();
  }
}

// ==================== LIST SESSIONS COMMAND ====================

export class ListSessionsParams extends CommandParams {
  filter?: SessionListFilter;
  limit?: number;
  includeInactive?: boolean;
  sortBy?: 'created' | 'lastActive' | 'owner' | 'type';
  sortOrder?: 'asc' | 'desc';

  constructor(data: Partial<ListSessionsParams> = {}) {
    super();
    this.filter = data.filter;
    this.limit = data.limit ?? 50;
    this.includeInactive = data.includeInactive ?? false;
    this.sortBy = data.sortBy ?? 'lastActive';
    this.sortOrder = data.sortOrder ?? 'desc';
  }
}

export class ListSessionsResult extends CommandResult {
  success: boolean;
  sessions?: SessionInfo[];
  totalCount?: number;
  filteredCount?: number;
  error?: string;
  environment: JTAGContext['environment'];
  timestamp: string;

  constructor(data: Partial<ListSessionsResult>) {
    super();
    this.success = data.success ?? false;
    this.sessions = data.sessions;
    this.totalCount = data.totalCount;
    this.filteredCount = data.filteredCount;
    this.error = data.error;
    this.environment = data.environment ?? 'server';
    this.timestamp = data.timestamp ?? new Date().toISOString();
  }
}

// ==================== UPDATE SESSION COMMAND ====================

export class UpdateSessionParams extends CommandParams {
  sessionId: string;
  updates: {
    metadata?: Partial<SessionInfo['metadata']>;
    config?: Partial<SessionInfo['config']>;
    description?: string;
    tags?: string[];
  };

  constructor(data: Partial<UpdateSessionParams> & { sessionId: string; updates: UpdateSessionParams['updates'] }) {
    super();
    this.sessionId = data.sessionId;
    this.updates = data.updates;
  }
}

export class UpdateSessionResult extends CommandResult {
  success: boolean;
  session?: SessionInfo;
  changesApplied?: string[];
  error?: string;
  environment: JTAGContext['environment'];
  timestamp: string;

  constructor(data: Partial<UpdateSessionResult>) {
    super();
    this.success = data.success ?? false;
    this.session = data.session;
    this.changesApplied = data.changesApplied;
    this.error = data.error;
    this.environment = data.environment ?? 'server';
    this.timestamp = data.timestamp ?? new Date().toISOString();
  }
}

// ==================== HEALTH CHECK COMMAND ====================

export class HealthCheckParams extends CommandParams {
  sessionId?: string; // If not provided, checks all sessions
  detailed?: boolean;
  includeProcesses?: boolean;

  constructor(data: Partial<HealthCheckParams> = {}) {
    super();
    this.sessionId = data.sessionId;
    this.detailed = data.detailed ?? false;
    this.includeProcesses = data.includeProcesses ?? true;
  }
}

export class HealthCheckResult extends CommandResult {
  success: boolean;
  sessionHealth?: {
    sessionId: string;
    health: 'healthy' | 'warning' | 'error';
    issues: string[];
    processCount: number;
    connectionCount: number;
    lastActive: number;
  }[];
  overallHealth?: 'healthy' | 'warning' | 'error';
  stats?: SessionStats;
  error?: string;
  environment: JTAGContext['environment'];
  timestamp: string;

  constructor(data: Partial<HealthCheckResult>) {
    super();
    this.success = data.success ?? false;
    this.sessionHealth = data.sessionHealth;
    this.overallHealth = data.overallHealth;
    this.stats = data.stats;
    this.error = data.error;
    this.environment = data.environment ?? 'server';
    this.timestamp = data.timestamp ?? new Date().toISOString();
  }
}

// ==================== PROCESS MANAGEMENT COMMANDS ====================

export class ManageProcessParams extends CommandParams {
  sessionId: string;
  action: 'list' | 'kill' | 'kill_all' | 'status';
  pid?: number;
  processType?: 'browser' | 'devtools' | 'server' | 'worker' | 'custom';

  constructor(data: Partial<ManageProcessParams> & { sessionId: string; action: ManageProcessParams['action'] }) {
    super();
    this.sessionId = data.sessionId;
    this.action = data.action;
    this.pid = data.pid;
    this.processType = data.processType;
  }
}

export class ManageProcessResult extends CommandResult {
  success: boolean;
  processes?: Array<{
    pid: number;
    type: string;
    command: string;
    startTime: number;
    isAlive: boolean;
    memoryUsage?: number;
    cpuUsage?: number;
  }>;
  processesKilled?: number;
  error?: string;
  environment: JTAGContext['environment'];
  timestamp: string;

  constructor(data: Partial<ManageProcessResult>) {
    super();
    this.success = data.success ?? false;
    this.processes = data.processes;
    this.processesKilled = data.processesKilled;
    this.error = data.error;
    this.environment = data.environment ?? 'server';
    this.timestamp = data.timestamp ?? new Date().toISOString();
  }
}

// ==================== COMMAND UNION TYPES ====================

export type SessionCommandParams = 
  | CreateSessionParams
  | JoinSessionParams
  | ForkSessionParams
  | StopSessionParams
  | GetSessionParams
  | ListSessionsParams
  | UpdateSessionParams
  | HealthCheckParams
  | ManageProcessParams;

export type SessionCommandResult = 
  | CreateSessionResult
  | JoinSessionResult
  | ForkSessionResult
  | StopSessionResult
  | GetSessionResult
  | ListSessionsResult
  | UpdateSessionResult
  | HealthCheckResult
  | ManageProcessResult;

// ==================== COMMAND FACTORY FUNCTIONS ====================

/**
 * Create session creation command
 */
export function createSessionCommand(
  type: CreateSessionParams['type'],
  owner: string,
  options?: Partial<CreateSessionParams>
): CreateSessionParams {
  return new CreateSessionParams({ type, owner, ...options });
}

/**
 * Create session join command
 */
export function joinSessionCommand(
  sessionId: string,
  identity: ConnectionIdentity,
  options?: Partial<JoinSessionParams>
): JoinSessionParams {
  return new JoinSessionParams({ sessionId, identity, ...options });
}

/**
 * Create session fork command
 */
export function forkSessionCommand(
  fromSessionId: string,
  forkType: ForkSessionParams['forkType'],
  options?: Partial<ForkSessionParams>
): ForkSessionParams {
  return new ForkSessionParams({ fromSessionId, forkType, ...options });
}