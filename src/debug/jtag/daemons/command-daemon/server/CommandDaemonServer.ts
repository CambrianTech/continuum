/**
 * Command Daemon - Server Implementation
 * 
 * Server-side command daemon that handles server-specific commands.
 */

import { JTAGContext } from '../../../shared/JTAGTypes';
import { JTAGRouter } from '../../../shared/JTAGRouter';
import { CommandDaemonBase } from '../shared/CommandDaemonBase';
import { getCommandManifest } from '../../../manifests/command-manifest';

export class CommandDaemonServer extends CommandDaemonBase {
  
  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);
  }

  /**
   * Initialize server-specific commands using auto-discovery
   */
  protected async initializeCommands(): Promise<void> {
    const commandManifest = getCommandManifest('server');
    
    for (const [commandName, manifestEntry] of Object.entries(commandManifest)) {
      try {
        // Dynamic import using manifest
        const commandModule = await import(manifestEntry.importPath);
        const CommandClass = commandModule[manifestEntry.className];
        
        if (CommandClass) {
          const command = new CommandClass(this.context, commandName, this);
          this.register(commandName, command);
          console.log(`📦 Auto-discovered command: ${commandName}`);
        } else {
          console.warn(`⚠️ Command class not found: ${manifestEntry.className} in ${manifestEntry.importPath}`);
        }
      } catch (error: any) {
        console.error(`❌ Failed to load command ${commandName}:`, error.message);
      }
    }
    
    console.log(`🎯 ${this.toString()}: Auto-initialized ${this.commands.size} server commands`);
  }
}