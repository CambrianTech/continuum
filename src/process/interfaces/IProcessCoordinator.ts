/**
 * Process Coordinator Interface
 * Core contract for managing multiple daemon processes
 */

export interface ProcessMessage {
  id: string;
  type: string;
  data: any;
  timestamp: number;
  sourceProcess?: string;
  targetProcess?: string;
}

export interface ProcessResult {
  success: boolean;
  data?: any;
  error?: string;
  processId?: string;
}

export interface ProcessHealth {
  processId: string;
  status: 'healthy' | 'unhealthy' | 'starting' | 'stopping';
  uptime: number;
  memory: number;
  cpu: number;
  lastHeartbeat: number;
}

export interface ProcessConfig {
  type: string;
  entryPoint: string;
  capabilities: string[];
  packagePath: string;
  maxMemory?: number;
  maxCpu?: number;
  restartOnCrash?: boolean;
}

/**
 * Main process coordinator interface
 * Manages lifecycle and communication for all daemon processes
 */
export interface IProcessCoordinator {
  // Lifecycle management
  spawn(daemonType: string): Promise<string>;
  kill(processId: string): Promise<void>;
  restart(processId: string): Promise<void>;
  
  // Message routing
  route(message: ProcessMessage): Promise<ProcessResult>;
  broadcast(message: ProcessMessage): Promise<ProcessResult[]>;
  
  // Registry management
  getAvailable(): string[];
  getProcessConfig(daemonType: string): ProcessConfig | null;
  
  // Health monitoring
  healthCheck(): Promise<ProcessHealth[]>;
  onProcessExit(callback: (processId: string) => void): void;
  
  // System management
  start(): Promise<void>;
  stop(): Promise<void>;
  getSystemStatus(): any;
}