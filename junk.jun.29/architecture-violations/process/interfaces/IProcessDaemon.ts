/**
 * Process Daemon Interface
 * Base contract for all daemon processes including core continuum
 */

import { ProcessMessage, ProcessResult, ProcessHealth } from './IProcessCoordinator.js';

/**
 * Base interface for all daemon processes
 * Every daemon (including core continuum) runs in its own process/worker
 */
export interface IProcessDaemon {
  readonly daemonType: string;
  readonly capabilities: string[];
  readonly processId: string;
  
  // Lifecycle
  start(): Promise<void>;
  stop(): Promise<void>;
  restart(): Promise<void>;
  
  // Message handling (IPC communication)
  handleMessage(message: ProcessMessage): Promise<ProcessResult>;
  sendMessage(targetProcess: string, message: ProcessMessage): Promise<void>;
  
  // Health monitoring
  getHealth(): ProcessHealth;
  heartbeat(): void;
  
  // Process registration
  registerCapabilities(): string[];
  getConfiguration(): any;
}

/**
 * Web Worker specific daemon interface
 * For daemons that run in browser web workers
 */
export interface IWebWorkerDaemon extends IProcessDaemon {
  // Web Worker specific lifecycle
  onWorkerMessage(event: MessageEvent): void;
  postMessageToMain(message: any): void;
  
  // Worker thread management
  setupWorkerListeners(): void;
  cleanupWorkerListeners(): void;
}

/**
 * Node.js Process specific daemon interface  
 * For daemons that run in separate Node.js processes
 */
export interface INodeProcessDaemon extends IProcessDaemon {
  // Process specific lifecycle
  onProcessMessage(message: any): void;
  sendToParent(message: any): void;
  
  // Process management
  setupProcessListeners(): void;
  cleanupProcessListeners(): void;
  gracefulShutdown(): Promise<void>;
}