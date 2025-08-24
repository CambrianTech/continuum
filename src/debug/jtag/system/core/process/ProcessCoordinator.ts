// ISSUES: 0 open, last updated 2025-08-23 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking

/**
 * ProcessCoordinator - Intelligent JTAG Process Management
 * 
 * Smart detection, graceful handoff, zero conflicts.
 * Eliminates multiple process startup issues with type-safe coordination.
 */

import { spawn, ChildProcess } from 'child_process';
import { readFileSync, writeFileSync, existsSync, unlinkSync } from 'fs';
import { resolve } from 'path';
import type { ExampleName } from '../../shared/ExampleConfigTypes';

export interface ProcessState {
  readonly pid: number;
  readonly ports: readonly number[];
  readonly context: string;
  readonly startTime: Date;
  readonly healthStatus: 'healthy' | 'degraded' | 'failed';
}

export interface ProcessLock {
  readonly lockId: string;
  readonly acquiredBy: string;
  readonly acquiredAt: Date;
  release(): Promise<void>;
}

export type ProcessTransition = 
  | { type: 'reuse_existing'; process: ProcessState }
  | { type: 'graceful_handoff'; from: ProcessState; to: ProcessState }
  | { type: 'clean_start'; context: string };

export class ProcessCoordinator {
  private static instance: ProcessCoordinator;
  private readonly lockFile = '.continuum/jtag/system/startup.lock';
  private readonly pidFile = '.continuum/jtag/system/server.pid';

  private constructor() {}

  static getInstance(): ProcessCoordinator {
    if (!ProcessCoordinator.instance) {
      ProcessCoordinator.instance = new ProcessCoordinator();
    }
    return ProcessCoordinator.instance;
  }

  /**
   * Smart process detection - check if server already running on ports
   */
  async detectExistingServer(ports: readonly number[]): Promise<ProcessState | null> {
    if (!existsSync(this.pidFile)) {
      return null;
    }

    try {
      const pidData = JSON.parse(readFileSync(this.pidFile, 'utf8'));
      const pid = pidData.pid;

      // Check if process is still alive
      try {
        process.kill(pid, 0); // Signal 0 checks if process exists
      } catch (error) {
        // Process dead, cleanup stale PID file
        unlinkSync(this.pidFile);
        return null;
      }

      // Check if ports are actually active
      const activePorts = await this.checkActivePorts(ports);
      if (activePorts.length === 0) {
        // PID exists but no ports active - process may be starting up
        return {
          pid,
          ports: [],
          context: pidData.context || 'unknown',
          startTime: new Date(pidData.startTime || Date.now()),
          healthStatus: 'degraded'
        };
      }

      return {
        pid,
        ports: activePorts,
        context: pidData.context || 'unknown', 
        startTime: new Date(pidData.startTime || Date.now()),
        healthStatus: activePorts.length === ports.length ? 'healthy' : 'degraded'
      };

    } catch (error) {
      console.warn(`ProcessCoordinator: Failed to read PID file: ${error}`);
      return null;
    }
  }

