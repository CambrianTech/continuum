/**
 * Server Structure Registry - Static Imports
 * 
 * Contains only server-side daemon and command imports.
 * No browser imports to avoid bundling issues.
 */

// Server Daemon Imports
import { CommandDaemonServer } from '../daemons/command-daemon/server/CommandDaemonServer';
import { ConsoleDaemonServer } from '../daemons/console-daemon/server/ConsoleDaemonServer';

// Server Command Imports
import { ScreenshotServerCommand } from '../daemons/command-daemon/commands/screenshot/server/ScreenshotServerCommand';

// Types
import { DaemonBase } from '../shared/DaemonBase';
import { CommandBase } from '../daemons/command-daemon/shared/CommandBase';

export interface ServerDaemonEntry {
  name: string;
  className: string;
  daemonClass: new (...args: any[]) => DaemonBase;
}

export interface ServerCommandEntry {
  name: string;
  className: string;
  commandClass: new (...args: any[]) => CommandBase;
}

/**
 * Server Environment Registry
 */
export const SERVER_DAEMONS: ServerDaemonEntry[] = [
  {
    name: 'CommandDaemon',
    className: 'CommandDaemonServer',
    daemonClass: CommandDaemonServer
  },
  {
    name: 'ConsoleDaemon', 
    className: 'ConsoleDaemonServer',
    daemonClass: ConsoleDaemonServer
  }
];

export const SERVER_COMMANDS: ServerCommandEntry[] = [
  {
    name: 'screenshot',
    className: 'ScreenshotServerCommand',
    commandClass: ScreenshotServerCommand
  }
];

/**
 * Create server daemon instance by name
 */
export function createServerDaemon(daemonName: string, ...args: any[]): DaemonBase | null {
  const entry = SERVER_DAEMONS.find(d => d.name === daemonName);
  
  if (!entry) {
    console.warn(`⚠️ Server Structure: Daemon '${daemonName}' not found`);
    return null;
  }
  
  try {
    console.log(`🏗️ Server Structure: Creating ${entry.className}`);
    return new entry.daemonClass(...args);
  } catch (error) {
    console.error(`❌ Server Structure: Failed to create ${entry.className}:`, error);
    return null;
  }
}

/**
 * Create server command instance by name
 */
export function createServerCommand(commandName: string, context: any, subpath: string, commander: any): CommandBase | null {
  const entry = SERVER_COMMANDS.find(c => c.name === commandName);
  
  if (!entry) {
    console.warn(`⚠️ Server Structure: Command '${commandName}' not found`);
    return null;
  }
  
  try {
    console.log(`🏗️ Server Structure: Creating ${entry.className}`);
    return new entry.commandClass(context, subpath, commander);
  } catch (error) {
    console.error(`❌ Server Structure: Failed to create ${entry.className}:`, error);
    return null;
  }
}

/**
 * Get all server daemon names
 */
export function getServerDaemonNames(): string[] {
  return SERVER_DAEMONS.map(d => d.name);
}

/**
 * Get all server command names  
 */
export function getServerCommandNames(): string[] {
  return SERVER_COMMANDS.map(c => c.name);
}