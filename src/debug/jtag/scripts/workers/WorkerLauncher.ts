/**
 * Modular Worker Launcher - Auto-discovery, parallel build, health monitoring
 *
 * Discovers Rust workers, builds them efficiently, and manages their lifecycle.
 */

import { spawn, execSync, ChildProcess } from 'child_process';
import * as fs from 'fs';
import * as path from 'path';
import {
  discoverWorkers,
  getWorkerBinaryPath,
  getWorkerPid,
  workerBinaryExists,
  workerSocketExists,
  isWorkerRunning,
  type WorkerConfig,
  type WorkerRegistry,
} from './WorkerRegistry';

interface WorkerProcess {
  config: WorkerConfig;
  process: ChildProcess | null;
  pid: number | null;
  startTime: Date | null;
  restartCount: number;
}

export class WorkerLauncher {
  private registry: WorkerRegistry;
  private processes = new Map<string, WorkerProcess>();
  private workersDir: string;

  constructor(workersDir: string) {
    this.workersDir = workersDir;
    this.registry = discoverWorkers(workersDir);
    // No shutdown handlers - workers run detached and independently
  }

  /**
   * Build all workers in dependency order with parallel compilation where possible
   */
  async buildAll(verbose = false): Promise<void> {
    console.log('🔨 Building Rust workers...\n');

    const { buildOrder, workers } = this.registry;

    // Group workers by dependency level for parallel building
    const levels: string[][] = [];
    const level = new Map<string, number>();

    for (const name of buildOrder) {
      const worker = workers.get(name)!;
      const depLevel = Math.max(
        0,
        ...worker.dependencies.map((dep) => (level.get(dep) || 0) + 1)
      );
      level.set(name, depLevel);

      if (!levels[depLevel]) levels[depLevel] = [];
      levels[depLevel].push(name);
    }

    // Build each level in parallel
    for (let i = 0; i < levels.length; i++) {
      const levelWorkers = levels[i];
      console.log(`📦 Building level ${i + 1}: ${levelWorkers.join(', ')}`);

      await Promise.all(
        levelWorkers.map((name) => this.buildWorker(workers.get(name)!, verbose))
      );
    }

    console.log('\n✅ All workers built successfully\n');
  }

  /**
   * Build a single worker
   */
  private async buildWorker(config: WorkerConfig, verbose: boolean): Promise<void> {
    return new Promise((resolve, reject) => {
      const startTime = Date.now();

      try {
        const result = execSync('cargo build --release', {
          cwd: config.workingDir,
          encoding: 'utf-8',
          stdio: verbose ? 'inherit' : 'pipe',
        });

        const duration = ((Date.now() - startTime) / 1000).toFixed(2);
        console.log(`  ✅ ${config.displayName} built in ${duration}s`);
        resolve();
      } catch (error: any) {
        console.error(`  ❌ ${config.displayName} build failed:`);
        if (!verbose && error.stdout) console.error(error.stdout);
        if (!verbose && error.stderr) console.error(error.stderr);
        reject(error);
      }
    });
  }

  /**
   * Start all workers in dependency order
   */
  async startAll(): Promise<void> {
    console.log('🚀 Starting workers in dependency order...\n');

    const { buildOrder, workers } = this.registry;

    for (const name of buildOrder) {
      const config = workers.get(name)!;
      await this.startWorker(config);

      // Wait a bit for dependent workers (e.g., logger needs to be ready)
      if (config.dependencies.length === 0) {
        await this.waitForWorkerReady(config, 5000);
      }
    }

    console.log('\n✅ All workers started successfully\n');
  }

  /**
   * Start a single worker
   */
  private async startWorker(config: WorkerConfig): Promise<void> {
    // Check if already running
    if (isWorkerRunning(config)) {
      console.log(`  ⚠️  ${config.displayName} already running`);
      return;
    }

    // Remove stale socket
    if (workerSocketExists(config)) {
      fs.unlinkSync(config.socketPath);
    }

    // Check binary exists
    const binaryPath = getWorkerBinaryPath(config);
    if (!workerBinaryExists(config)) {
      throw new Error(`${config.displayName} binary not found at ${binaryPath}`);
    }

    // Spawn worker process (detached so it runs independently)
    const proc = spawn(binaryPath, [config.socketPath], {
      cwd: config.workingDir,
      stdio: 'ignore', // Workers log via LoggerWorker
      detached: true,   // Run independently of launcher
    });

    // Unref so parent process can exit
    proc.unref();

    const workerProcess: WorkerProcess = {
      config,
      process: proc,
      pid: proc.pid || null,
      startTime: new Date(),
      restartCount: 0,
    };

    this.processes.set(config.name, workerProcess);

    console.log(`  ✅ ${config.displayName} started (PID: ${proc.pid})`);
  }

