/**
 * JTAG Base - Common foundation for JTAGSystem and JTAGClient
 * 
 * Simple base class that extends JTAGModule to establish inheritance
 */

import { JTAGModule } from './JTAGModule';
import type { CommandParams, CommandResult } from './JTAGTypes';
import type { UUID } from './CrossPlatformUUID';

export abstract class JTAGBase extends JTAGModule {
  // Abstract methods that both JTAGSystem and JTAGClient need
  abstract getSessionId(): string;
  
  // Abstract method for subclasses to provide their command source
  protected abstract getCommandsInterface(): Record<string, Function>;
  
  /**
   * Commands interface - migrated from JTAGSystem
   * Creates proxy that injects sessionId and context into all commands
   */
  get commands(): Record<string, (params?: CommandParams) => Promise<CommandResult>> {
    const commandsInterface = this.getCommandsInterface();
    
    // Create proxy that injects sessionId (same logic as JTAGSystem)
    return new Proxy(commandsInterface, {
      get: (target, commandName: string) => {
        const originalCommand = target[commandName];
        if (typeof originalCommand !== 'function') {
          return originalCommand;
        }
        
        // Wrap command to inject real sessionId and ensure required fields
        return async (params?: CommandParams) => {
          const sessionId = this.getSessionId();
          const paramsWithSession: CommandParams = { 
            context: this.context,  // Ensure context is always present
            sessionId: sessionId as UUID,  // Use real sessionId from system
            ...params              // User params override defaults
          };
          return await originalCommand(paramsWithSession);
        };
      }
    }) as Record<string, (params?: CommandParams) => Promise<CommandResult>>;
  }
}