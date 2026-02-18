/**
 * ProcessManager - Generic Process Spawner
 *
 * Reusable infrastructure for spawning and managing child processes.
 * Used by LoggerDaemon, cognition layer (persona/tools), and other multiprocess components.
 */

import { spawn, fork, ChildProcess } from 'child_process';
import { EventEmitter } from 'events';
import type { IPCMessage, IPCResponse } from './IPCProtocol';
import { Logger, type ComponentLogger } from '../logging/Logger';

/**
 * Configuration for spawning a process
 */
export interface ProcessConfig {
  /** Unique identifier for this process instance */
  readonly processId: string;

  /** Command to execute (default: 'node', use 'npx' for tsx) */
  readonly command?: string;

  /** Script path to execute (relative to project root) */
  readonly scriptPath: string;

  /** Environment variables to pass to process */
  readonly env?: Record<string, string>;

  /** Command line arguments */
  readonly args?: string[];

  /** Working directory (defaults to current) */
  readonly cwd?: string;

  /** Auto-restart on crash (default: true) */
  readonly autoRestart?: boolean;

  /** Max restart attempts (default: 3) */
  readonly maxRestarts?: number;

  /** Restart delay in ms (default: 1000) */
  readonly restartDelayMs?: number;

  /** Health check interval in ms (default: 5000) */
  readonly healthCheckIntervalMs?: number;
}

/**
 * Process state
 */
export type ProcessState =
  | 'starting'    // Process is spawning
  | 'running'     // Process is healthy and operational
  | 'stopping'    // Process is shutting down
  | 'stopped'     // Process has stopped
  | 'crashed'     // Process exited unexpectedly
  | 'restarting'; // Process is being restarted

/**
 * Process health status
 */
export interface ProcessHealth {
  readonly state: ProcessState;
  readonly pid?: number;
  readonly uptime: number;          // Milliseconds since start
  readonly restartCount: number;    // How many times restarted
  readonly lastHealthCheck: string; // ISO timestamp
  readonly memoryUsage?: number;    // Bytes (if available)
}

/**
 * Managed Process Instance
 */
export class ManagedProcess extends EventEmitter {
  private _process: ChildProcess | null = null;
  private _state: ProcessState = 'stopped';
  private _startTime: number = 0;
  private _restartCount: number = 0;
  private _healthCheckTimer: NodeJS.Timeout | null = null;
  private _lastHealthCheck: string = new Date().toISOString();
  private readonly log: ComponentLogger;

  constructor(
    private readonly config: ProcessConfig,
    private readonly manager: ProcessManager
  ) {
    super();
    this.log = Logger.create(
      `Process[${config.processId}]`,
      `process/${config.processId}`
    );
  }

  /**
   * Start the process
   */
  async start(): Promise<void> {
    if (this._state === 'running' || this._state === 'starting') {
      this.log.warn('Process already running or starting');
      return;
    }

    this._state = 'starting';
    this.log.info('Starting process', { scriptPath: this.config.scriptPath });

    try {
      // Use fork() for tsx processes to enable IPC, spawn() for others
      const command = this.config.command || 'node';

      if (command === 'npx' && this.config.args?.includes('tsx')) {
        // fork() with tsx for IPC support (tsx must be installed)
        const tsxPath = require.resolve('tsx/dist/cli.mjs');
        this._process = fork(tsxPath, [this.config.scriptPath, ...(this.config.args?.slice(1) || [])], {
          cwd: this.config.cwd || process.cwd(),
          env: { ...process.env, ...this.config.env },
          silent: true  // Pipe stdout/stderr so we can forward them
        });
      } else {
        // Standard spawn for other commands
        const args = command === 'node'
          ? [this.config.scriptPath, ...(this.config.args || [])]
          : [...(this.config.args || []), this.config.scriptPath];

        this._process = spawn(command, args, {
          cwd: this.config.cwd || process.cwd(),
          env: { ...process.env, ...this.config.env },
          stdio: ['pipe', 'pipe', 'pipe', 'ipc'] // stdin, stdout, stderr, ipc
        });
      }

      this._startTime = Date.now();
      this._state = 'running';

      // Set up event listeners
      this.setupListeners();

      // Start health checks
      this.startHealthChecks();

      this.log.info('Process started', { pid: this._process.pid });
      this.emit('started', { pid: this._process.pid });

    } catch (error) {
      this._state = 'crashed';
      this.log.error('Failed to start process:', error);
      this.emit('error', error);

      if (this.shouldRestart()) {
        await this.restart();
      }
    }
  }

  /**
   * Stop the process gracefully
   */
  async stop(): Promise<void> {
    if (this._state === 'stopped' || this._state === 'stopping') {
      return;
    }

    this._state = 'stopping';
    this.log.info('Stopping process');

    // Stop health checks
    this.stopHealthChecks();

    if (this._process) {
      // Send shutdown signal via IPC first (graceful)
      this.send({ type: 'shutdown' });

      // Give it 5 seconds to shut down gracefully
      await new Promise<void>((resolve) => {
        const timeout = setTimeout(() => {
          if (this._process && !this._process.killed) {
            this.log.warn('Process did not shut down gracefully, forcing kill');
            this._process.kill('SIGKILL');
          }
          resolve();
        }, 5000);

        this._process?.once('exit', () => {
          clearTimeout(timeout);
          resolve();
        });
      });
    }

    this._state = 'stopped';
    this._process = null;
    this.log.info('Process stopped');
    this.emit('stopped');
  }

  /**
   * Restart the process
   */
  async restart(): Promise<void> {
    this._state = 'restarting';
    this.log.info('Restarting process', { restartCount: this._restartCount });

    await this.stop();

    // Wait for restart delay
    const delay = this.config.restartDelayMs || 1000;
    await new Promise(resolve => setTimeout(resolve, delay));

    this._restartCount++;
    await this.start();
  }

