/**
 * Ping Command - Server Implementation
 * 
 * Server-side ping command with server-specific environment detection.
 */

import type { JTAGContext } from '@shared/JTAGTypes';
import type { ICommandDaemon } from '@commandBase';
import { PingCommand } from '../shared/PingCommand';
import type { PingResult } from '../shared/PingTypes';

export class PingServerCommand extends PingCommand {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  protected async getEnvironmentInfo(): Promise<PingResult['environment']> {
    // Rich server environment information
    const process = typeof globalThis.process !== 'undefined' ? globalThis.process : null;
    const os = await import('os').catch(() => null);
    
    const memoryUsage = process?.memoryUsage() || { heapUsed: 0, heapTotal: 0, external: 0, arrayBuffers: 0 };
    const cpuUsage = process?.cpuUsage() || { user: 0, system: 0 };
    
    return {
      type: 'server',
      nodeVersion: process?.version || 'Unknown',
      platform: process?.platform || os?.platform() || 'Unknown',
      arch: process?.arch || os?.arch() || 'Unknown',
      processId: process?.pid || 0,
      uptime: process?.uptime() || 0,
      memory: {
        used: memoryUsage.heapUsed,
        total: memoryUsage.heapTotal,
        usage: `${Math.round((memoryUsage.heapUsed / memoryUsage.heapTotal) * 100)}%`
      },
      cpuUsage: {
        user: cpuUsage.user,
        system: cpuUsage.system
      },
      timestamp: new Date().toISOString()
    };
  }

  protected getEnvironmentLabel(): string {
    return 'SERVER';
  }
}