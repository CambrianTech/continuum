/**
 * Session Manager Types - Shared definitions for session management
 * 
 * This module contains all type definitions for session management
 * following the middle-out modular pattern.
 */

import { PersonaGenome } from '../../../shared/AcademyTypes';

// ==================== SESSION TYPES ====================

/**
 * Session configuration
 */
export interface SessionConfig {
  sessionType: SessionType;
  participants: PersonaGenome[];
  isolation: IsolationLevel;
  timeLimit?: number;
  resources?: string[];
  environment?: EnvironmentConfig;
  monitoring?: MonitoringConfig;
}

/**
 * Session types
 */
export type SessionType = 
  | 'individual'     // Single persona training
  | 'collaborative'  // Multiple personas working together
  | 'competitive'    // Personas competing against each other
  | 'tournament'     // Tournament-style competition
  | 'evaluation'     // Assessment and testing
  | 'sandbox'        // Isolated experimentation
  | 'evolution';     // Evolution-specific session

/**
 * Isolation levels
 */
export type IsolationLevel = 
  | 'none'           // No isolation
  | 'basic'          // Basic resource isolation
  | 'standard'       // Standard sandboxing
  | 'strict'         // Strict isolation
  | 'complete';      // Complete isolation

/**
 * Environment configuration
 */
export interface EnvironmentConfig {
  sandboxType: 'process' | 'container' | 'vm' | 'iframe';
  resourceLimits: ResourceLimits;
  networkAccess: boolean;
  fileSystemAccess: boolean;
  allowedDomains?: string[];
  environmentVariables?: Record<string, string>;
}

/**
 * Resource limits
 */
export interface ResourceLimits {
  maxMemory: number;      // in MB
  maxCpuTime: number;     // in seconds
  maxDiskSpace: number;   // in MB
  maxNetworkRequests: number;
  maxFileOperations: number;
}

/**
 * Monitoring configuration
 */
export interface MonitoringConfig {
  logLevel: 'debug' | 'info' | 'warn' | 'error';
  captureOutput: boolean;
  trackPerformance: boolean;
  recordInteractions: boolean;
  alertThresholds: AlertThresholds;
}

/**
 * Alert thresholds
 */
export interface AlertThresholds {
  memoryUsage: number;    // percentage
  cpuUsage: number;       // percentage
  responseTime: number;   // in ms
  errorRate: number;      // percentage
}

// ==================== SESSION LIFECYCLE ====================

/**
 * Session information
 */
export interface SessionInfo {
  id: string;
  config: SessionConfig;
  status: SessionStatus;
  created: Date;
  started?: Date;
  ended?: Date;
  duration?: number;
  
  // Runtime information
  processId?: number;
  containerid?: string;
  sandboxPath?: string;
  
  // Metrics
  resourceUsage: ResourceUsage;
  performance: SessionPerformance;
  logs: SessionLog[];
  
  // Results
  results: SessionResult[];
  errors: SessionError[];
}

/**
 * Session status
 */
export type SessionStatus = 
  | 'created'        // Session created but not started
  | 'starting'       // Session is starting up
  | 'running'        // Session is actively running
  | 'paused'         // Session is paused
  | 'stopping'       // Session is stopping
  | 'stopped'        // Session has stopped normally
  | 'error'          // Session encountered an error
  | 'timeout'        // Session timed out
  | 'killed';        // Session was forcefully terminated

/**
 * Resource usage tracking
 */
export interface ResourceUsage {
  memoryUsage: number;    // current memory usage in MB
  peakMemoryUsage: number; // peak memory usage in MB
  cpuUsage: number;       // current CPU usage percentage
  diskUsage: number;      // disk usage in MB
  networkRequests: number; // number of network requests
  fileOperations: number;  // number of file operations
}

/**
 * Session performance metrics
 */
export interface SessionPerformance {
  startupTime: number;    // time to start in ms
  responseTime: number;   // average response time in ms
  throughput: number;     // operations per second
  errorRate: number;      // error percentage
  successRate: number;    // success percentage
}

/**
 * Session log entry
 */
export interface SessionLog {
  timestamp: Date;
  level: 'debug' | 'info' | 'warn' | 'error';
  source: string;
  message: string;
  data?: any;
}

/**
 * Session result
 */
export interface SessionResult {
  id: string;
  personaId: string;
  type: 'output' | 'error' | 'metric' | 'event';
  timestamp: Date;
  data: any;
  metadata?: Record<string, any>;
}

/**
 * Session error
 */
export interface SessionError {
  id: string;
  timestamp: Date;
  severity: 'low' | 'medium' | 'high' | 'critical';
  type: string;
  message: string;
  stack?: string;
  context?: Record<string, any>;
}

// ==================== SESSION REQUESTS/RESPONSES ====================

/**
 * Create session request
 */
export interface CreateSessionRequest {
  config: SessionConfig;
  metadata?: Record<string, any>;
}

/**
 * Create session response
 */
export interface CreateSessionResponse {
  success: boolean;
  sessionId?: string;
  sessionInfo?: SessionInfo;
  error?: string;
}

/**
 * Start session request
 */
export interface StartSessionRequest {
  sessionId: string;
  options?: StartSessionOptions;
}

/**
 * Start session options
 */
export interface StartSessionOptions {
  detached?: boolean;
  waitForReady?: boolean;
  timeoutMs?: number;
}

/**
 * Start session response
 */
export interface StartSessionResponse {
  success: boolean;
  sessionId?: string;
  processId?: number;
  error?: string;
}

/**
 * Stop session request
 */
export interface StopSessionRequest {
  sessionId: string;
  force?: boolean;
  cleanupResources?: boolean;
}

