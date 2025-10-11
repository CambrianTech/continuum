/**
 * ProcessPool - Elastic process lifecycle management
 *
 * Kubernetes-style process pool for genome inference workers.
 * Manages Hot/Warm/Cold pools with auto-scaling and health monitoring.
 *
 * Architecture:
 * - HOT Pool: Pre-spawned processes with genomes loaded (< 10ms latency)
 * - WARM Pool: Processes ready to load genomes (< 500ms latency)
 * - COLD Start: Spawn new process + load genome (< 3s latency)
 *
 * Phase 2.1: Basic spawn/kill lifecycle
 * Phase 2.2: Pool management + health monitoring
 * Phase 2.3: Auto-scaling + eviction strategies
 */

import { ChildProcess, fork } from 'child_process';
import { EventEmitter } from 'events';
import type { UUID } from '../../core/types/CrossPlatformUUID';

/**
 * Process state machine
 */
export type ProcessState =
  | 'spawning'
  | 'idle'
  | 'loading'
  | 'ready'
  | 'busy'
  | 'unhealthy'
  | 'terminating'
  | 'terminated';

/**
 * Pool tier for performance targeting
 */
export type PoolTier = 'hot' | 'warm' | 'cold';

/**
 * Managed process wrapper
 */
export interface ManagedProcess {
  id: UUID;
  pid: number;
  process: ChildProcess;
  state: ProcessState;
  poolTier: PoolTier;
  genomeId?: UUID;
  spawnedAt: number;
  lastUsed: number;
  requestCount: number;
  errorCount: number;
  memoryMB: number;
}

/**
 * Process pool configuration
 */
export interface ProcessPoolConfig {
  // Pool sizes
  hotPoolSize: number; // Default: 3
  warmPoolSize: number; // Default: 10
  minProcesses: number; // Default: 1
  maxProcesses: number; // Default: 10

  // Health monitoring
  healthCheckIntervalMs: number; // Default: 5000
  maxIdleTimeMs: number; // Default: 60000
  maxMemoryMB: number; // Default: 2048

  // Process limits
  maxRequestsPerProcess: number; // Default: 1000
  maxErrorsBeforeTerminate: number; // Default: 5
  processTimeoutMs: number; // Default: 30000
}

/**
 * Process pool events
 */
export interface ProcessPoolEvents {
  'process-spawned': (process: ManagedProcess) => void;
  'process-ready': (process: ManagedProcess) => void;
  'process-busy': (process: ManagedProcess) => void;
  'process-idle': (process: ManagedProcess) => void;
  'process-unhealthy': (process: ManagedProcess, reason: string) => void;
  'process-terminated': (process: ManagedProcess, reason: string) => void;
  'pool-full': (tier: PoolTier) => void;
  'pool-empty': (tier: PoolTier) => void;
}

/**
 * ProcessPool - Elastic process lifecycle manager
 */
export class ProcessPool extends EventEmitter {
  private readonly config: ProcessPoolConfig;
  private readonly processes: Map<UUID, ManagedProcess> = new Map();
  private readonly workerScriptPath: string;
  private healthCheckInterval?: NodeJS.Timeout;
  private isShuttingDown = false;

  constructor(workerScriptPath: string, config?: Partial<ProcessPoolConfig>) {
    super();
    this.workerScriptPath = workerScriptPath;
    this.config = {
      hotPoolSize: config?.hotPoolSize ?? 3,
      warmPoolSize: config?.warmPoolSize ?? 10,
      minProcesses: config?.minProcesses ?? 1,
      maxProcesses: config?.maxProcesses ?? 10,
      healthCheckIntervalMs: config?.healthCheckIntervalMs ?? 5000,
      maxIdleTimeMs: config?.maxIdleTimeMs ?? 60000,
      maxMemoryMB: config?.maxMemoryMB ?? 2048,
      maxRequestsPerProcess: config?.maxRequestsPerProcess ?? 1000,
      maxErrorsBeforeTerminate: config?.maxErrorsBeforeTerminate ?? 5,
      processTimeoutMs: config?.processTimeoutMs ?? 30000,
    };
  }

  /**
   * Initialize pool with minimum processes
   */
  async initialize(): Promise<void> {
    console.log('🏊 ProcessPool: Initializing with minimum processes...');

    // Start health monitoring
    this.healthCheckInterval = setInterval(
      () => this.performHealthCheck(),
      this.config.healthCheckIntervalMs
    );

    // Spawn minimum processes
    for (let i = 0; i < this.config.minProcesses; i++) {
      await this.spawnProcess('warm');
    }

    console.log(
      `✅ ProcessPool: Initialized with ${this.processes.size} processes`
    );
  }

