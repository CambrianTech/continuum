/**
 * InfoCommand - TypeScript Implementation
 * Parent class for information display commands with full type safety
 */

import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';
import { fileURLToPath } from 'url';
import { BaseCommand, CommandDefinition, CommandContext, CommandResult } from '../BaseCommand';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

interface InfoParams {
  section?: 'overview' | 'system' | 'server' | 'memory' | 'connections';
}

interface SystemInfo {
  platform: string;
  architecture: string;
  nodeVersion: string;
  osType: string;
  osRelease: string;
  cpuModel: string;
  cpuCores: number;
  uptime: string;
}

interface ServerInfo {
  version: string;
  uptime: string;
  pid: number;
  workingDirectory: string;
  nodeArgs: string;
}

interface MemoryInfo {
  rss: string;
  heapUsed: string;
  heapTotal: string;
  external: string;
}

interface ConnectionInfo {
  webSocketServer: string;
  port: number;
  activeConnections: number;
  commandBus: string;
}

export class InfoCommand extends BaseCommand {
  static getDefinition(): CommandDefinition {
    try {
      const readmePath = path.join(__dirname, 'README.md');
      const readme = fs.readFileSync(readmePath, 'utf8');
      return this.parseReadmeDefinition(readme);
    } catch (error) {
      // Fallback definition if README.md not found
      return {
        name: 'info',
        description: 'Display system information and server status',
        icon: 'ℹ️',
        category: 'core',
        params: '{"section": "overview|system|server|memory|connections"}',
        examples: [
          'info',
          'info --params \'{"section": "system"}\'',
          'info --params \'{"section": "memory"}\''
        ],
        usage: 'info [--params \'{"section": "<section-name"}\']'
      };
    }
  }

  static async execute(params: InfoParams, context?: CommandContext): Promise<CommandResult> {
    try {
      const { section = 'overview' } = this.parseParams<InfoParams>(params);
      
      this.logExecution('InfoCommand', { section }, context);
      
      // Get all system information
      const systemInfo = this.getSystemInfo();
      const serverInfo = this.getServerInfo();
      const result: any = {};

      switch (section) {
        case 'overview':
          this.displaySection('🖥️ SYSTEM', this.formatSystemInfo(systemInfo));
          this.displaySection('🚀 SERVER', this.formatServerInfo(serverInfo));
          result.system = systemInfo;
          result.server = serverInfo;
          break;
          
        case 'system':
          this.displaySection('🖥️ SYSTEM', this.formatSystemInfo(systemInfo));
          result.system = systemInfo;
          break;
          
        case 'server':
          this.displaySection('🚀 SERVER', this.formatServerInfo(serverInfo));
          result.server = serverInfo;
          break;
          
        case 'memory':
          const memoryInfo = this.getMemoryInfo();
          this.displaySection('💾 MEMORY', this.formatMemoryInfo(memoryInfo));
          result.memory = memoryInfo;
          break;
          
        case 'connections':
          const connectionInfo = this.getConnectionInfo(context);
          this.displaySection('🔗 CONNECTIONS', this.formatConnectionInfo(connectionInfo));
          result.connections = connectionInfo;
          break;
          
        default:
          return this.createErrorResult(
            `Invalid section: ${section}. Available: overview, system, server, memory, connections`
          );
      }

      result.version = this.getVersion();
      
      return this.createSuccessResult(
        `System information (${section}) displayed successfully`,
        result
      );
      
    } catch (error) {
      console.error('❌ InfoCommand Error:', error);
      return this.createErrorResult(`Failed to get system information: ${error.message}`);
    }
  }

  /**
   * Parse README.md for command definition
   */
  private static parseReadmeDefinition(readme: string): CommandDefinition {
    const lines = readme.split('\n');
    const definition: Partial<CommandDefinition> = {};
    
    let inDefinition = false;
    let inParams = false;
    let inTodos = false;
    const todos: string[] = [];
    
    for (const line of lines) {
      if (line.includes('## Definition')) {
        inDefinition = true;
        continue;
      }
      if (inDefinition && line.startsWith('##')) {
        inDefinition = false;
      }
      if (line.includes('## Parameters')) {
        inParams = true;
        continue;
      }
      if (inParams && line.startsWith('##')) {
        inParams = false;
      }
      if (line.includes('## TODO:')) {
        inTodos = true;
        continue;
      }
      if (inTodos && line.startsWith('##')) {
        inTodos = false;
      }
      
      if (inDefinition) {
        if (line.includes('**Name**:')) {
          definition.name = line.split('**Name**:')[1].trim();
        } else if (line.includes('**Description**:')) {
          definition.description = line.split('**Description**:')[1].trim();
        } else if (line.includes('**Icon**:')) {
          definition.icon = line.split('**Icon**:')[1].trim();
        } else if (line.includes('**Category**:')) {
          definition.category = line.split('**Category**:')[1].trim();
        }
      }
      
      if (inTodos && line.includes('TODO:')) {
        todos.push(line.trim());
      }
    }
    
    // Add TODOs to description if they exist
    if (todos.length > 0 && definition.description) {
      definition.description += ` (⚠️ ${todos.length} TODOs pending)`;
    }
    
    return definition as CommandDefinition;
  }