  /**
   * Wait for worker to be ready (socket exists)
   */
  private async waitForWorkerReady(config: WorkerConfig, timeout: number): Promise<void> {
    const startTime = Date.now();

    while (Date.now() - startTime < timeout) {
      if (workerSocketExists(config)) {
        return;
      }
      await new Promise((resolve) => setTimeout(resolve, 100));
    }

    throw new Error(`${config.displayName} failed to create socket within ${timeout}ms`);
  }

  /**
   * Stop all workers gracefully
   */
  async stopAll(): Promise<void> {
    console.log('\n🛑 Stopping all workers...\n');

    const { buildOrder, workers } = this.registry;

    // Stop in reverse dependency order
    for (let i = buildOrder.length - 1; i >= 0; i--) {
      const name = buildOrder[i];
      const config = workers.get(name)!;
      await this.stopWorker(config);
    }

    console.log('\n✅ All workers stopped\n');
  }

  /**
   * Stop a single worker
   */
  private async stopWorker(config: WorkerConfig): Promise<void> {
    const workerProcess = this.processes.get(config.name);

    if (workerProcess?.process && workerProcess.pid) {
      try {
        process.kill(workerProcess.pid, 'SIGTERM');
        console.log(`  ✅ ${config.displayName} stopped (PID: ${workerProcess.pid})`);
      } catch (error: any) {
        if (error.code !== 'ESRCH') {
          // ESRCH = process doesn't exist, which is fine
          console.error(`  ⚠️  ${config.displayName} stop failed:`, error.message);
        }
      }
    }

    // Clean up socket
    if (workerSocketExists(config)) {
      try {
        fs.unlinkSync(config.socketPath);
      } catch (error: any) {
        console.error(`  ⚠️  Failed to remove socket ${config.socketPath}:`, error.message);
      }
    }

    this.processes.delete(config.name);
  }

  /**
   * Get worker status (checks ALL workers in registry, not just ones we started)
   */
  getStatus(): Array<{ name: string; running: boolean; pid: number | null; uptime: string }> {
    const status: Array<{ name: string; running: boolean; pid: number | null; uptime: string }> = [];

    // Check all workers in registry (not just processes we started in this instance)
    for (const [name, config] of this.registry.workers) {
      const running = isWorkerRunning(config);
      const pid = getWorkerPid(config);

      // Check if we have process info (started in this instance)
      const workerProcess = this.processes.get(name);
      const uptime = workerProcess?.startTime && running
        ? this.formatUptime(Date.now() - workerProcess.startTime.getTime())
        : 'N/A';

      status.push({
        name: config.displayName,
        running,
        pid,
        uptime: running ? uptime : 'N/A',
      });
    }

    return status;
  }

  /**
   * Format uptime in human-readable format
   */
  private formatUptime(ms: number): string {
    const seconds = Math.floor(ms / 1000);
    const minutes = Math.floor(seconds / 60);
    const hours = Math.floor(minutes / 60);

    if (hours > 0) return `${hours}h ${minutes % 60}m`;
    if (minutes > 0) return `${minutes}m ${seconds % 60}s`;
    return `${seconds}s`;
  }

  /**
   * Print worker registry info
   */
  printRegistry(): void {
    console.log('📋 Worker Registry:\n');

    for (const name of this.registry.buildOrder) {
      const config = this.registry.workers.get(name)!;
      console.log(`  ${config.displayName}`);
      console.log(`    Binary: ${config.binaryName}`);
      console.log(`    Socket: ${config.socketPath}`);
      console.log(`    Required: ${config.required ? 'Yes' : 'No'}`);
      console.log(`    Dependencies: ${config.dependencies.join(', ') || 'None'}`);
      console.log();
    }
  }
}