  /**
   * Spawn a new worker process
   */
  async spawnProcess(tier: PoolTier): Promise<ManagedProcess | null> {
    if (this.processes.size >= this.config.maxProcesses) {
      console.warn('⚠️  ProcessPool: Max processes reached, cannot spawn');
      this.emit('pool-full', tier);
      return null;
    }

    const processId = this.generateProcessId();
    const startTime = Date.now();

    console.log(`🔄 ProcessPool: Spawning ${tier} process ${processId}...`);

    try {
      const childProcess = fork(this.workerScriptPath, [], {
        stdio: ['pipe', 'pipe', 'pipe', 'ipc'],
        env: {
          ...process.env,
          PROCESS_ID: processId,
          POOL_TIER: tier,
        },
      });

      if (!childProcess.pid) {
        throw new Error('Failed to get PID from spawned process');
      }

      const managedProcess: ManagedProcess = {
        id: processId,
        pid: childProcess.pid,
        process: childProcess,
        state: 'spawning',
        poolTier: tier,
        spawnedAt: Date.now(),
        lastUsed: Date.now(),
        requestCount: 0,
        errorCount: 0,
        memoryMB: 0,
      };

      this.processes.set(processId, managedProcess);

      // Set up process event handlers
      this.setupProcessHandlers(managedProcess);

      // Wait for ready message from worker
      await this.waitForReady(managedProcess);

      const spawnTime = Date.now() - startTime;
      console.log(
        `✅ ProcessPool: ${tier} process ${processId} spawned in ${spawnTime}ms (PID: ${childProcess.pid})`
      );

      this.emit('process-spawned', managedProcess);
      return managedProcess;
    } catch (error) {
      console.error(`❌ ProcessPool: Failed to spawn process:`, error);
      this.processes.delete(processId);
      return null;
    }
  }

  /**
   * Terminate a specific process
   */
  async terminateProcess(
    processId: UUID,
    reason: string = 'manual'
  ): Promise<boolean> {
    const managedProcess = this.processes.get(processId);
    if (!managedProcess) {
      console.warn(`⚠️  ProcessPool: Process ${processId} not found`);
      return false;
    }

    console.log(
      `🔄 ProcessPool: Terminating process ${processId} (reason: ${reason})...`
    );

    managedProcess.state = 'terminating';

    try {
      // Send graceful shutdown message
      managedProcess.process.send({ type: 'shutdown' });

      // Wait for graceful shutdown with timeout
      await Promise.race([
        new Promise<void>((resolve) => {
          managedProcess.process.once('exit', () => resolve());
        }),
        new Promise<void>((resolve) =>
          setTimeout(() => {
            console.warn(
              `⚠️  ProcessPool: Process ${processId} didn't exit gracefully, force killing`
            );
            managedProcess.process.kill('SIGKILL');
            resolve();
          }, 5000)
        ),
      ]);

      managedProcess.state = 'terminated';
      this.processes.delete(processId);

      console.log(`✅ ProcessPool: Process ${processId} terminated`);
      this.emit('process-terminated', managedProcess, reason);
      return true;
    } catch (error) {
      console.error(
        `❌ ProcessPool: Failed to terminate process ${processId}:`,
        error
      );
      return false;
    }
  }

  /**
   * Shutdown entire pool
   */
  async shutdown(): Promise<void> {
    console.log('🔄 ProcessPool: Shutting down...');
    this.isShuttingDown = true;

    // Stop health checks
    if (this.healthCheckInterval) {
      clearInterval(this.healthCheckInterval);
    }

    // Terminate all processes
    const terminationPromises = Array.from(this.processes.keys()).map((id) =>
      this.terminateProcess(id, 'pool-shutdown')
    );

    await Promise.all(terminationPromises);

    console.log('✅ ProcessPool: Shutdown complete');
  }