  /**
   * Acquire startup lock to prevent multiple simultaneous starts
   */
  async acquireStartupLock(context: string): Promise<ProcessLock> {
    const lockId = `${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
    const lockData = {
      lockId,
      acquiredBy: `ProcessCoordinator-${process.pid}`,
      acquiredAt: new Date().toISOString(),
      context
    };

    // Simple file-based locking (could be improved with proper file locking)
    let attempts = 0;
    const maxAttempts = 30; // 30 seconds max wait

    while (attempts < maxAttempts) {
      if (!existsSync(this.lockFile)) {
        try {
          writeFileSync(this.lockFile, JSON.stringify(lockData));
          console.log(`🔒 ProcessCoordinator: Acquired startup lock (${lockId})`);
          
          return {
            lockId,
            acquiredBy: lockData.acquiredBy,
            acquiredAt: new Date(lockData.acquiredAt),
            release: async () => {
              try {
                const currentLock = JSON.parse(readFileSync(this.lockFile, 'utf8'));
                if (currentLock.lockId === lockId) {
                  unlinkSync(this.lockFile);
                  console.log(`🔓 ProcessCoordinator: Released startup lock (${lockId})`);
                }
              } catch (error) {
                console.warn(`ProcessCoordinator: Failed to release lock: ${error}`);
              }
            }
          };
        } catch (error) {
          // Lock file creation failed, someone else got it
        }
      } else {
        // Intelligent stale lock detection and recovery
        try {
          const existingLock = JSON.parse(readFileSync(this.lockFile, 'utf8'));
          const lockAge = Date.now() - new Date(existingLock.acquiredAt).getTime();
          const lockingProcessPidMatch = existingLock.acquiredBy.match(/ProcessCoordinator-(\d+)/);
          
          // Multi-layered stale lock detection
          let isStaleLock = false;
          let staleReason = '';
          
          // Check 1: Lock age (older than 5 minutes is suspicious)
          if (lockAge > 5 * 60 * 1000) {
            isStaleLock = true;
            staleReason = `age=${Math.round(lockAge/1000)}s`;
          }
          
          // Check 2: Process existence (most reliable)
          if (lockingProcessPidMatch && !isStaleLock) {
            const lockingPid = parseInt(lockingProcessPidMatch[1], 10);
            try {
              process.kill(lockingPid, 0); // Signal 0 just checks if process exists
              console.log(`⏳ ProcessCoordinator: Valid lock held by running process ${lockingPid} (${lockingProcessPidMatch}), waiting...`);
            } catch (error) {
              isStaleLock = true;
              staleReason = `dead_process=${lockingPid}`;
            }
          }
          
          // Check 3: Lock format validation
          if (!existingLock.lockId || !existingLock.acquiredBy || !existingLock.acquiredAt) {
            isStaleLock = true;
            staleReason = 'malformed_lock';
          }
          
          if (isStaleLock) {
            console.warn(`🧹 ProcessCoordinator: Removing stale lock (${staleReason}) from: ${existingLock.acquiredBy || 'unknown'}`);
            unlinkSync(this.lockFile);
            continue; // Immediately try to acquire lock on next iteration
          }
          
        } catch (error) {
          // Failed to read/parse lock file - treat as corrupted and remove
          console.warn(`🧹 ProcessCoordinator: Removing corrupted lock file: ${error.message}`);
          try {
            unlinkSync(this.lockFile);
            continue; // Immediately try to acquire lock on next iteration  
          } catch (unlinkError) {
            console.error(`❌ ProcessCoordinator: Failed to remove corrupted lock: ${unlinkError.message}`);
          }
        }
      }

      // Wait and retry
      await new Promise(resolve => setTimeout(resolve, 1000));
      attempts++;
    }

    throw new Error('ProcessCoordinator: Failed to acquire startup lock after 30 seconds');
  }

  /**
   * Intelligent startup decision
   */
  async planStartup(targetContext: string, targetPorts: readonly number[]): Promise<ProcessTransition> {
    const existing = await this.detectExistingServer(targetPorts);

    if (!existing) {
      return { type: 'clean_start', context: targetContext };
    }

    if (existing.context === targetContext && existing.healthStatus === 'healthy') {
      return { type: 'reuse_existing', process: existing };
    }

    // Different context or unhealthy - need transition
    const newProcess: ProcessState = {
      pid: 0, // Will be set during startup
      ports: targetPorts,
      context: targetContext,
      startTime: new Date(),
      healthStatus: 'healthy'
    };

    return { type: 'graceful_handoff', from: existing, to: newProcess };
  }

  /**
   * Save process metadata
   */
  async saveProcessState(state: ProcessState): Promise<void> {
    const pidData = {
      pid: state.pid,
      context: state.context,
      startTime: state.startTime.toISOString(),
      ports: state.ports
    };

    writeFileSync(this.pidFile, JSON.stringify(pidData, null, 2));
    console.log(`💾 ProcessCoordinator: Saved process state (PID: ${state.pid})`);
  }

  /**
   * Check which ports are actually active
   */
  private async checkActivePorts(ports: readonly number[]): Promise<number[]> {
    const activePorts: number[] = [];
    
    for (const port of ports) {
      try {
        // Try to connect to the port
        const net = await import('net');
        const socket = new net.Socket();
        
        const isActive = await new Promise<boolean>((resolve) => {
          const timeout = setTimeout(() => {
            socket.destroy();
            resolve(false);
          }, 1000);

          socket.connect(port, 'localhost', () => {
            clearTimeout(timeout);
            socket.destroy();
            resolve(true);
          });

          socket.on('error', () => {
            clearTimeout(timeout);
            resolve(false);
          });
        });

        if (isActive) {
          activePorts.push(port);
        }
      } catch (error) {
        // Port check failed, assume inactive
      }
    }

    return activePorts;
  }
}