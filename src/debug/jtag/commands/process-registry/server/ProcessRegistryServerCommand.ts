/**
 * Process Registry Server Command
 * 
 * Server-side implementation for process registry operations.
 * Handles file system access, process management, and port cleanup.
 */

import { promises as fs } from 'fs';
import * as path from 'path';
import { execSync } from 'child_process';
import { getRegistryDir, getRegistryFile } from '../../../system/core/registry/RegistryPath';
import { ProcessRegistryCommand } from '../shared/ProcessRegistryCommand';
import type { 
  RegisterProcessParams, 
  RegisterProcessResult,
  ListProcessesParams,
  ListProcessesResult,
  CleanupProcessesParams,
  CleanupProcessesResult,
  ProcessRegistryEntry
} from '../shared/ProcessRegistryTypes';
import { 
  PROCESS_REGISTRY_PORTS, 
  DEFAULT_NODE_IDS 
} from '../shared/ProcessRegistryTypes';
import { 
  validateRegisterProcessParams,
  getProcessCapabilities,
  generateProcessId
} from '../shared/ProcessRegistryCommand';

interface ProcessRegistryState {
  registryVersion: string;
  lastUpdate: number;
  processes: Record<string, ProcessRegistryEntry>;
}

export class ProcessRegistryServerCommand extends ProcessRegistryCommand {
  private registryDir = getRegistryDir();
  private registryFile = getRegistryFile();
  private registryState: ProcessRegistryState | null = null;

  /**
   * Register the current process in the global registry
   */
  async registerProcess(params: RegisterProcessParams): Promise<RegisterProcessResult> {
    try {
      // Validate parameters
      const validationError = validateRegisterProcessParams(params);
      if (validationError) {
        return {
          context: params.context,
          sessionId: params.sessionId,
          success: false,
          error: `Validation failed: ${validationError}`
        };
      }

      console.debug(`🏷️  Process Registry: Registering ${params.processType} process: ${params.description}`);
      
      // Ensure registry directory exists
      await this.ensureRegistryDir();
      
      // Load existing registry
      await this.loadRegistry();
      
      // Generate unique process ID
      const processId = generateProcessId();
      
      // Get system configuration for ports from context
      const instanceConfig = this.context.config.instance;
      const ports = params.ports && params.ports.length > 0 
        ? params.ports 
        : [
            instanceConfig.ports.websocket_server,
            instanceConfig.ports.http_server,
            PROCESS_REGISTRY_PORTS.DEFAULT_MULTICAST_PORT,
            PROCESS_REGISTRY_PORTS.DEFAULT_MULTICAST_PORT + PROCESS_REGISTRY_PORTS.DEFAULT_UNICAST_PORT_OFFSET
          ].filter(port => port > 0);

      // Create registry entry
      const entry: ProcessRegistryEntry = {
        processId,
        nodeId: DEFAULT_NODE_IDS.DEFAULT_NODE,
        pid: process.pid,
        ports,
        startTime: Date.now(),
        processType: params.processType,
        description: params.description,
        parentProcessId: params.parentProcessId,
        capabilities: params.capabilities ?? getProcessCapabilities(params.processType)
      };
      
      // Register the process
      await this.addProcessToRegistry(entry);
      
      // Set up cleanup on exit
      this.setupExitHandlers(processId);
      
      // console.debug(`✅ Process Registry: Registered ${params.processType} as ${processId}`);
      // console.debug(`🔍 Process Registry: Using ports [${entry.ports.join(', ')}]`);
      
      return {
        context: params.context,
        sessionId: params.sessionId,
        success: true,
        processId
      };

    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : String(error);
      console.error('❌ Process Registry: Registration failed:', errorMsg);
      
      return {
        context: params.context,
        sessionId: params.sessionId,
        success: false,
        error: errorMsg
      };
    }
  }

  /**
   * List active JTAG processes with optional filtering
   */
  async listProcesses(params: ListProcessesParams): Promise<ListProcessesResult> {
    try {
      await this.loadRegistry();
      
      if (!params.includeStale) {
        await this.cleanupStaleProcesses();
      }
      
      if (!this.registryState) {
        return {
          context: params.context,
          sessionId: params.sessionId,
          success: true,
          processes: []
        };
      }

      let processes = Object.values(this.registryState.processes);

      // Apply filters
      if (params.filterByPorts) {
        processes = processes.filter(proc => 
          proc.ports.some(port => params.filterByPorts!.includes(port))
        );
      }

      if (params.filterByType) {
        processes = processes.filter(proc => proc.processType === params.filterByType);
      }

      return {
        context: params.context,
        sessionId: params.sessionId,
        success: true,
        processes
      };

    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : String(error);
      console.error('❌ Process Registry: List failed:', errorMsg);
      
      return {
        context: params.context,
        sessionId: params.sessionId,
        success: false,
        processes: [],
        error: errorMsg
      };
    }
  }

