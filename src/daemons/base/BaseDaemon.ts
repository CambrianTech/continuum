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
  
  private status: DaemonStatus = 'stopped';
  private startTime?: Date;
  private lastHeartbeat?: Date;
  private processId: number = process.pid;
  private heartbeatInterval?: NodeJS.Timeout;
  
  constructor() {
    super();
    this.setupSignalHandlers();
  }

  /**
   * Start the daemon
   */
  async start(): Promise<void> {
    if (this.status !== 'stopped') {
      throw new Error(`Daemon ${this.name} is already ${this.status}`);
    }

    this.status = 'starting';
    this.startTime = new Date();
    
    this.log(`Starting daemon ${this.name} v${this.version}`);
    
    try {
      // CRITICAL: Call the subclass's onStart() method
      await this.onStart();
      
      // Start heartbeat
      this.startHeartbeat();
      
      this.status = 'running';
      this.emit('started');
      
    } catch (error) {
      this.status = 'failed';
      this.log(`Failed to start daemon: ${error.message}`, 'error');
      throw error;
    }
  }

  /**
   * Stop the daemon gracefully
   */
  async stop(): Promise<void> {
    if (this.status === 'stopped') {
      return;
    }

    this.status = 'stopping';
    this.log(`Stopping daemon ${this.name}`);
    
    try {
      // CRITICAL: Call the subclass's onStop() method
      await this.onStop();
      
      // Stop heartbeat
      this.stopHeartbeat();
      
      this.status = 'stopped';
      this.emit('stopped');
      
    } catch (error) {
      this.log(`Error stopping daemon: ${error.message}`, 'error');
      this.status = 'stopped'; // Still mark as stopped even if cleanup failed
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
  getStatus(): DaemonStatusInfo {
    return {
      name: this.name,
      version: this.version,
      status: this.status,
      pid: this.processId,
      startTime: this.startTime,
      lastHeartbeat: this.lastHeartbeat,
      uptime: this.getUptime(),
      memoryUsage: process.memoryUsage(),
      cpuUsage: process.cpuUsage()
    };
  }

  /**
   * Abstract method for handling messages - each daemon implements its own logic
   */
  abstract handleMessage(message: DaemonMessage): Promise<DaemonResponse>;

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
    process.on('SIGTERM', async () => {
      this.log('Received SIGTERM, shutting down gracefully');
      await this.stop();
      process.exit(0);
    });

    process.on('SIGINT', async () => {
      this.log('Received SIGINT, shutting down gracefully');
      await this.stop();
      process.exit(0);
    });

    process.on('uncaughtException', (error) => {
      this.log(`Uncaught exception: ${error.message}`, 'error');
      this.log(error.stack || '', 'error');
      process.exit(1);
    });

    process.on('unhandledRejection', (reason) => {
      this.log(`Unhandled rejection: ${reason}`, 'error');
      process.exit(1);
    });
  }

  /**
   * Generate unique message ID
   */
  private generateMessageId(): string {
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