/**
 * Worker Registry - Auto-discovery and management of Rust workers
 *
 * Scans workers/ directory for Cargo.toml files and builds a registry
 * of available workers with their metadata and dependencies.
 */

import * as fs from 'fs';
import * as path from 'path';
import { execSync } from 'child_process';

export interface WorkerConfig {
  name: string;
  displayName: string;
  binaryName: string;
  socketPath: string;
  workingDir: string;
  dependencies: string[];
  required: boolean;
  healthCheck: {
    enabled: boolean;
    pingCommand?: string;
  };
}

export interface WorkerRegistry {
  workers: Map<string, WorkerConfig>;
  buildOrder: string[];
}

/**
 * Parse Cargo.toml to extract worker metadata
 */
function parseCargoToml(filePath: string): Partial<WorkerConfig> | null {
  try {
    const content = fs.readFileSync(filePath, 'utf-8');
    const lines = content.split('\n');

    let binaryName: string | null = null;
    let packageName: string | null = null;

    // Parse [[bin]] section for binary name
    for (let i = 0; i < lines.length; i++) {
      const line = lines[i].trim();

      if (line.startsWith('name = ') && lines[i - 1]?.trim() === '[[bin]]') {
        binaryName = line.match(/name = "(.+)"/)?.[1] || null;
      }

      if (line.startsWith('name = ') && !binaryName) {
        packageName = line.match(/name = "(.+)"/)?.[1] || null;
      }
    }

    return {
      binaryName: binaryName || packageName || 'unknown',
    };
  } catch (error) {
    console.error(`Failed to parse ${filePath}:`, error);
    return null;
  }
}

/**
 * Discover all Rust workers in workers/ directory
 */
export function discoverWorkers(workersDir: string): WorkerRegistry {
  const workers = new Map<string, WorkerConfig>();

  // Scan workers directory for Cargo.toml files
  const entries = fs.readdirSync(workersDir, { withFileTypes: true });

  for (const entry of entries) {
    if (!entry.isDirectory()) continue;
    if (entry.name === 'shared') continue; // Skip shared code directory

    const cargoPath = path.join(workersDir, entry.name, 'Cargo.toml');
    if (!fs.existsSync(cargoPath)) continue;

    const metadata = parseCargoToml(cargoPath);
    if (!metadata) continue;

    // Worker configuration with sensible defaults
    const workerName = entry.name;

    const config: WorkerConfig = {
      name: workerName,
      displayName: workerName.charAt(0).toUpperCase() + workerName.slice(1) + 'Worker',
      binaryName: metadata.binaryName || `${workerName}-worker`,
      socketPath: `/tmp/jtag-${workerName}-worker.sock`, // All workers use jtag- prefix for namespace safety
      workingDir: path.join(workersDir, workerName),
      dependencies: workerName === 'training' ? ['logger'] : [], // Training depends on logger for logging
      required: workerName === 'logger', // Logger is required for other workers
      healthCheck: {
        enabled: true,
        pingCommand: workerName === 'logger' ? undefined : 'ping', // Logger doesn't have ping
      },
    };

    workers.set(workerName, config);
  }

  // Calculate build order based on dependencies
  const buildOrder = calculateBuildOrder(workers);

  return { workers, buildOrder };
}

/**
 * Calculate build order using topological sort (dependencies first)
 */
function calculateBuildOrder(workers: Map<string, WorkerConfig>): string[] {
  const order: string[] = [];
  const visited = new Set<string>();
  const visiting = new Set<string>();

  function visit(name: string) {
    if (visited.has(name)) return;
    if (visiting.has(name)) {
      throw new Error(`Circular dependency detected: ${name}`);
    }

    visiting.add(name);
    const worker = workers.get(name);

    if (worker) {
      // Visit dependencies first
      for (const dep of worker.dependencies) {
        if (workers.has(dep)) {
          visit(dep);
        }
      }
    }

    visiting.delete(name);
    visited.add(name);
    order.push(name);
  }

  // Visit all workers
  for (const name of workers.keys()) {
    visit(name);
  }

  return order;
}

/**
 * Get worker binary path
 */
export function getWorkerBinaryPath(config: WorkerConfig): string {
  return path.join(config.workingDir, 'target', 'release', config.binaryName);
}

/**
 * Check if worker binary exists
 */
export function workerBinaryExists(config: WorkerConfig): boolean {
  return fs.existsSync(getWorkerBinaryPath(config));
}

/**
 * Check if worker socket exists
 */
export function workerSocketExists(config: WorkerConfig): boolean {
  try {
    const stats = fs.statSync(config.socketPath);
    return stats.isSocket();
  } catch {
    return false;
  }
}

/**
 * Get worker process ID by socket
 */
export function getWorkerPid(config: WorkerConfig): number | null {
  try {
    const result = execSync(`lsof -t ${config.socketPath} 2>/dev/null || true`, {
      encoding: 'utf-8',
      stdio: ['pipe', 'pipe', 'ignore'],
    }).trim();

    return result ? parseInt(result, 10) : null;
  } catch {
    return null;
  }
}

/**
 * Check if worker process is running
 */
export function isWorkerRunning(config: WorkerConfig): boolean {
  const pid = getWorkerPid(config);
  if (!pid) return false;

  try {
    // Check if process exists (kill -0 doesn't actually kill)
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}