  /**
   * Perform intelligent cleanup respecting process registry
   */
  async cleanupProcesses(params: CleanupProcessesParams): Promise<CleanupProcessesResult> {
    try {
      // console.debug('🧹 Smart Port Cleanup: Starting registry-aware cleanup...');
      
      const activeProcesses = await this.getActiveProcesses();
      // console.debug(`🔍 Found ${activeProcesses.length} active JTAG processes in registry`);
      
      // Determine which processes to preserve vs kill
      const { toKill, toPreserve } = this.categorizeProcesses(activeProcesses, params);

      const killedProcesses: ProcessRegistryEntry[] = [];
      const cleanedPorts: number[] = [];
      const errors: string[] = [];

      // Kill tmux sessions (safe to always clean)
      await this.cleanupTmuxSessions();

      // Handle JTAG processes
      for (const proc of toKill) {
        try {
          await this.killJTAGProcess(proc);
          killedProcesses.push(proc);
          cleanedPorts.push(...proc.ports);
        } catch (error) {
          const errorMsg = error instanceof Error ? error.message : String(error);
          errors.push(`Failed to kill JTAG process ${proc.processId}: ${errorMsg}`);
        }
      }

      // Handle non-JTAG processes on target ports  
      const instanceConfig = this.context.config.instance;
      const targetPorts = params.targetPorts || [
        instanceConfig.ports.websocket_server,
        instanceConfig.ports.http_server,
        PROCESS_REGISTRY_PORTS.DEFAULT_MULTICAST_PORT,  // Include multicast port for clean test isolation
        PROCESS_REGISTRY_PORTS.DEFAULT_MULTICAST_PORT + PROCESS_REGISTRY_PORTS.DEFAULT_UNICAST_PORT_OFFSET  // unicast port
      ].filter(port => port > 0);

      if (params.forceAll) {
        // Force kill all processes on target ports for clean test isolation
        for (const port of targetPorts) {
          try {
            execSync(`lsof -ti:${port} | xargs kill -9 2>/dev/null`, { stdio: 'ignore' });
            // console.debug(`💀 Force killed all processes on port ${port}`);
            cleanedPorts.push(port);
          } catch {
            // console.debug(`ℹ️  No processes found on port ${port}`);
          }
        }
      } else {
        // Only kill non-JTAG processes
        const jtagPorts = new Set<number>();
        activeProcesses.forEach(proc => proc.ports.forEach(port => jtagPorts.add(port)));

        for (const port of targetPorts) {
          if (jtagPorts.has(port)) {
            // console.debug(`⚠️  Preserving port ${port} (owned by JTAG process)`);
            continue;
          }

          try {
            execSync(`lsof -ti:${port} | xargs kill -9 2>/dev/null`, { stdio: 'ignore' });
            // console.debug(`✅ Cleaned non-JTAG processes on port ${port}`);
            cleanedPorts.push(port);
          } catch {
            // console.debug(`ℹ️  No non-JTAG processes found on port ${port}`);
          }
        }
      }

      // Log results
      // console.debug(`🎉 Smart cleanup complete:`);
      // console.debug(`  • Killed processes: ${killedProcesses.length}`);
      // console.debug(`  • Preserved processes: ${toPreserve.length}`);
      // console.debug(`  • Cleaned ports: [${cleanedPorts.join(', ')}]`);

      if (toPreserve.length > 0) {
        // console.debug(`📋 Active JTAG processes preserved:`);
        toPreserve.forEach(proc => {
          // console.debug(`  • ${proc.processId}: ${proc.description} (ports: ${proc.ports.join(', ')})`);
        });
      }

      return {
        context: params.context,
        sessionId: params.sessionId,
        success: errors.length === 0,
        killedProcesses,
        preservedProcesses: toPreserve,
        cleanedPorts,
        errors
      };

    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : String(error);
      console.error('❌ Smart Port Cleanup failed:', errorMsg);
      
      return {
        context: params.context,
        sessionId: params.sessionId,
        success: false,
        killedProcesses: [],
        preservedProcesses: [],
        cleanedPorts: [],
        errors: [errorMsg]
      };
    }
  }

  private async getActiveProcesses(): Promise<ProcessRegistryEntry[]> {
    // Import secure context creation
    const { createJTAGConfig } = require('../../../system/shared/BrowserSafeConfig');
    const { createServerContext } = require('../../../system/core/context/SecureJTAGContext');
    const jtagConfig = createJTAGConfig();
    const context = createServerContext(jtagConfig, 'internal-process-registry');
    
    const result = await this.listProcesses({
      context,
      sessionId: 'internal' as any,
      includeStale: false
    });
    return [...result.processes]; // Convert readonly array to mutable
  }

