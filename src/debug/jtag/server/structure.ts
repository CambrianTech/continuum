/**
 * Server Structure Registry - Static Imports
 * 
 * Contains only server-side daemon and command imports.
 * No browser imports to avoid bundling issues.
 */

// Server Daemon Imports
import { CommandDaemonServer } from '../daemons/command-daemon/server/CommandDaemonServer';
import { ConsoleDaemonServer } from '../daemons/console-daemon/server/ConsoleDaemonServer';
import { HealthDaemonServer } from '../daemons/health-daemon/server/HealthDaemonServer';

// Server Command Imports
import { ScreenshotServerCommand } from '../daemons/command-daemon/commands/screenshot/server/ScreenshotServerCommand';

// Types
import type { DaemonEntry } from '../shared/DaemonBase';
import type { CommandEntry } from '../daemons/command-daemon/shared/CommandBase';

/**
 * Server Environment Registry
 */
export const SERVER_DAEMONS: DaemonEntry[] = [
  {
    name: 'CommandDaemon',
    className: 'CommandDaemonServer',
    daemonClass: CommandDaemonServer
  },
  {
    name: 'ConsoleDaemon', 
    className: 'ConsoleDaemonServer',
    daemonClass: ConsoleDaemonServer
  },
  {
    name: 'HealthDaemon',
    className: 'HealthDaemonServer', 
    daemonClass: HealthDaemonServer
  }
];

export const SERVER_COMMANDS: CommandEntry[] = [
  {
    name: 'screenshot',
    className: 'ScreenshotServerCommand',
    commandClass: ScreenshotServerCommand
  }
];