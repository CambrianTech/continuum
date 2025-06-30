/**
 * Base Daemon - Foundation for all Continuum OS daemon processes
 * Provides standard daemon lifecycle, IPC, logging, and management
 */

import { EventEmitter } from 'events';
import { DaemonMessage, DaemonResponse, DaemonStatus } from './DaemonProtocol';

export abstract class BaseDaemon extends EventEmitter {
  public abstract readonly name: string;
  public abstract readonly version: string;
  
  // Abstract lifecycle methods that subclasses must implement
  protected abstract onStart(): Promise<void>;
  protected abstract onStop(): Promise<void>;
  protected abstract handleMessage(message: DaemonMessage): Promise<DaemonResponse>;
  
  private status: DaemonStatus = DaemonStatus.STOPPED;
  private startTime?: Date;
  private lastHeartbeat?: Date;
  private processId: number = process.pid;
  private heartbeatInterval: NodeJS.Timeout | undefined;
  private startPromise?: Promise<void>;
  private stopPromise?: Promise<void>;
  private signalHandlers: { [key: string]: (signal: NodeJS.Signals) => void } = {};
  
  constructor() {
    super();
    this.setupSignalHandlers();
  }

  /**
   * Start the daemon
   */
  async start(): Promise<void> {
    // Handle concurrent start attempts
    if (this.startPromise) {
      return this.startPromise;
    }
    
    if (this.status === DaemonStatus.RUNNING) {
      return;
    }
    
    if (this.status !== DaemonStatus.STOPPED) {
      throw new Error(`Daemon ${this.name} is already ${this.status}`);
    }

    this.startPromise = this._performStart();
    try {
      await this.startPromise;
    } finally {
      this.startPromise = undefined;
    }
  }

