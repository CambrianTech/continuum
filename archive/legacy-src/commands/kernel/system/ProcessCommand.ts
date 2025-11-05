/**
 * Process Command - Low-level process management for system daemons
 * 
 * Provides kernel-level process operations that daemons need:
 * - Process discovery and enumeration  
 * - Process lifecycle management (spawn, kill, monitor)
 * - Port allocation and conflict resolution
 * - Resource monitoring (CPU, memory, file descriptors)
 * - Process tree analysis and cleanup
 */

import { BaseCommand } from '../../core/base-command/BaseCommand';
import { CommandResult } from '../../core/base-command/BaseCommand';
import { COMMAND_CATEGORIES } from '../../../types/shared/CommandTypes';
import { spawn, exec } from 'child_process';
import { promisify } from 'util';

const execAsync = promisify(exec);

export interface ProcessInfo {
  pid: number;
  ppid: number;
  name: string;
  command: string;
  args: string[];
  cpu: number;
  memory: number;
  ports: number[];
  state: 'running' | 'sleeping' | 'stopped' | 'zombie';
  startTime: Date;
}

export interface SpawnOptions {
  command: string;
  args: string[];
  cwd?: string;
  env?: Record<string, string>;
  detached?: boolean;
  stdio?: 'pipe' | 'inherit' | 'ignore';
}

export interface ProcessFilter {
  name?: string;
  command?: string;
  ports?: number[];
  minAge?: number; // milliseconds
  maxAge?: number;
  cpuThreshold?: number;
  memoryThreshold?: number;
}

export class ProcessCommand extends BaseCommand {
  public static getDefinition() {
    return {
      name: 'process',
      description: 'Low-level process management for system operations',
      category: COMMAND_CATEGORIES.KERNEL,
      parameters: {
        subcommand: { type: 'string' as const, description: 'Process operation to perform (list|find|spawn|kill|monitor|ports|tree|cleanup)' },
        filter: { type: 'object' as const, description: 'Filter criteria for process operations', required: false },
        pid: { type: 'number' as const, description: 'Process ID for operations like kill, monitor, tree', required: false },
        signal: { type: 'string' as const, description: 'Signal to send when killing process (TERM, KILL, etc.)', required: false }
      },
      examples: [
        { description: 'List all processes', command: '{"subcommand": "list"}' },
        { description: 'Find Node.js processes', command: '{"subcommand": "find", "filter": {"name": "node"}}' },
        { description: 'Kill process by PID', command: '{"subcommand": "kill", "pid": 1234}' }
      ],
      subcommands: {
        'list': 'List processes with optional filtering',
        'find': 'Find processes matching criteria',
        'spawn': 'Spawn new process with monitoring',
        'kill': 'Terminate process(es) by PID or criteria',
        'monitor': 'Monitor process resource usage',
        'ports': 'List processes using specific ports',
        'tree': 'Show process tree for given PID',
        'cleanup': 'Clean up zombie/orphaned processes'
      }
    };
  }

