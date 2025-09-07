/**
 * System Commands API - Public Interface Types
 * 
 * Consumer-first API for system operations and health monitoring.
 */

// System health and status
export interface SystemHealthParams {
  includeMetrics?: boolean;
  includeServices?: boolean;
  includeConnections?: boolean;
}

export interface SystemHealthMetrics {
  cpu: {
    usage: number; // percentage
    cores: number;
  };
  memory: {
    used: number; // bytes
    total: number; // bytes
    percentage: number;
  };
  uptime: number; // seconds
  loadAverage?: number[];
}

export interface ServiceStatus {
  name: string;
  status: 'running' | 'stopped' | 'error' | 'unknown';
  pid?: number;
  startTime?: string;
  memoryUsage?: number;
  cpuUsage?: number;
  error?: string;
}

export interface ConnectionStatus {
  type: 'websocket' | 'http' | 'database' | 'external';
  name: string;
  status: 'connected' | 'disconnected' | 'error';
  url?: string;
  lastConnected?: string;
  error?: string;
}

export interface SystemHealthResult {
  success: boolean;
  status: 'healthy' | 'degraded' | 'unhealthy';
  metrics?: SystemHealthMetrics;
  services?: ServiceStatus[];
  connections?: ConnectionStatus[];
  timestamp: string;
  error?: string;
}

// System configuration
export interface GetConfigParams {
  section?: string;
  includeSecrets?: boolean;
}

export interface SystemConfig {
  [section: string]: {
    [key: string]: any;
  };
}

export interface GetConfigResult {
  success: boolean;
  config?: SystemConfig;
  section?: string;
  error?: string;
}

export interface SetConfigParams {
  section: string;
  key: string;
  value: any;
  persist?: boolean;
}

export interface SetConfigResult {
  success: boolean;
  previousValue?: any;
  error?: string;
}

// Process and service management
export interface ListProcessesParams {
  filter?: string; // name pattern
  includeSystem?: boolean;
  sortBy?: 'name' | 'pid' | 'cpu' | 'memory' | 'startTime';
}

export interface ProcessInfo {
  pid: number;
  name: string;
  command: string;
  startTime: string;
  cpuUsage: number;
  memoryUsage: number;
  status: 'running' | 'sleeping' | 'stopped' | 'zombie';
  parentPid?: number;
}

export interface ListProcessesResult {
  success: boolean;
  processes: ProcessInfo[];
  totalCount: number;
  error?: string;
}

export interface StartServiceParams {
  serviceName: string;
  config?: Record<string, any>;
  waitForReady?: boolean;
}

export interface StartServiceResult {
  success: boolean;
  service?: ServiceStatus;
  error?: string;
}

export interface StopServiceParams {
  serviceName: string;
  graceful?: boolean;
  timeout?: number; // seconds
}

export interface StopServiceResult {
  success: boolean;
  stopped: boolean;
  error?: string;
}

// System commands and execution
export interface ExecuteCommandParams {
  command: string;
  args?: string[];
  cwd?: string;
  env?: Record<string, string>;
  timeout?: number; // seconds
  captureOutput?: boolean;
  shell?: boolean;
}

export interface ExecuteCommandResult {
  success: boolean;
  exitCode?: number;
  stdout?: string;
  stderr?: string;
  duration?: number; // milliseconds
  timedOut?: boolean;
  error?: string;
}

// Log management
export interface GetLogsParams {
  service?: string;
  level?: 'error' | 'warn' | 'info' | 'debug' | 'trace';
  since?: string; // ISO date or relative like '1h'
  limit?: number;
  follow?: boolean; // streaming
  search?: string; // pattern to search for
}

export interface LogEntry {
  timestamp: string;
  level: 'error' | 'warn' | 'info' | 'debug' | 'trace';
  service: string;
  message: string;
  metadata?: Record<string, any>;
}

export interface GetLogsResult {
  success: boolean;
  logs: LogEntry[];
  totalCount?: number;
  hasMore: boolean;
  streamId?: string; // For follow mode
  error?: string;
}

// System events and monitoring
export interface SystemEvent {
  type: 'service_started' | 'service_stopped' | 'service_error' | 'config_changed' | 'health_alert' | 'connection_lost' | 'connection_restored';
  service?: string;
  timestamp: string;
  data: any;
  severity: 'low' | 'medium' | 'high' | 'critical';
}

export interface SubscribeToSystemEventsParams {
  eventTypes?: SystemEvent['type'][];
  services?: string[];
  minSeverity?: SystemEvent['severity'];
}

export interface SubscribeToSystemEventsResult {
  success: boolean;
  subscriptionId?: string;
  error?: string;
}

// Backup and restore
export interface CreateBackupParams {
  type: 'config' | 'data' | 'logs' | 'full';
  destination?: string;
  compression?: 'none' | 'gzip' | 'bzip2';
  includeSecrets?: boolean;
}

export interface BackupInfo {
  id: string;
  type: string;
  created: string;
  size: number;
  filepath: string;
  checksum: string;
}

export interface CreateBackupResult {
  success: boolean;
  backup?: BackupInfo;
  error?: string;
}

export interface RestoreBackupParams {
  backupId: string;
  restoreLocation?: string;
  overwriteExisting?: boolean;
}

export interface RestoreBackupResult {
  success: boolean;
  restored: boolean;
  restoredFiles?: string[];
  error?: string;
}

// Export all system command types
export type SystemCommandParams = 
  | SystemHealthParams
  | GetConfigParams
  | SetConfigParams
  | ListProcessesParams
  | StartServiceParams
  | StopServiceParams
  | ExecuteCommandParams
  | GetLogsParams
  | SubscribeToSystemEventsParams
  | CreateBackupParams
  | RestoreBackupParams;

export type SystemCommandResult = 
  | SystemHealthResult
  | GetConfigResult
  | SetConfigResult
  | ListProcessesResult
  | StartServiceResult
  | StopServiceResult
  | ExecuteCommandResult
  | GetLogsResult
  | SubscribeToSystemEventsResult
  | CreateBackupResult
  | RestoreBackupResult;