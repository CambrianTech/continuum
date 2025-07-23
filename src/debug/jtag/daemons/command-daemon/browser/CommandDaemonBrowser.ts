/**
 * Command Daemon - Browser Implementation
 * 
 * Browser-side command daemon that handles browser-specific commands.
 */

import { JTAGContext } from '../../../shared/JTAGTypes';
import { JTAGRouter } from '../../../shared/JTAGRouter';
import { CommandDaemonBase } from '../shared/CommandDaemonBase';
import { BROWSER_COMMANDS, createBrowserCommand } from '../../../browser/structure';

export class CommandDaemonBrowser extends CommandDaemonBase {
  
  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);
  }

  /**
   * Initialize browser-specific commands using static imports
   */
  protected async initializeCommands(): Promise<void> {
    for (const commandEntry of BROWSER_COMMANDS) {
      try {
        const command = createBrowserCommand(commandEntry.name, this.context, commandEntry.name, this);
        if (command) {
          this.register(commandEntry.name, command);
          console.log(`📦 Registered browser command: ${commandEntry.name} (${commandEntry.className})`);
        }
      } catch (error: any) {
        console.error(`❌ Failed to create browser command ${commandEntry.name}:`, error.message);
      }
    }
    
    console.log(`🎯 ${this.toString()}: Auto-initialized ${this.commands.size} browser commands`);
  }
}