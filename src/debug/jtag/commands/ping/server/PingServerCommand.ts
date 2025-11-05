/**
 * Ping Command - Server Implementation
 *
 * Server collects server info, optionally includes browser info if connected.
 */

import { CommandBase, type ICommandDaemon } from '../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload, CommandParams, CommandResult } from '../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../system/core/types/JTAGTypes';
import type { PingParams, PingResult, ServerEnvironmentInfo } from '../shared/PingTypes';
import type { AIStatusParams, AIStatusResult } from '../../ai/status/shared/AIStatusTypes';

export class PingServerCommand extends CommandBase<PingParams, PingResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ping', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<PingResult> {
    const pingParams = params as PingParams;
    const server = await this.getServerInfo();

    // Collect AI status if verbose flag set
    let aiStatus;
    if (pingParams.verbose) {
      const startTime = Date.now();
      try {
        // Get ai/status command from commander
        interface CommandDaemonWithCommands {
          commands: Map<string, CommandBase<CommandParams, CommandResult>>;
        }
        const commandDaemon = this.commander as unknown as CommandDaemonWithCommands;
        const aiStatusCommand = commandDaemon.commands.get('ai/status');
        if (aiStatusCommand) {
          // Call ai/status with 2 second timeout
          const statusParams: AIStatusParams = {
            context: params.context,
            sessionId: params.sessionId,
            includeInactive: false,
            timeout: 2000  // 2 second timeout for AI status check
          };
          const statusResult = await aiStatusCommand.execute(statusParams) as AIStatusResult;

          const checkDuration = Date.now() - startTime;

          if (statusResult.success) {
            aiStatus = {
              ...statusResult.summary,
              checkDuration
            };
          }
        }
      } catch (error) {
        // AI status check failed or timed out - include empty summary
        aiStatus = {
          total: 0,
          healthy: 0,
          starting: 0,
          degraded: 0,
          dead: 0,
          checkDuration: Date.now() - startTime
        };
      }
    }

    // If browser already collected its info, we're the final step
    if (pingParams.browser) {
      return transformPayload(pingParams, {
        success: true,
        server,
        browser: pingParams.browser,
        aiStatus,
        timestamp: new Date().toISOString()
      });
    }

    // No browser info yet - delegate to browser to collect it
    return await this.remoteExecute({ ...pingParams, server, aiStatus });
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
      packageName: pkg.name,
      packageVersion: pkg.version,
      name: 'Node.js',
      version: proc.version,
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

      interface ICommandDaemonWithCommands {
        name: string;
        commands?: Map<string, unknown>;
      }

      const daemons = sys.systemDaemons ?? [];
      const commandDaemon = daemons.find(d => d.name === 'command-daemon') as ICommandDaemonWithCommands | undefined;
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
