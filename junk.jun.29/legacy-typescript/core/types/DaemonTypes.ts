/**
 * Core Daemon Types - Strongly typed TypeScript interfaces
 * Foundation types for the new Continuum OS architecture
 */

export type DaemonStatus = 'stopped' | 'starting' | 'running' | 'stopping' | 'crashed' | 'unhealthy';

export interface DaemonMessage<T = any> {
  readonly id: string;
  readonly from: string;
  readonly to: string;
  readonly type: string;
  readonly data: T;
  readonly timestamp: Date;
  readonly priority?: 'low' | 'normal' | 'high' | 'critical';
  readonly timeout?: number;
  readonly retryCount?: number;
  readonly traceId?: string;
}

export interface DaemonResponse<T = any> {
  readonly success: boolean;
  readonly data?: T;
  readonly error?: string;
  readonly timestamp: Date;
  readonly processingTime?: number;
  readonly warnings?: readonly string[];
}

export interface DaemonConfig {
  readonly name: string;
  readonly version: string;
  readonly port?: number;
  readonly autoStart: boolean;
  readonly dependencies: readonly string[];
  readonly healthCheck: {
    readonly interval: number;
    readonly timeout: number;
    readonly retries: number;
  };
  readonly resources: {
    readonly maxMemory: number;
    readonly maxCpu: number;
  };
}

export interface DaemonInfo {
  readonly config: DaemonConfig;
  readonly status: DaemonStatus;
  readonly pid?: number;
  readonly startTime?: Date;
  readonly uptime: number;
  readonly memoryUsage?: NodeJS.MemoryUsage;
  readonly cpuUsage?: NodeJS.CpuUsage;
}

export interface CommandRequest<T = any> {
  readonly command: string;
  readonly parameters: T;
  readonly context: ExecutionContext;
  readonly preferences: ExecutionPreferences;
}

export interface ExecutionContext {
  readonly sessionId: string;
  readonly userId?: string;
  readonly source: 'browser' | 'portal' | 'api' | 'mesh';
  readonly priority: 'low' | 'normal' | 'high' | 'critical';
  readonly traceId: string;
}

export interface ExecutionPreferences {
  readonly timeout: number;
  readonly retries: number;
  readonly provider?: 'auto' | 'local' | 'remote' | 'mesh';
  readonly quality: 'fast' | 'balanced' | 'accurate';
}

export interface CommandResult<T = any> {
  readonly success: boolean;
  readonly result?: T;
  readonly error?: string;
  readonly executionTime: number;
  readonly provider: string;
}

export const enum DaemonEvents {
  STARTED = 'daemon:started',
  STOPPED = 'daemon:stopped',
  CRASHED = 'daemon:crashed',
  HEALTH_CHECK = 'daemon:health_check',
  COMMAND_EXECUTED = 'command:executed',
  MESH_DISCOVERY = 'mesh:discovery'
}

export interface IDaemonManager {
  start(): Promise<void>;
  stop(): Promise<void>;
  registerDaemon(config: DaemonConfig): Promise<string>;
  startDaemon(id: string): Promise<boolean>;
  stopDaemon(id: string): Promise<boolean>;
  getDaemonInfo(id: string): Promise<DaemonInfo | null>;
  getAllDaemons(): Promise<readonly DaemonInfo[]>;
  executeCommand<T, R>(request: CommandRequest<T>): Promise<CommandResult<R>>;
}

export interface IDaemon {
  readonly name: string;
  readonly version: string;
  readonly status: DaemonStatus;
  
  start(): Promise<void>;
  stop(): Promise<void>;
  handleMessage<T, R>(message: DaemonMessage<T>): Promise<DaemonResponse<R>>;
  getHealthCheck(): Promise<DaemonInfo>;
}

export class DaemonError extends Error {
  constructor(
    public readonly code: string,
    message: string,
    public readonly context?: Readonly<Record<string, unknown>>
  ) {
    super(message);
    this.name = 'DaemonError';
  }
}

export namespace DaemonProtocol {
  export function createMessage<T>(
    from: string,
    to: string,
    type: string,
    data: T,
    options: Partial<Pick<DaemonMessage, 'priority' | 'timeout' | 'traceId'>> = {}
  ): DaemonMessage<T> {
    return {
      id: `msg-${Date.now().toString(36)}-${Math.random().toString(36).substring(2, 8)}`,
      from,
      to,
      type,
      data,
      timestamp: new Date(),
      ...options
    };
  }

  export function createResponse<T>(
    success: boolean,
    data?: T,
    error?: string
  ): DaemonResponse<T> {
    return {
      success,
      data,
      error,
      timestamp: new Date()
    };
  }
}