/**
 * Base Daemon - Foundation for all Continuum OS daemon processes
 * Provides standard daemon lifecycle, IPC, logging, and management
 */

import { EventEmitter } from 'events';
import * as fs from 'fs';
import * as fsPromises from 'fs/promises';
import * as path from 'path';
import { DaemonMessage, DaemonResponse, DaemonStatus } from './DaemonProtocol';
import { DaemonType } from './DaemonTypes';

// Global daemon registry for inter-daemon communication
export const DAEMON_REGISTRY = new Map<string, BaseDaemon>();

export abstract class BaseDaemon extends EventEmitter {
  public abstract readonly name: string;
  public abstract readonly version: string;
  public abstract readonly daemonType: DaemonType;
  
  /**
   * Static metadata for daemon discovery
   * Daemons can override this to provide custom configuration
   */
  static readonly metadata = {
    autoStart: true,
    priority: 50,
    dependencies: [] as string[]
  };
  
  // Abstract lifecycle methods that subclasses must implement
  protected abstract onStart(): Promise<void>;
  protected abstract onStop(): Promise<void>;
  protected abstract handleMessage(message: DaemonMessage): Promise<DaemonResponse>;
  
  private status: DaemonStatus = DaemonStatus.STOPPED;
  private startTime?: Date;
  private lastHeartbeat?: Date;
  private processId: number = process.pid;
  private heartbeatInterval: ReturnType<typeof setInterval> | undefined;
  private startPromise: Promise<void> | null = null;
  private stopPromise: Promise<void> | null = null;
  private signalHandlers: { [key: string]: (...args: unknown[]) => void } = {};
  
  // Session-specific logging support
  private sessionLogPath: string | null = null;
  
  constructor() {
    super();
    this.ensureValidWorkingDirectory();
    this.setupSignalHandlers();
  }

  /**
   * Ensure daemon starts in the continuum project root
   * Prevents "getcwd: cannot access parent directories" errors
   */
  private ensureValidWorkingDirectory(): void {
    try {
      // fs and path are already imported at the top
      
      // Find continuum root by walking up from __dirname (don't use process.cwd())
      let searchDir = __dirname;
      let continuumRoot = null;
      
      while (searchDir !== path.dirname(searchDir)) {
        const packagePath = path.join(searchDir, 'package.json');
        if (fs.existsSync(packagePath)) {
          try {
            const pkg = JSON.parse(fs.readFileSync(packagePath, 'utf8'));
            if (pkg.name === 'continuum') {
              continuumRoot = searchDir;
              break;
            }
          } catch {
            // Skip invalid package.json files
          }
        }
        searchDir = path.dirname(searchDir);
      }
      
      // Change to continuum root if found
      if (continuumRoot && fs.existsSync(continuumRoot)) {
        process.chdir(continuumRoot);
      } else {
        // Fallback: use the resolved path from __dirname
        const fallbackRoot = path.resolve(__dirname, '../../../..');
        if (fs.existsSync(fallbackRoot)) {
          process.chdir(fallbackRoot);
        }
      }
    } catch {
      // Don't log anything - avoid any operations that might fail
      // Just let the daemon start from wherever it is
    }
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
      this.startPromise = null;
    }
  }

  private async _performStart(): Promise<void> {
    this.status = DaemonStatus.STARTING;
    this.startTime = new Date();
    
    // Register daemon in global registry for inter-daemon communication
    DAEMON_REGISTRY.set(this.name, this);
    
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
      this.stopPromise = null;
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
  protected async sendMessage(target: DaemonType, type: string, data: unknown): Promise<DaemonResponse> {
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
   * Set session-specific log file path for live logging
   */
  setSessionLogPath(logFilePath: string): void {
    this.sessionLogPath = logFilePath;
    this.log(`Session logging enabled: ${logFilePath}`);
  }

  /**
   * Clear session-specific logging
   */
  clearSessionLogPath(): void {
    if (this.sessionLogPath) {
      this.log(`Session logging disabled: ${this.sessionLogPath}`);
      this.sessionLogPath = null;
    }
  }

  /**
   * Log message with daemon context - writes to both console and session log
   */
  protected log(message: string, level: 'info' | 'warn' | 'error' | 'debug' = 'info'): void {
    const timestamp = new Date().toISOString();
    const safeLevel = typeof level === 'string' ? level : 'info';
    const logMessage = `[${timestamp}] [${this.name}:${this.processId}] ${safeLevel.toUpperCase()}: ${message}`;
    
    // Console output (existing behavior)
    switch (safeLevel) {
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

    // Session-specific log file (NEW: live logging)
    if (this.sessionLogPath) {
      this.writeToSessionLog(logMessage).catch(error => {
        console.error(`Failed to write to session log: ${error.message}`);
      });
    }

    // Emit log event for external log aggregation
    this.emit('log', { level, message, timestamp });
  }

  /**
   * Write log message to session-specific log file
   */
  private async writeToSessionLog(logMessage: string): Promise<void> {
    if (!this.sessionLogPath) return;
    
    try {
      await fsPromises.appendFile(this.sessionLogPath, logMessage + '\n');
    } catch (error) {
      // Don't log to session file if there's an error (would cause recursion)
      console.error(`Session log write failed: ${error instanceof Error ? error.message : String(error)}`);
    }
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

    this.signalHandlers['uncaughtException'] = (error: unknown) => {
      const errorMessage = error instanceof Error ? error.message : String(error);
      const errorStack = error instanceof Error ? error.stack : '';
      this.log(`Uncaught exception: ${errorMessage}`, 'error');
      if (errorStack) this.log(errorStack, 'error');
      process.exit(1);
    };

    this.signalHandlers['unhandledRejection'] = (reason: unknown) => {
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
   * Send message via IPC using daemon registry
   */
  private async sendViaIPC(message: DaemonMessage): Promise<DaemonResponse> {
    const targetDaemon = DAEMON_REGISTRY.get(message.to);
    
    if (!targetDaemon) {
      return {
        success: false,
        error: `Target daemon '${message.to}' not found in registry. Available daemons: ${Array.from(DAEMON_REGISTRY.keys()).join(', ')}`
      };
    }
    
    try {
      // Call the target daemon's handleMessage method directly
      const response = await targetDaemon.handleMessage(message);
      return response;
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        error: `IPC error: ${errorMessage}`
      };
    }
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
  memoryUsage: ReturnType<typeof process.memoryUsage>;
  cpuUsage: ReturnType<typeof process.cpuUsage>;
}