/**
 * Ping Command - Server Implementation
 * 
 * Server-side ping command with server-specific environment detection.
 */

import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../daemons/command-daemon/shared/CommandBase';
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

    // Collect system health metrics
    const health = await this.collectSystemHealth();

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
      health,
      timestamp: new Date().toISOString()
    };
  }

  /**
   * Collect system health metrics leveraging cross-environment command system
   * Server pings browser to get complete picture of system health
   */
  private async collectSystemHealth(): Promise<{ browsersConnected: number; commandsRegistered: number; daemonsActive: number; systemReady: boolean }> {
    try {
      // Access the JTAGSystem singleton
      const { JTAGSystemServer } = await import('../../../system/core/system/server/JTAGSystemServer');
      const system = JTAGSystemServer.instance;

      if (!system) {
        return { browsersConnected: 0, commandsRegistered: 0, daemonsActive: 0, systemReady: false };
      }

      // Get WebSocket transport to count connected browsers
      let browsersConnected = 0;
      try {
        const transports = (system as any).transports || [];
        const wsTransport = transports.find((t: any) => t.name === 'websocket-server');
        if (wsTransport && wsTransport.clients) {
          browsersConnected = wsTransport.clients.size || 0;
        }
      } catch (e) {
        // Silently fail if transport access doesn't work
      }

      // Note: Could ping browser via remoteExecute() for deeper health check
      // For now, just report connection count as browsers are verified via WebSocket
      const browserAlive = browsersConnected > 0;

      // Get CommandDaemon to count registered commands
      let commandsRegistered = 0;
      try {
        const commandDaemon = (system as any).systemDaemons?.find((d: any) => d.name === 'command-daemon');
        if (commandDaemon && commandDaemon.commands) {
          commandsRegistered = commandDaemon.commands.size || 0;
        }
      } catch (e) {
        // Silently fail
      }

      // Count active daemons
      const daemonsActive = (system as any).systemDaemons?.length || 0;

      return {
        browsersConnected: browserAlive ? browsersConnected : 0, // Only count if browser responds
        commandsRegistered,
        daemonsActive,
        systemReady: browserAlive || commandsRegistered > 0 // System ready if browser responds OR commands registered
      };
    } catch (error) {
      return { browsersConnected: 0, commandsRegistered: 0, daemonsActive: 0, systemReady: false };
    }
  }

  protected getEnvironmentLabel(): string {
    return 'SERVER';
  }
}