  /**
   * Get pool statistics
   */
  getStats() {
    const processes = Array.from(this.processes.values());

    return {
      total: this.processes.size,
      byState: {
        spawning: processes.filter((p) => p.state === 'spawning').length,
        idle: processes.filter((p) => p.state === 'idle').length,
        loading: processes.filter((p) => p.state === 'loading').length,
        ready: processes.filter((p) => p.state === 'ready').length,
        busy: processes.filter((p) => p.state === 'busy').length,
        unhealthy: processes.filter((p) => p.state === 'unhealthy').length,
        terminating: processes.filter((p) => p.state === 'terminating').length,
      },
      byTier: {
        hot: processes.filter((p) => p.poolTier === 'hot').length,
        warm: processes.filter((p) => p.poolTier === 'warm').length,
        cold: processes.filter((p) => p.poolTier === 'cold').length,
      },
      totalRequests: processes.reduce((sum, p) => sum + p.requestCount, 0),
      totalErrors: processes.reduce((sum, p) => sum + p.errorCount, 0),
      avgMemoryMB:
        processes.reduce((sum, p) => sum + p.memoryMB, 0) / processes.length ||
        0,
    };
  }

  /**
   * Setup event handlers for a managed process
   */
  private setupProcessHandlers(managedProcess: ManagedProcess): void {
    const { process: childProcess, id } = managedProcess;

    childProcess.on('message', (message: any) => {
      if (message.type === 'ready') {
        managedProcess.state = 'idle';
        this.emit('process-ready', managedProcess);
      } else if (message.type === 'error') {
        managedProcess.errorCount++;
        if (
          managedProcess.errorCount >= this.config.maxErrorsBeforeTerminate
        ) {
          this.markUnhealthy(
            managedProcess,
            `Too many errors: ${managedProcess.errorCount}`
          );
        }
      }
    });

    childProcess.on('error', (error) => {
      console.error(`❌ ProcessPool: Process ${id} error:`, error);
      this.markUnhealthy(managedProcess, `Process error: ${error.message}`);
    });

    childProcess.on('exit', (code, signal) => {
      console.log(
        `🔄 ProcessPool: Process ${id} exited (code: ${code}, signal: ${signal})`
      );
      if (managedProcess.state !== 'terminating') {
        this.markUnhealthy(managedProcess, `Unexpected exit: ${code}`);
      }
      this.processes.delete(id);
    });
  }

  /**
   * Wait for process to send ready message
   */
  private waitForReady(managedProcess: ManagedProcess): Promise<void> {
    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        reject(new Error('Process ready timeout'));
      }, this.config.processTimeoutMs);

      const onReady = () => {
        clearTimeout(timeout);
        resolve();
      };

      managedProcess.process.once('message', (message: any) => {
        if (message.type === 'ready') {
          onReady();
        }
      });
    });
  }

  /**
   * Mark process as unhealthy
   */
  private markUnhealthy(managedProcess: ManagedProcess, reason: string): void {
    console.warn(
      `⚠️  ProcessPool: Process ${managedProcess.id} unhealthy: ${reason}`
    );
    managedProcess.state = 'unhealthy';
    this.emit('process-unhealthy', managedProcess, reason);

    // Auto-terminate unhealthy processes
    this.terminateProcess(managedProcess.id, `unhealthy: ${reason}`);
  }

  /**
   * Periodic health check for all processes
   */
  private async performHealthCheck(): Promise<void> {
    if (this.isShuttingDown) return;

    const now = Date.now();

    for (const managedProcess of this.processes.values()) {
      // Check for idle timeout
      const idleTime = now - managedProcess.lastUsed;
      if (
        managedProcess.state === 'idle' &&
        idleTime > this.config.maxIdleTimeMs
      ) {
        console.log(
          `🧹 ProcessPool: Evicting idle process ${managedProcess.id} (idle: ${idleTime}ms)`
        );
        await this.terminateProcess(managedProcess.id, 'idle-timeout');
        continue;
      }

      // Check request limit
      if (managedProcess.requestCount >= this.config.maxRequestsPerProcess) {
        console.log(
          `🧹 ProcessPool: Evicting process ${managedProcess.id} (max requests: ${managedProcess.requestCount})`
        );
        await this.terminateProcess(managedProcess.id, 'max-requests');
        continue;
      }
    }

    // Ensure minimum processes
    if (
      this.processes.size < this.config.minProcesses &&
      !this.isShuttingDown
    ) {
      console.log(
        `🔄 ProcessPool: Below minimum, spawning process (${this.processes.size}/${this.config.minProcesses})`
      );
      await this.spawnProcess('warm');
    }
  }

  /**
   * Generate unique process ID
   */
  private generateProcessId(): UUID {
    return `process-${Date.now()}-${Math.random().toString(36).substring(2, 10)}`;
  }
}
