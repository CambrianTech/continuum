/**
 * Info Command Types
 * 
 * Type definitions for system information and status functionality
 */

export interface SystemInfo {
  name: string;
  version: string;
  nodeVersion: string;
  platform: string;
  arch: string;
  uptime: number;
  memoryUsage: NodeJS.MemoryUsage;
  timestamp: string;
}

export interface InfoResult {
  success: boolean;
  message: string;
  system: SystemInfo;
  continuum: ContinuumInfo;
}

export interface ContinuumInfo {
  version: string;
  sessionId?: string;
  daemons: DaemonInfo[];
  commands: number;
  uptime: number;
}

export interface DaemonInfo {
  name: string;
  status: 'running' | 'stopped' | 'unknown';
  pid?: number;
  uptime?: number;
}

export interface InfoParams {
  verbose?: boolean;
  format?: 'json' | 'text';
  includeSystem?: boolean;
  includeDaemons?: boolean;
}