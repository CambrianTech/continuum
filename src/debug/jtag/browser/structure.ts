/**
 * Browser Structure Registry - Static Imports
 * 
 * Contains only browser-side daemon and command imports.
 * No server imports to avoid Node.js dependencies in browser bundle.
 */

// Browser Daemon Imports  
import { CommandDaemonBrowser } from '../daemons/command-daemon/browser/CommandDaemonBrowser';
import { ConsoleDaemonBrowser } from '../daemons/console-daemon/browser/ConsoleDaemonBrowser';
import { HealthDaemonBrowser } from '../daemons/health-daemon/browser/HealthDaemonBrowser';

// Browser Command Imports
import { ScreenshotBrowserCommand } from '../daemons/command-daemon/commands/screenshot/browser/ScreenshotBrowserCommand';

// Types
import type { DaemonEntry } from '../shared/DaemonBase';
import type { CommandEntry } from '../daemons/command-daemon/shared/CommandBase';

/**
 * Browser Environment Registry
 */
export const BROWSER_DAEMONS: DaemonEntry[] = [
  {
    name: 'CommandDaemon',
    className: 'CommandDaemonBrowser', 
    daemonClass: CommandDaemonBrowser
  },
  {
    name: 'ConsoleDaemon',
    className: 'ConsoleDaemonBrowser',
    daemonClass: ConsoleDaemonBrowser
  },
  {
    name: 'HealthDaemon',
    className: 'HealthDaemonBrowser',
    daemonClass: HealthDaemonBrowser
  }
];

export const BROWSER_COMMANDS: CommandEntry[] = [
  {
    name: 'screenshot',
    className: 'ScreenshotBrowserCommand',
    commandClass: ScreenshotBrowserCommand
  }
];