  /**
   * Get typed system information
   */
  private static getSystemInfo(): SystemInfo {
    const cpus = os.cpus();
    const uptime = os.uptime();
    const uptimeFormatted = `${Math.floor(uptime / 3600)}h ${Math.floor((uptime % 3600) / 60)}m`;
    
    return {
      platform: os.platform(),
      architecture: os.arch(),
      nodeVersion: process.version,
      osType: os.type(),
      osRelease: os.release(),
      cpuModel: cpus[0]?.model || 'Unknown',
      cpuCores: cpus.length,
      uptime: uptimeFormatted
    };
  }

  /**
   * Get typed server information
   */
  private static getServerInfo(): ServerInfo {
    const uptime = process.uptime();
    const uptimeFormatted = `${Math.floor(uptime / 3600)}h ${Math.floor((uptime % 3600) / 60)}m ${Math.floor(uptime % 60)}s`;
    
    return {
      version: this.getVersion(),
      uptime: uptimeFormatted,
      pid: process.pid,
      workingDirectory: process.cwd(),
      nodeArgs: process.argv.slice(2).join(' ') || 'none'
    };
  }

  /**
   * Get typed memory information
   */
  private static getMemoryInfo(): MemoryInfo {
    const memUsage = process.memoryUsage();
    const formatBytes = (bytes: number): string => (bytes / 1024 / 1024).toFixed(2) + ' MB';
    
    return {
      rss: formatBytes(memUsage.rss),
      heapUsed: formatBytes(memUsage.heapUsed),
      heapTotal: formatBytes(memUsage.heapTotal),
      external: formatBytes(memUsage.external)
    };
  }

  /**
   * Get typed connection information
   */
  private static getConnectionInfo(context?: CommandContext): ConnectionInfo {
    return {
      webSocketServer: 'Active',
      port: 9000,
      activeConnections: context?.webSocketServer?.clients?.size || 0,
      commandBus: 'Ready'
    };
  }

  /**
   * Get application version
   */
  private static getVersion(): string {
    try {
      const packagePath = path.join(process.cwd(), 'package.json');
      const packageJson = JSON.parse(fs.readFileSync(packagePath, 'utf8'));
      return packageJson.version || 'unknown';
    } catch (error) {
      return 'unknown';
    }
  }

  /**
   * Format system info for display
   */
  private static formatSystemInfo(info: SystemInfo): string {
    return `  Platform: ${info.platform} ${info.architecture}
  Node.js: ${info.nodeVersion}
  OS: ${info.osType} ${info.osRelease}
  CPU: ${info.cpuModel} (${info.cpuCores} cores)
  Uptime: ${info.uptime}`;
  }

  /**
   * Format server info for display
   */
  private static formatServerInfo(info: ServerInfo): string {
    return `  Version: ${info.version}
  Uptime: ${info.uptime}
  PID: ${info.pid}
  Working Directory: ${info.workingDirectory}
  Node Args: ${info.nodeArgs}`;
  }

  /**
   * Format memory info for display
   */
  private static formatMemoryInfo(info: MemoryInfo): string {
    return `  RSS: ${info.rss}
  Heap Used: ${info.heapUsed}
  Heap Total: ${info.heapTotal}
  External: ${info.external}`;
  }

  /**
   * Format connection info for display
   */
  private static formatConnectionInfo(info: ConnectionInfo): string {
    return `  WebSocket Server: ${info.webSocketServer}
  Port: ${info.port}
  Active Connections: ${info.activeConnections}
  Command Bus: ${info.commandBus}`;
  }

  /**
   * Display section with consistent formatting
   */
  private static displaySection(title: string, content: string): void {
    console.log(`${title}:`);
    console.log(content);
    console.log('');
  }
}

// Default export for easier module loading
export default InfoCommand;