  private categorizeProcesses(
    processes: ProcessRegistryEntry[],
    params: CleanupProcessesParams
  ): { toKill: ProcessRegistryEntry[]; toPreserve: ProcessRegistryEntry[] } {
    const toKill: ProcessRegistryEntry[] = [];
    const toPreserve: ProcessRegistryEntry[] = [];

    for (const proc of processes) {
      let shouldKill = false;

      if (params.forceAll) {
        shouldKill = true;
      } else if (params.targetProcessId && proc.processId === params.targetProcessId) {
        shouldKill = true;
      } else if (params.targetPorts && proc.ports.some(port => params.targetPorts!.includes(port))) {
        shouldKill = !params.preserveActive;
      } else {
        shouldKill = !params.preserveActive;
      }

      if (shouldKill) {
        toKill.push(proc);
      } else {
        toPreserve.push(proc);
      }
    }

    return { toKill, toPreserve };
  }

  private async killJTAGProcess(proc: ProcessRegistryEntry): Promise<void> {
    // console.debug(`🎯 Killing JTAG process ${proc.processId} (${proc.description}) PID ${proc.pid}`);
    
    // Kill the process
    process.kill(proc.pid, 'SIGTERM');
    
    // Give it time to cleanup gracefully
    await new Promise(resolve => setTimeout(resolve, 1000));
    
    // Force kill if still running
    try {
      process.kill(proc.pid, 0); // Check if still exists
      process.kill(proc.pid, 'SIGKILL');
      // console.debug(`💀 Force killed JTAG process ${proc.processId}`);
    } catch {
      // Process already exited
    }
  }

  private async cleanupTmuxSessions(): Promise<void> {
    try {
      execSync('tmux kill-session -t jtag-test 2>/dev/null', { stdio: 'ignore' });
      // console.debug('✅ Tmux session jtag-test killed');
    } catch {
      // console.debug('ℹ️  No tmux session jtag-test found');
    }

    // Kill npm start processes (usually safe)
    try {
      execSync(`pkill -f 'npm.*start' 2>/dev/null`, { stdio: 'ignore' });
      // console.debug('✅ npm start processes killed');
    } catch {
      // console.debug('ℹ️  No npm start processes found');
    }
  }

  private async addProcessToRegistry(entry: ProcessRegistryEntry): Promise<void> {
    if (!this.registryState) {
      this.registryState = {
        registryVersion: '1.0.0',
        lastUpdate: Date.now(),
        processes: {}
      };
    }

    // Clean up stale processes before adding new one
    await this.cleanupStaleProcesses();

    // Add this process
    this.registryState.processes[entry.processId] = entry;
    this.registryState.lastUpdate = Date.now();

    // Save to disk
    await this.saveRegistry();
  }

  private async ensureRegistryDir(): Promise<void> {
    try {
      await fs.mkdir(this.registryDir, { recursive: true });
    } catch {
      // Directory might already exist
    }
  }

  private async loadRegistry(): Promise<void> {
    try {
      const content = await fs.readFile(this.registryFile, 'utf8');
      this.registryState = JSON.parse(content);
    } catch {
      // Registry doesn't exist yet or is corrupted
      this.registryState = {
        registryVersion: '1.0.0',
        lastUpdate: Date.now(),
        processes: {}
      };
    }
  }

  private async saveRegistry(): Promise<void> {
    if (!this.registryState) return;

    const content = JSON.stringify(this.registryState, null, 2);
    await fs.writeFile(this.registryFile, content, 'utf8');
  }

  private async cleanupStaleProcesses(): Promise<void> {
    if (!this.registryState) return;

    const activeProcessIds: string[] = [];
    
    for (const [processId, entry] of Object.entries(this.registryState.processes)) {
      try {
        // Check if process is still running
        process.kill(entry.pid, 0); // Signal 0 just checks if process exists
        activeProcessIds.push(processId);
      } catch {
        // Process is no longer running
        // console.debug(`🧹 Process Registry: Cleaned up stale process ${processId} (PID ${entry.pid})`);
      }
    }

    // Keep only active processes
    const newProcesses: Record<string, ProcessRegistryEntry> = {};
    activeProcessIds.forEach(id => {
      newProcesses[id] = this.registryState!.processes[id];
    });

    this.registryState.processes = newProcesses;
    this.registryState.lastUpdate = Date.now();
  }

  private setupExitHandlers(processId: string): void {
    const cleanup = async () => {
      try {
        if (this.registryState && this.registryState.processes[processId]) {
          delete this.registryState.processes[processId];
          this.registryState.lastUpdate = Date.now();
          await this.saveRegistry();
          // console.debug(`🧹 Process Registry: Unregistered ${processId}`);
        }
      } catch {
        // Best effort cleanup
      }
    };

    process.on('exit', cleanup);
    process.on('SIGINT', cleanup);
    process.on('SIGTERM', cleanup);
    process.on('uncaughtException', cleanup);
    process.on('unhandledRejection', cleanup);
  }
}