  private async _performStart(): Promise<void> {
    this.status = DaemonStatus.STARTING;
    this.startTime = new Date();
    
    this.log(`Starting daemon ${this.name} v${this.version}`);
    
    try {
      // CRITICAL: Call the subclass's onStart() method
      await this.onStart();
      
      // Start heartbeat
      this.startHeartbeat();
      
      this.status = DaemonStatus.RUNNING;
      this.emit('started');
      
    } catch (error) {
      this.status = DaemonStatus.FAILED;
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`Failed to start daemon: ${errorMessage}`, 'error');
      throw error;
    }
  }

  /**
   * Stop the daemon gracefully
   */
  async stop(): Promise<void> {
    // Handle concurrent stop attempts
    if (this.stopPromise) {
      return this.stopPromise;
    }
    
    if (this.status === DaemonStatus.STOPPED) {
      return;
    }

    this.stopPromise = this._performStop();
    try {
      await this.stopPromise;
    } finally {
      this.stopPromise = undefined;
    }
  }

  private async _performStop(): Promise<void> {
    this.status = DaemonStatus.STOPPING;
    this.log(`Stopping daemon ${this.name}`);
    
    try {
      // CRITICAL: Call the subclass's onStop() method
      await this.onStop();
      
      // Stop heartbeat
      this.stopHeartbeat();
      
      // Cleanup signal handlers
      this.cleanupSignalHandlers();
      
      this.status = DaemonStatus.STOPPED;
      this.emit('stopped');
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`Error stopping daemon: ${errorMessage}`, 'error');
      this.status = DaemonStatus.STOPPED; // Still mark as stopped even if cleanup failed
      throw error;
    }
  }

  /**
   * Get simple status string
   */
  getSimpleStatus(): DaemonStatus {
    return this.status;
  }

  /**
   * Get daemon uptime in milliseconds
   */
  getUptime(): number {
    return this.startTime ? Date.now() - this.startTime.getTime() : 0;
  }

  /**
   * Get daemon status and metrics
   */
  /**
   * Check if daemon is currently running (including during shutdown)
   */
  isRunning(): boolean {
    return this.status === DaemonStatus.RUNNING || this.status === DaemonStatus.STOPPING;
  }

  getStatus(): DaemonStatusInfo {
    const status: DaemonStatusInfo = {
      name: this.name,
      version: this.version,
      status: this.status,
      pid: this.processId,
      uptime: this.getUptime(),
      memoryUsage: process.memoryUsage(),
      cpuUsage: process.cpuUsage()
    };
    
    if (this.startTime) {
      status.startTime = this.startTime;
    }
    
    if (this.lastHeartbeat) {
      status.lastHeartbeat = this.lastHeartbeat;
    }
    
    return status;
  }

  // handleMessage is already declared as abstract above

  /**
   * Send message to another daemon or OS component
   */
  protected async sendMessage(target: string, type: string, data: any): Promise<DaemonResponse> {
    const message: DaemonMessage = {
      id: this.generateMessageId(),
      from: this.name,
      to: target,
      type,
      data,
      timestamp: new Date()
    };

    // Send via IPC or message bus
    return await this.sendViaIPC(message);
  }

  /**
   * Log message with daemon context
   */
  protected log(message: string, level: 'info' | 'warn' | 'error' | 'debug' = 'info'): void {
    const timestamp = new Date().toISOString();
    const logMessage = `[${timestamp}] [${this.name}:${this.processId}] ${level.toUpperCase()}: ${message}`;
    
    switch (level) {
      case 'error':
        console.error(logMessage);
        break;
      case 'warn':
        console.warn(logMessage);
        break;
      case 'debug':
        if (process.env.DEBUG) console.log(logMessage);
        break;
      default:
        console.log(logMessage);
    }

    // Emit log event for external log aggregation
    this.emit('log', { level, message, timestamp });
  }

  /**
   * Send health check heartbeat
   */
  private startHeartbeat(): void {
    this.heartbeatInterval = setInterval(() => {
      this.lastHeartbeat = new Date();
      this.emit('heartbeat', this.getStatus());
    }, 30000); // Every 30 seconds
  }

  private stopHeartbeat(): void {
    if (this.heartbeatInterval) {
      clearInterval(this.heartbeatInterval);
      this.heartbeatInterval = undefined;
    }
  }

  /**
   * Setup signal handlers for graceful shutdown
   */
  private setupSignalHandlers(): void {
    this.signalHandlers['SIGTERM'] = async () => {
      this.log('Received SIGTERM, shutting down gracefully');
      await this.stop();
      process.exit(0);
    };

    this.signalHandlers['SIGINT'] = async () => {
      this.log('Received SIGINT, shutting down gracefully');
      await this.stop();
      process.exit(0);
    };

    this.signalHandlers['uncaughtException'] = (error: Error) => {
      this.log(`Uncaught exception: ${error.message}`, 'error');
      this.log(error.stack || '', 'error');
      process.exit(1);
    };

    this.signalHandlers['unhandledRejection'] = (reason: any) => {
      this.log(`Unhandled rejection: ${reason}`, 'error');
      process.exit(1);
    };

    process.on('SIGTERM', this.signalHandlers['SIGTERM']);
    process.on('SIGINT', this.signalHandlers['SIGINT']);
    process.on('uncaughtException', this.signalHandlers['uncaughtException']);
    process.on('unhandledRejection', this.signalHandlers['unhandledRejection']);
  }

  /**
   * Clean up signal handlers to prevent memory leaks
   */
  private cleanupSignalHandlers(): void {
    if (this.signalHandlers['SIGTERM']) {
      process.off('SIGTERM', this.signalHandlers['SIGTERM']);
    }
    if (this.signalHandlers['SIGINT']) {
      process.off('SIGINT', this.signalHandlers['SIGINT']);
    }
    if (this.signalHandlers['uncaughtException']) {
      process.off('uncaughtException', this.signalHandlers['uncaughtException']);
    }
    if (this.signalHandlers['unhandledRejection']) {
      process.off('unhandledRejection', this.signalHandlers['unhandledRejection']);
    }
    this.signalHandlers = {};
  }

  /**
   * Generate unique message ID
   */
  protected generateMessageId(): string {
    return `${this.name}-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
  }

  /**
   * Send message via IPC (implementation depends on Continuum OS message bus)
   */
  private async sendViaIPC(message: DaemonMessage): Promise<DaemonResponse> {
    // This would interface with Continuum OS's IPC system
    // For now, return a success response
    return {
      success: true,
      data: `Message sent from ${this.name} to ${message.to}`
    };
  }
}

export interface DaemonStatusInfo {
  name: string;
  version: string;
  status: DaemonStatus;
  pid: number;
  startTime?: Date;
  lastHeartbeat?: Date;
  uptime: number;
  memoryUsage: NodeJS.MemoryUsage;
  cpuUsage: NodeJS.CpuUsage;
}