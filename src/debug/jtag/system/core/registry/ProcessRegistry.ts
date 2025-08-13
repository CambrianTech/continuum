/**
 * Process Registry - Global process identification and tracking system
 * 
 * Enables JTAG processes to:
 * 1. Mark themselves with unique identifiers
 * 2. Discover other JTAG processes safely
 * 3. Coordinate cleanup without killing active systems
 * 4. Support P2P mesh networking identification
 */

import { promises as fs } from 'fs';
import * as path from 'path';
import * as crypto from 'crypto';
import { getSystemConfig } from '../config/SystemConfiguration';

export type ProcessType = 'server' | 'browser' | 'test' | 'client';
export type ProcessCapability = 
  | 'websocket-server' 
  | 'command-execution' 
  | 'file-operations' 
  | 'console-logging'
  | 'screenshot' 
  | 'dom-interaction' 
  | 'browser-automation' 
  | 'console-forwarding'
  | 'test-execution' 
  | 'process-spawning' 
  | 'validation'
  | 'command-sending' 
  | 'result-receiving';

export interface ProcessRegistryEntry {
  readonly processId: string;        // Unique GUID for this process
  readonly nodeId: string;           // JTAG node identifier  
  readonly pid: number;              // System process ID
  readonly ports: readonly number[]; // Ports this process is using
  readonly startTime: number;        // Process start timestamp
  readonly processType: ProcessType;
  readonly description: string;      // Human-readable description
  readonly parentProcessId?: string; // Parent JTAG process (if any)
  readonly capabilities: readonly ProcessCapability[]; // What this process can do
}

export interface ProcessRegistryState {
  readonly registryVersion: string;
  readonly lastUpdate: number;
  readonly processes: Record<string, ProcessRegistryEntry>;
}

export class ProcessRegistry {
  private static _instance: ProcessRegistry | null = null;
  private registryDir = '.continuum/jtag/registry';
  private registryFile = path.join(this.registryDir, 'process-registry.json');
  private processId: string;
  private registryState: ProcessRegistryState | null = null;

  private constructor() {
    // Generate unique process ID for this instance
    this.processId = this.generateProcessId();
  }

  /**
   * Get singleton instance
   */
  static getInstance(): ProcessRegistry {
    if (!ProcessRegistry._instance) {
      ProcessRegistry._instance = new ProcessRegistry();
    }
    return ProcessRegistry._instance;
  }

  /**
   * Initialize process registry and register current process
   */
  async initialize(processType: ProcessType, description: string): Promise<string> {
    try {
      console.log(`🏷️  Process Registry: Initializing ${processType} process: ${description}`);
      
      // Ensure registry directory exists
      await this.ensureRegistryDir();
      
      // Load existing registry
      await this.loadRegistry();
      
      // Register this process
      const systemConfig = getSystemConfig();
      const entry: ProcessRegistryEntry = {
        processId: this.processId,
        nodeId: systemConfig.getNodeId(),
        pid: process.pid,
        ports: [
          systemConfig.getWebSocketPort(),
          systemConfig.getHTTPPort(),
          systemConfig.getUDPConfig().port,
          systemConfig.getUDPConfig().unicastPort
        ].filter(port => port > 0),
        startTime: Date.now(),
        processType,
        description,
        capabilities: this.getProcessCapabilities(processType)
      };
      
      await this.registerProcess(entry);
      
      // Set up cleanup on exit
      this.setupExitHandlers();
      
      console.log(`✅ Process Registry: Registered ${processType} as ${this.processId}`);
      console.log(`🔍 Process Registry: Using ports [${entry.ports.join(', ')}]`);
      
      return this.processId;
      
    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : String(error);
      console.error('❌ Process Registry: Initialization failed:', errorMsg);
      throw error;
    }
  }