  /**
   * Send IPC message to process
   */
  send(message: IPCMessage): void {
    if (!this._process || this._state !== 'running') {
      this.log.warn('Cannot send message, process not running');
      return;
    }

    if (!this._process.send) {
      this.log.warn('Process does not support IPC');
      return;
    }

    this._process.send(message);
  }

  /**
   * Get process health status
   */
  getHealth(): ProcessHealth {
    return {
      state: this._state,
      pid: this._process?.pid,
      uptime: this._state === 'running' ? Date.now() - this._startTime : 0,
      restartCount: this._restartCount,
      lastHealthCheck: this._lastHealthCheck,
      memoryUsage: this._process ? (this._process as any).memoryUsage?.rss : undefined
    };
  }

  /**
   * Get current state
   */
  get state(): ProcessState {
    return this._state;
  }

  /**
   * Get process ID
   */
  get pid(): number | undefined {
    return this._process?.pid;
  }

  /**
   * Set up process event listeners
   */
  private setupListeners(): void {
    if (!this._process) return;

    // Handle IPC messages from child
    this._process.on('message', (message: IPCMessage | IPCResponse) => {
      this.emit('message', message);
    });

    // Handle stdout - forward directly to console to avoid circular IPC dependency
    this._process.stdout?.on('data', (data: Buffer) => {
      const output = data.toString().trim();
      console.log(`[${this.config.processId}] ${output}`);
      this.log.debug('Process stdout:', output);
    });

    // Handle stderr - forward directly to console to avoid circular IPC dependency
    this._process.stderr?.on('data', (data: Buffer) => {
      const output = data.toString().trim();
      console.error(`[${this.config.processId}] ${output}`);
      this.log.error('Process stderr:', output);
    });

    // Handle exit
    this._process.on('exit', async (code: number | null, signal: string | null) => {
      this.log.info('Process exited', { code, signal });

      if (this._state === 'stopping' || this._state === 'stopped') {
        // Expected exit
        return;
      }

      // Unexpected exit
      this._state = 'crashed';
      this.emit('crashed', { code, signal });

      if (this.shouldRestart()) {
        await this.restart();
      }
    });

    // Handle errors
    this._process.on('error', (error: Error) => {
      this.log.error('Process error:', error);
      this.emit('error', error);
    });
  }

  /**
   * Start periodic health checks
   */
  private startHealthChecks(): void {
    const interval = this.config.healthCheckIntervalMs || 5000;

    this._healthCheckTimer = setInterval(() => {
      this._lastHealthCheck = new Date().toISOString();

      // Send health check ping
      this.send({ type: 'health-check' });

      // Emit health status
      this.emit('health', this.getHealth());
    }, interval);
  }

  /**
   * Stop health checks
   */
  private stopHealthChecks(): void {
    if (this._healthCheckTimer) {
      clearInterval(this._healthCheckTimer);
      this._healthCheckTimer = null;
    }
  }

  /**
   * Determine if process should be restarted
   */
  private shouldRestart(): boolean {
    if (this.config.autoRestart === false) {
      return false;
    }

    const maxRestarts = this.config.maxRestarts || 3;
    return this._restartCount < maxRestarts;
  }
}

/**
 * ProcessManager - Manages multiple child processes
 */
export class ProcessManager {
  private readonly processes = new Map<string, ManagedProcess>();
  private readonly log: ComponentLogger;
  private _isShuttingDown = false;

  constructor() {
    this.log = Logger.create('ProcessManager', 'process/manager');

    // Handle process exit
    process.on('SIGINT', () => this.shutdown());
    process.on('SIGTERM', () => this.shutdown());
  }

  /**
   * Spawn a new managed process
   */
  spawn(config: ProcessConfig): ManagedProcess {
    if (this.processes.has(config.processId)) {
      throw new Error(`Process ${config.processId} already exists`);
    }

    this.log.info('Spawning process', { processId: config.processId });

    const managedProcess = new ManagedProcess(config, this);
    this.processes.set(config.processId, managedProcess);

    // Forward events
    managedProcess.on('started', (data) => {
      this.log.info('Process started', { processId: config.processId, ...data });
    });

    managedProcess.on('crashed', (data) => {
      this.log.error('Process crashed', { processId: config.processId, ...data });
    });

    managedProcess.on('stopped', () => {
      this.log.info('Process stopped', { processId: config.processId });
    });

    return managedProcess;
  }

  /**
   * Get a managed process by ID
   */
  get(processId: string): ManagedProcess | undefined {
    return this.processes.get(processId);
  }

  /**
   * Get all managed processes
   */
  getAll(): ManagedProcess[] {
    return Array.from(this.processes.values());
  }

  /**
   * Get health status for all processes
   */
  getAllHealth(): Record<string, ProcessHealth> {
    const health: Record<string, ProcessHealth> = {};
    for (const [id, process] of this.processes) {
      health[id] = process.getHealth();
    }
    return health;
  }

  /**
   * Stop a specific process
   */
  async stopProcess(processId: string): Promise<void> {
    const process = this.processes.get(processId);
    if (!process) {
      throw new Error(`Process ${processId} not found`);
    }

    await process.stop();
    this.processes.delete(processId);
  }

  /**
   * Shutdown all processes
   */
  async shutdown(): Promise<void> {
    if (this._isShuttingDown) {
      return;
    }

    this._isShuttingDown = true;
    this.log.info('Shutting down all processes', { count: this.processes.size });

    const stopPromises = Array.from(this.processes.values()).map(p => p.stop());
    await Promise.all(stopPromises);

    this.processes.clear();
    this.log.info('All processes stopped');
  }
}