  public static async execute(params: any): Promise<CommandResult> {
    const { subcommand, ...options } = params;

    try {
      switch (subcommand) {
        case 'list':
          return await ProcessCommand.listProcesses(options.filter);
          
        case 'find':
          return await ProcessCommand.findProcesses(options.criteria);
          
        case 'spawn':
          return await ProcessCommand.spawnProcess(options.spawnOptions);
          
        case 'kill':
          return await ProcessCommand.killProcess(options.pid, options.signal);
          
        case 'monitor':
          return await ProcessCommand.monitorProcess(options.pid);
          
        case 'ports':
          return await ProcessCommand.getPortProcesses(options.ports);
          
        case 'tree':
          return await ProcessCommand.getProcessTree(options.pid);
          
        case 'cleanup':
          return await ProcessCommand.cleanupProcesses(options.filter);
          
        default:
          return {
            success: false,
            message: `Unknown subcommand: ${subcommand}`,
            error: `Unknown subcommand: ${subcommand}. Use 'list', 'find', 'spawn', 'kill', 'monitor', 'ports', 'tree', or 'cleanup'`
          };
      }
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        message: `Process command failed`,
        error: `Process command failed: ${errorMessage}`
      };
    }
  }

  /**
   * List all processes with optional filtering
   */
  private static async listProcesses(filter?: ProcessFilter): Promise<CommandResult> {
    try {
      const processes = await ProcessCommand.getAllProcesses();
      
      let filteredProcesses = processes;
      
      if (filter) {
        filteredProcesses = processes.filter(proc => {
          if (filter.name && !proc.name.includes(filter.name)) return false;
          if (filter.command && !proc.command.includes(filter.command)) return false;
          if (filter.ports && !filter.ports.some(port => proc.ports.includes(port))) return false;
          if (filter.cpuThreshold && proc.cpu < filter.cpuThreshold) return false;
          if (filter.memoryThreshold && proc.memory < filter.memoryThreshold) return false;
          
          const ageMs = Date.now() - proc.startTime.getTime();
          if (filter.minAge && ageMs < filter.minAge) return false;
          if (filter.maxAge && ageMs > filter.maxAge) return false;
          
          return true;
        });
      }

      return {
        success: true,
        message: `Found ${filteredProcesses.length} processes`,
        data: {
          processes: filteredProcesses,
          total: filteredProcesses.length,
          filtered: filter ? processes.length - filteredProcesses.length : 0
        }
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        message: `Failed to list processes`,
        error: `Failed to list processes: ${errorMessage}`
      };
    }
  }

  /**
   * Find processes matching specific criteria
   */
  private static async findProcesses(criteria: { pattern: string; matchType: 'name' | 'command' | 'args' }): Promise<CommandResult> {
    try {
      const processes = await ProcessCommand.getAllProcesses();
      const pattern = new RegExp(criteria.pattern, 'i');
      
      const matches = processes.filter(proc => {
        switch (criteria.matchType) {
          case 'name':
            return pattern.test(proc.name);
          case 'command':
            return pattern.test(proc.command);
          case 'args':
            return pattern.test(proc.args.join(' '));
          default:
            return pattern.test(proc.name) || pattern.test(proc.command);
        }
      });

      return {
        success: true,
        message: `Found ${matches.length} matching processes`,
        data: {
          matches,
          count: matches.length,
          pattern: criteria.pattern,
          matchType: criteria.matchType
        }
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        message: `Process search failed`,
        error: `Process search failed: ${errorMessage}`
      };
    }
  }

  /**
   * Spawn new process with monitoring
   */
  private static async spawnProcess(options: SpawnOptions): Promise<CommandResult> {
    try {
      const childProcess = spawn(options.command, options.args, {
        cwd: options.cwd,
        env: { ...process.env, ...options.env },
        detached: options.detached || false,
        stdio: options.stdio || 'pipe'
      });

      if (!childProcess.pid) {
        throw new Error('Failed to spawn process - no PID assigned');
      }

      // Set up process monitoring
      const processInfo: Partial<ProcessInfo> = {
        pid: childProcess.pid,
        name: options.command,
        command: options.command,
        args: options.args,
        startTime: new Date(),
        state: 'running'
      };

      // Monitor process lifecycle
      childProcess.on('error', (error) => {
        console.error(`Process ${childProcess.pid} error:`, error);
      });

      childProcess.on('exit', (code, signal) => {
        console.log(`Process ${childProcess.pid} exited with code ${code}, signal ${signal}`);
      });

      return {
        success: true,
        message: `Process spawned successfully: PID ${childProcess.pid}`,
        data: {
          pid: childProcess.pid,
          process: processInfo,
          message: `Process spawned successfully: ${options.command} (PID: ${childProcess.pid})`
        }
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        message: `Failed to spawn process`,
        error: `Failed to spawn process: ${errorMessage}`
      };
    }
  }

  /**
   * Kill process by PID
   */
  private static async killProcess(pid: number, signal: string = 'TERM'): Promise<CommandResult> {
    try {
      // Verify process exists first
      const { stdout } = await execAsync(`ps -p ${pid} -o comm=`);
      const processName = stdout.trim();
      
      if (!processName) {
        return {
          success: false,
          message: `Process ${pid} not found`,
          error: `Process ${pid} not found`
        };
      }

      // Send kill signal
      process.kill(pid, signal);
      
      // Wait a moment and verify process is gone (for SIGKILL)
      if (signal === 'KILL' || signal === '9') {
        await new Promise(resolve => setTimeout(resolve, 1000));
        
        try {
          await execAsync(`ps -p ${pid} -o comm=`);
          // If we get here, process still exists
          return {
            success: false,
            message: `Process ${pid} did not terminate`,
            error: `Process ${pid} did not terminate after SIGKILL`
          };
        } catch (error) {
          // Expected - process is gone
        }
      }

      return {
        success: true,
        message: `Process ${pid} terminated`,
        data: {
          pid,
          processName,
          signal,
          message: `Process ${pid} (${processName}) terminated with signal ${signal}`
        }
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        message: `Failed to kill process ${pid}`,
        error: `Failed to kill process ${pid}: ${errorMessage}`
      };
    }
  }

  /**
   * Get processes using specific ports
   */
  private static async getPortProcesses(ports: number[]): Promise<CommandResult> {
    try {
      const portProcesses: Record<number, ProcessInfo[]> = {};
      
      for (const port of ports) {
        try {
          const { stdout } = await execAsync(`lsof -ti:${port}`);
          const pids = stdout.trim().split('\n').filter(pid => pid).map(Number);
          
          portProcesses[port] = [];
          
          for (const pid of pids) {
            const processInfo = await ProcessCommand.getProcessInfo(pid);
            if (processInfo) {
              portProcesses[port].push(processInfo);
            }
          }
        } catch (error) {
          // Port not in use
          portProcesses[port] = [];
        }
      }

      return {
        success: true,
        message: `Found processes on ${Object.keys(portProcesses).length} ports`,
        data: {
          portProcesses,
          summary: Object.entries(portProcesses).map(([port, procs]) => ({
            port: Number(port),
            processCount: procs.length,
            pids: procs.map(p => p.pid)
          }))
        }
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        message: `Failed to get port processes`,
        error: `Failed to get port processes: ${errorMessage}`
      };
    }
  }

  /**
   * Get all processes on system
   */
  private static async getAllProcesses(): Promise<ProcessInfo[]> {
    try {
      // Use ps with specific format for cross-platform compatibility
      const { stdout } = await execAsync('ps -eo pid,ppid,comm,command,pcpu,pmem,lstart');
      const lines = stdout.trim().split('\n').slice(1); // Skip header
      
      const processes: ProcessInfo[] = [];
      
      for (const line of lines) {
        const parts = line.trim().split(/\s+/);
        if (parts.length >= 7) {
          const pid = Number(parts[0]);
          const ppid = Number(parts[1]);
          const name = parts[2];
          const command = parts[3];
          const cpu = Number(parts[4]);
          const memory = Number(parts[5]);
          
          // Parse start time (rough approximation)
          const startTimeStr = parts.slice(6).join(' ');
          const startTime = new Date(startTimeStr);
          
          // Get ports for this process
          const ports = await ProcessCommand.getProcessPorts(pid);
          
          processes.push({
            pid,
            ppid,
            name,
            command,
            args: command.split(' ').slice(1),
            cpu,
            memory,
            ports,
            state: 'running', // ps only shows running processes by default
            startTime
          });
        }
      }
      
      return processes;
    } catch (error) {
      throw new Error(`Failed to get process list: ${error}`);
    }
  }

  /**
   * Get ports used by specific process
   */
  private static async getProcessPorts(pid: number): Promise<number[]> {
    try {
      const { stdout } = await execAsync(`lsof -Pan -p ${pid} -i`);
      const ports: number[] = [];
      
      const lines = stdout.trim().split('\n');
      for (const line of lines) {
        const match = line.match(/:(\d+)\s/);
        if (match) {
          ports.push(Number(match[1]));
        }
      }
      
      return [...new Set(ports)]; // Remove duplicates
    } catch (error) {
      return []; // Process has no open ports
    }
  }

  /**
   * Get detailed process information
   */
  private static async getProcessInfo(pid: number): Promise<ProcessInfo | null> {
    try {
      const { stdout } = await execAsync(`ps -p ${pid} -o pid,ppid,comm,command,pcpu,pmem,lstart`);
      const lines = stdout.trim().split('\n');
      
      if (lines.length < 2) return null;
      
      const parts = lines[1].trim().split(/\s+/);
      if (parts.length < 7) return null;
      
      const ppid = Number(parts[1]);
      const name = parts[2];
      const command = parts[3];
      const cpu = Number(parts[4]);
      const memory = Number(parts[5]);
      const startTimeStr = parts.slice(6).join(' ');
      const startTime = new Date(startTimeStr);
      const ports = await ProcessCommand.getProcessPorts(pid);
      
      return {
        pid,
        ppid,
        name,
        command,
        args: command.split(' ').slice(1),
        cpu,
        memory,
        ports,
        state: 'running',
        startTime
      };
    } catch (error) {
      return null;
    }
  }

  /**
   * Monitor process resources over time
   */
  private static async monitorProcess(pid: number): Promise<CommandResult> {
    try {
      const processInfo = await ProcessCommand.getProcessInfo(pid);
      if (!processInfo) {
        return {
          success: false,
          message: `Process ${pid} not found`,
          error: `Process ${pid} not found`
        };
      }

      // Take multiple samples for trend analysis
      const samples = [];
      for (let i = 0; i < 5; i++) {
        const sample = await ProcessCommand.getProcessInfo(pid);
        if (sample) {
          samples.push({
            timestamp: new Date(),
            cpu: sample.cpu,
            memory: sample.memory,
            ports: sample.ports.length
          });
        }
        
        if (i < 4) {
          await new Promise(resolve => setTimeout(resolve, 1000));
        }
      }

      return {
        success: true,
        message: `Process ${pid} monitoring complete`,
        data: {
          pid,
          processInfo,
          samples,
          averages: {
            cpu: samples.reduce((sum, s) => sum + s.cpu, 0) / samples.length,
            memory: samples.reduce((sum, s) => sum + s.memory, 0) / samples.length
          }
        }
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        message: `Process monitoring failed`,
        error: `Process monitoring failed: ${errorMessage}`
      };
    }
  }

  /**
   * Get process tree for given PID
   */
  private static async getProcessTree(pid: number): Promise<CommandResult> {
    try {
      const { stdout } = await execAsync(`pstree -p ${pid}`);
      const tree = stdout.trim();
      
      return {
        success: true,
        message: `Process tree for PID ${pid}`,
        data: {
          pid,
          tree,
          message: `Process tree for PID ${pid}`
        }
      };
    } catch (error) {
      // Fallback to manual tree building
      const processInfo = await ProcessCommand.getProcessInfo(pid);
      if (!processInfo) {
        return {
          success: false,
          message: `Process ${pid} not found`,
          error: `Process ${pid} not found`
        };
      }

      return {
        success: true,
        message: `Process tree for PID ${pid}`,
        data: {
          pid,
          processInfo,
          tree: `${processInfo.name}(${pid})`,
          message: `Basic process info for PID ${pid} (pstree not available)`
        }
      };
    }
  }

  /**
   * Cleanup zombie/orphaned processes
   */
  private static async cleanupProcesses(filter?: ProcessFilter): Promise<CommandResult> {
    try {
      const processes = await ProcessCommand.getAllProcesses();
      const zombies = processes.filter(p => p.state === 'zombie');
      const orphans = processes.filter(p => p.ppid === 1 && p.pid !== 1);
      
      let cleanedUp = 0;
      const results = [];
      
      // Clean up zombies
      for (const zombie of zombies) {
        try {
          const result = await ProcessCommand.killProcess(zombie.pid, 'KILL');
          if (result.success) {
            cleanedUp++;
            results.push(`Cleaned zombie: ${zombie.name} (${zombie.pid})`);
          }
        } catch (error) {
          results.push(`Failed to clean zombie ${zombie.pid}: ${error}`);
        }
      }
      
      // Optionally clean up orphaned processes matching filter
      if (filter) {
        const filteredOrphans = orphans.filter(proc => {
          if (filter.name && !proc.name.includes(filter.name)) return false;
          if (filter.command && !proc.command.includes(filter.command)) return false;
          return true;
        });
        
        for (const orphan of filteredOrphans) {
          try {
            const result = await ProcessCommand.killProcess(orphan.pid, 'TERM');
            if (result.success) {
              cleanedUp++;
              results.push(`Cleaned orphan: ${orphan.name} (${orphan.pid})`);
            }
          } catch (error) {
            results.push(`Failed to clean orphan ${orphan.pid}: ${error}`);
          }
        }
      }

      return {
        success: true,
        message: `Cleanup complete: ${cleanedUp} processes cleaned`,
        data: {
          zombiesFound: zombies.length,
          orphansFound: orphans.length,
          cleanedUp,
          results
        }
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        message: `Process cleanup failed`,
        error: `Cleanup failed: ${errorMessage}`
      };
    }
  }
}

// Default export for convenience
export default ProcessCommand;