/**
 * Stop session response
 */
export interface StopSessionResponse {
  success: boolean;
  sessionId?: string;
  stopped?: boolean;
  error?: string;
}

/**
 * Get session info request
 */
export interface GetSessionInfoRequest {
  sessionId: string;
  includeMetrics?: boolean;
  includeLogs?: boolean;
}

/**
 * Get session info response
 */
export interface GetSessionInfoResponse {
  success: boolean;
  sessionInfo?: SessionInfo;
  error?: string;
}

/**
 * Execute in session request
 */
export interface ExecuteInSessionRequest {
  sessionId: string;
  personaId: string;
  action: string;
  parameters?: any;
  timeout?: number;
}

/**
 * Execute in session response
 */
export interface ExecuteInSessionResponse {
  success: boolean;
  result?: any;
  output?: string;
  error?: string;
  performance?: {
    executionTime: number;
    memoryUsed: number;
    cpuTime: number;
  };
}

/**
 * List sessions request
 */
export interface ListSessionsRequest {
  status?: SessionStatus;
  sessionType?: SessionType;
  participantId?: string;
  createdAfter?: Date;
  createdBefore?: Date;
  limit?: number;
  offset?: number;
}

/**
 * List sessions response
 */
export interface ListSessionsResponse {
  success: boolean;
  sessions?: SessionInfo[];
  totalCount?: number;
  error?: string;
}

// ==================== CONSTANTS ====================

/**
 * Default session configuration
 */
export const DEFAULT_SESSION_CONFIG: SessionConfig = {
  sessionType: 'individual',
  participants: [],
  isolation: 'standard',
  timeLimit: 3600000, // 1 hour
  resources: ['cpu', 'memory', 'disk'],
  environment: {
    sandboxType: 'process',
    resourceLimits: {
      maxMemory: 512,         // 512 MB
      maxCpuTime: 300,        // 5 minutes
      maxDiskSpace: 100,      // 100 MB
      maxNetworkRequests: 100,
      maxFileOperations: 1000
    },
    networkAccess: false,
    fileSystemAccess: true
  },
  monitoring: {
    logLevel: 'info',
    captureOutput: true,
    trackPerformance: true,
    recordInteractions: true,
    alertThresholds: {
      memoryUsage: 80,      // 80%
      cpuUsage: 90,         // 90%
      responseTime: 5000,   // 5 seconds
      errorRate: 10         // 10%
    }
  }
};

/**
 * Default resource limits
 */
export const DEFAULT_RESOURCE_LIMITS: ResourceLimits = {
  maxMemory: 512,
  maxCpuTime: 300,
  maxDiskSpace: 100,
  maxNetworkRequests: 100,
  maxFileOperations: 1000
};

/**
 * Session timeout defaults
 */
export const SESSION_TIMEOUTS = {
  startup: 30000,      // 30 seconds
  execution: 300000,   // 5 minutes
  shutdown: 10000,     // 10 seconds
  cleanup: 5000        // 5 seconds
};

// ==================== VALIDATION ====================

/**
 * Validate session configuration
 */
export function validateSessionConfig(config: SessionConfig): {
  valid: boolean;
  errors: string[];
} {
  const errors: string[] = [];

  if (!config.sessionType) {
    errors.push('Session type is required');
  }

  if (!config.participants || config.participants.length === 0) {
    errors.push('At least one participant is required');
  }

  if (!config.isolation) {
    errors.push('Isolation level is required');
  }

  if (config.timeLimit && config.timeLimit <= 0) {
    errors.push('Time limit must be positive');
  }

  if (config.environment) {
    const env = config.environment;
    
    if (env.resourceLimits) {
      const limits = env.resourceLimits;
      
      if (limits.maxMemory <= 0) errors.push('Max memory must be positive');
      if (limits.maxCpuTime <= 0) errors.push('Max CPU time must be positive');
      if (limits.maxDiskSpace <= 0) errors.push('Max disk space must be positive');
      if (limits.maxNetworkRequests < 0) errors.push('Max network requests cannot be negative');
      if (limits.maxFileOperations < 0) errors.push('Max file operations cannot be negative');
    }
  }

  return {
    valid: errors.length === 0,
    errors
  };
}

/**
 * Validate create session request
 */
export function validateCreateSessionRequest(request: CreateSessionRequest): {
  valid: boolean;
  errors: string[];
} {
  const errors: string[] = [];

  if (!request.config) {
    errors.push('Session config is required');
  } else {
    const configValidation = validateSessionConfig(request.config);
    if (!configValidation.valid) {
      errors.push(...configValidation.errors);
    }
  }

  return {
    valid: errors.length === 0,
    errors
  };
}

// ==================== EXPORTS ====================

// Note: Types are exported inline above, no need to re-export
// export {
//   SessionConfig,
//   SessionType,
//   IsolationLevel,
//   EnvironmentConfig,
//   ResourceLimits,
//   MonitoringConfig,
//   AlertThresholds,
//   SessionInfo,
//   SessionStatus,
//   ResourceUsage,
//   SessionPerformance,
//   SessionLog,
//   SessionResult,
//   SessionError,
//   CreateSessionRequest,
//   CreateSessionResponse,
//   StartSessionRequest,
//   StartSessionOptions,
//   StartSessionResponse,
//   StopSessionRequest,
//   StopSessionResponse,
//   GetSessionInfoRequest,
//   GetSessionInfoResponse,
//   ExecuteInSessionRequest,
//   ExecuteInSessionResponse,
//   ListSessionsRequest,
//   ListSessionsResponse
// };