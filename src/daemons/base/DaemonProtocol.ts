/**
 * Daemon Protocol - Standard message format for Continuum OS daemon communication
 * Defines the IPC protocol between daemons and the OS
 */

export interface DaemonMessage {
  id: string;
  from: string;
  to: string;
  type: string;
  data: any;
  timestamp: Date;
  priority?: 'low' | 'normal' | 'high' | 'critical';
  correlationId?: string;
}

export interface DaemonResponse {
  success: boolean;
  data?: any;
  error?: string;
  messageId?: string;
  processingTime?: number;
}

export enum DaemonStatus {
  STOPPED = 'stopped',
  STARTING = 'starting', 
  RUNNING = 'running',
  STOPPING = 'stopping',
  FAILED = 'failed'
}

// Keep type for backward compatibility
export type DaemonStatusType = DaemonStatus;

export interface DaemonCapability {
  name: string;
  version: string;
  description: string;
  endpoints: string[];
  dependencies: string[];
}

export interface DaemonRegistration {
  daemon: string;
  version: string;
  capabilities: DaemonCapability[];
  endpoints: string[];
  pid: number;
  startTime: Date;
}

export interface DaemonHeartbeat {
  daemon: string;
  timestamp: Date;
  status: DaemonStatus;
  metrics: DaemonMetrics;
}

export interface DaemonMetrics {
  memoryUsage: number;
  cpuUsage: number;
  requestCount: number;
  errorCount: number;
  averageResponseTime: number;
  uptime: number;
}