/**
 * Command Daemon - Server Implementation
 * 
 * Server-side command daemon that handles server-specific commands.
 */

import { JTAGContext } from '../../../shared/JTAGTypes';
import { JTAGRouter } from '../../../shared/JTAGRouter';
import { CommandDaemonBase } from '../shared/CommandDaemonBase';
import { SERVER_COMMANDS, createServerCommand } from '../../../server/structure';

export class CommandDaemonServer extends CommandDaemonBase {
  
  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);
  }

  /**
   * Initialize server-specific commands using static imports
   */
  protected async initializeCommands(): Promise<void> {
    for (const commandEntry of SERVER_COMMANDS) {
      try {
        const command = createServerCommand(commandEntry.name, this.context, commandEntry.name, this);
        if (command) {
          this.register(commandEntry.name, command);
          console.log(`📦 Registered server command: ${commandEntry.name} (${commandEntry.className})`);
        }
      } catch (error: any) {
        console.error(`❌ Failed to create server command ${commandEntry.name}:`, error.message);
      }
    }
    
    console.log(`🎯 ${this.toString()}: Auto-initialized ${this.commands.size} server commands`);
  }
}