  /**
   * Register a process in the global registry
   */
  private async registerProcess(entry: ProcessRegistryEntry): Promise<void> {
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

  /**
   * Get all active JTAG processes
   */
  async getActiveProcesses(): Promise<ProcessRegistryEntry[]> {
    await this.loadRegistry();
    await this.cleanupStaleProcesses();
    
    if (!this.registryState) {
      return [];
    }

    return Object.values(this.registryState.processes);
  }

  /**
   * Get processes using specific ports
   */
  async getProcessesByPorts(ports: number[]): Promise<ProcessRegistryEntry[]> {
    const activeProcesses = await this.getActiveProcesses();
    
    return activeProcesses.filter(proc => 
      proc.ports.some(port => ports.includes(port))
    );
  }

  /**
   * Check if a port is owned by a JTAG process
   */
  async isPortOwnedByJTAG(port: number): Promise<ProcessRegistryEntry | null> {
    const processes = await this.getProcessesByPorts([port]);
    return processes.length > 0 ? processes[0] : null;
  }

  /**
   * Get current process ID
   */
  getProcessId(): string {
    return this.processId;
  }

  /**
   * Unregister this process (called on exit)
   */
  async unregister(): Promise<void> {
    try {
      if (!this.registryState) {
        await this.loadRegistry();
      }

      if (this.registryState && this.registryState.processes[this.processId]) {
        delete this.registryState.processes[this.processId];
        this.registryState.lastUpdate = Date.now();
        await this.saveRegistry();
        console.log(`🧹 Process Registry: Unregistered ${this.processId}`);
      }
    } catch (error) {
      console.error('❌ Process Registry: Unregister failed:', error);
    }
  }

  /**
   * Generate unique process identifier
   */
  private generateProcessId(): string {
    const timestamp = Date.now().toString(36);
    const random = crypto.randomBytes(8).toString('hex');
    const pid = process.pid.toString(36);
    return `jtag-${timestamp}-${pid}-${random}`;
  }

  /**
   * Get capabilities based on process type
   */
  private getProcessCapabilities(processType: ProcessType): ProcessCapability[] {
    const capabilities: Record<ProcessType, ProcessCapability[]> = {
      server: ['websocket-server', 'command-execution', 'file-operations', 'console-logging'],
      browser: ['screenshot', 'dom-interaction', 'browser-automation', 'console-forwarding'],
      test: ['test-execution', 'process-spawning', 'validation'],
      client: ['command-sending', 'result-receiving']
    };

    return capabilities[processType] || [];
  }

  /**
   * Ensure registry directory exists
   */
  private async ensureRegistryDir(): Promise<void> {
    try {
      await fs.mkdir(this.registryDir, { recursive: true });
    } catch (error) {
      // Directory might already exist
    }
  }

  /**
   * Load registry from disk
   */
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

  /**
   * Save registry to disk
   */
  private async saveRegistry(): Promise<void> {
    if (!this.registryState) return;

    const content = JSON.stringify(this.registryState, null, 2);
    await fs.writeFile(this.registryFile, content, 'utf8');
  }

  /**
   * Clean up processes that are no longer running
   */
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
        console.log(`🧹 Process Registry: Cleaned up stale process ${processId} (PID ${entry.pid})`);
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

  /**
   * Set up process exit handlers for cleanup
   */
  private setupExitHandlers(): void {
    const cleanup = () => {
      this.unregister().catch(() => {
        // Best effort cleanup
      });
    };

    process.on('exit', cleanup);
    process.on('SIGINT', cleanup);
    process.on('SIGTERM', cleanup);
    process.on('uncaughtException', cleanup);
    process.on('unhandledRejection', cleanup);
  }
}

/**
 * Convenience function to get process registry instance
 */
export function getProcessRegistry(): ProcessRegistry {
  return ProcessRegistry.getInstance();
}

/**
 * Initialize process registry for current process
 */
export async function initializeProcessRegistry(
  processType: ProcessType, 
  description: string
): Promise<string> {
  const registry = getProcessRegistry();
  return registry.initialize(processType, description);
}