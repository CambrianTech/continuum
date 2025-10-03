/**
 * Ping Command - Server Implementation
 *
 * Server collects server info, optionally includes browser info if connected.
 */

import { CommandBase, type ICommandDaemon } from '../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../system/core/types/JTAGTypes';
import type { PingParams, PingResult, ServerEnvironmentInfo } from '../shared/PingTypes';

export class PingServerCommand extends CommandBase<PingParams, PingResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ping', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<PingResult> {
    const pingParams = params as PingParams;
    const server = await this.getServerInfo();

    // If browser already collected its info, we're the final step
    if (pingParams.browser) {
      return {
        ...pingParams,
        success: true,
        server,
        timestamp: new Date().toISOString()
      };
    }

    // No browser info yet - delegate to browser to collect it
    return await this.remoteExecute({ ...pingParams, server });
  }

  private async getServerInfo(): Promise<ServerEnvironmentInfo> {
    const proc = globalThis.process;
    const fs = await import('fs');
    const path = await import('path');

    const pkg = JSON.parse(fs.readFileSync(path.join(proc.cwd(), 'package.json'), 'utf-8'));
    const mem = proc.memoryUsage();

    const health = await this.collectSystemHealth();

    return {
      type: 'server',
      name: pkg.name,
      version: pkg.version,
      runtime: proc.version,
      platform: proc.platform,
      arch: proc.arch,
      processId: proc.pid,
      uptime: proc.uptime(),
      memory: {
        used: mem.heapUsed,
        total: mem.heapTotal,
        usage: `${Math.round((mem.heapUsed / mem.heapTotal) * 100)}%`
      },
      health,
      timestamp: new Date().toISOString()
    };
  }

  private async collectSystemHealth(): Promise<ServerEnvironmentInfo['health']> {
    try {
      const { JTAGSystemServer } = await import('../../../system/core/system/server/JTAGSystemServer');
      const sys = JTAGSystemServer.instance;

      if (!sys) {
        return { commandsRegistered: 0, daemonsActive: 0, systemReady: false };
      }

      const daemons = sys.systemDaemons ?? [];
      const commandDaemon = daemons.find(d => d.name === 'command-daemon') as unknown as { commands?: Map<string, unknown> };
      const commandsRegistered = commandDaemon?.commands?.size ?? 0;
      const daemonsActive = daemons.length;

      return {
        commandsRegistered,
        daemonsActive,
        systemReady: commandsRegistered > 0
      };
    } catch {
      return { commandsRegistered: 0, daemonsActive: 0, systemReady: false };
    }
  }
}
