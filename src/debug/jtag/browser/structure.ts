/**
 * Browser Structure Registry - Static Imports
 * 
 * Contains only browser-side daemon and command imports.
 * No server imports to avoid Node.js dependencies in browser bundle.
 */

// Browser Daemon Imports  
import { CommandDaemonBrowser } from '../daemons/command-daemon/browser/CommandDaemonBrowser';
import { ConsoleDaemonBrowser } from '../daemons/console-daemon/browser/ConsoleDaemonBrowser';

// Browser Command Imports
import { ScreenshotBrowserCommand } from '../daemons/command-daemon/commands/screenshot/browser/ScreenshotBrowserCommand';

// Types
import { DaemonBase } from '../shared/DaemonBase';
import { CommandBase } from '../daemons/command-daemon/shared/CommandBase';

export interface BrowserDaemonEntry {
  name: string;
  className: string;
  daemonClass: new (...args: any[]) => DaemonBase;
}

export interface BrowserCommandEntry {
  name: string;
  className: string;
  commandClass: new (...args: any[]) => CommandBase;
}

/**
 * Browser Environment Registry
 */
export const BROWSER_DAEMONS: BrowserDaemonEntry[] = [
  {
    name: 'CommandDaemon',
    className: 'CommandDaemonBrowser', 
    daemonClass: CommandDaemonBrowser
  },
  {
    name: 'ConsoleDaemon',
    className: 'ConsoleDaemonBrowser',
    daemonClass: ConsoleDaemonBrowser
  }
];

export const BROWSER_COMMANDS: BrowserCommandEntry[] = [
  {
    name: 'screenshot',
    className: 'ScreenshotBrowserCommand',
    commandClass: ScreenshotBrowserCommand
  }
];

/**
 * Create browser daemon instance by name
 */
export function createBrowserDaemon(daemonName: string, ...args: any[]): DaemonBase | null {
  const entry = BROWSER_DAEMONS.find(d => d.name === daemonName);
  
  if (!entry) {
    console.warn(`⚠️ Browser Structure: Daemon '${daemonName}' not found`);
    return null;
  }
  
  try {
    console.log(`🏗️ Browser Structure: Creating ${entry.className}`);
    return new entry.daemonClass(...args);
  } catch (error) {
    console.error(`❌ Browser Structure: Failed to create ${entry.className}:`, error);
    return null;
  }
}

/**
 * Create browser command instance by name
 */
export function createBrowserCommand(commandName: string, context: any, subpath: string, commander: any): CommandBase | null {
  const entry = BROWSER_COMMANDS.find(c => c.name === commandName);
  
  if (!entry) {
    console.warn(`⚠️ Browser Structure: Command '${commandName}' not found`);
    return null;
  }
  
  try {
    console.log(`🏗️ Browser Structure: Creating ${entry.className}`);
    return new entry.commandClass(context, subpath, commander);
  } catch (error) {
    console.error(`❌ Browser Structure: Failed to create ${entry.className}:`, error);
    return null;
  }
}

/**
 * Get all browser daemon names
 */
export function getBrowserDaemonNames(): string[] {
  return BROWSER_DAEMONS.map(d => d.name);
}

/**
 * Get all browser command names  
 */
export function getBrowserCommandNames(): string[] {
  return BROWSER_COMMANDS.map(c => c.name);
}