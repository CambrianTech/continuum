/**
 * JTAG Base - Common foundation for JTAGSystem and JTAGClient
 * 
 * Simple base class that extends JTAGModule to establish inheritance
 */

import { JTAGModule } from './JTAGModule';
import type { CommandParams, CommandResult } from './JTAGTypes';
import type { UUID } from './CrossPlatformUUID';
import type { CommandBase } from '@commandBase';
import { EventManager } from '@systemEvents';

/**
 * Strongly-typed command function signature
 */
export type CommandFunction<TParams extends CommandParams = CommandParams, TResult extends CommandResult = CommandResult> = 
  (params?: TParams) => Promise<TResult>;

/**
 * Strongly-typed commands interface
 */
export type CommandsInterface = Map<string, CommandBase<CommandParams, CommandResult>>;

export abstract class JTAGBase extends JTAGModule {
  // Abstract methods that both JTAGSystem and JTAGClient need
  public abstract get sessionId(): UUID;

  public readonly eventManager = new EventManager();


  // Abstract method for subclasses to provide their command source
  protected abstract getCommandsInterface(): CommandsInterface;


  protected abstract initialize(): Promise<void>;
  
  /**
   * Commands interface - migrated from JTAGSystem
   * Creates proxy that injects sessionId and context into all commands
   */
  get commands(): Record<string, CommandFunction> {
    const commandsMap = this.getCommandsInterface();
    
    // Create proxy that injects sessionId from the Map structure
    return new Proxy({}, {
      get: (target, commandName: string) => {
        const command = commandsMap.get(commandName);
        if (!command) {
          throw new Error(`Command '${commandName}' not found. Available: ${Array.from(commandsMap.keys()).join(', ')}`);
        }
        
        // Wrap command to inject real sessionId and ensure required fields
        return async (params?: CommandParams) => {

          const fullParams = command.withDefaults(params ?? {}, this.sessionId, this.context);

          console.log(`Executing command '${commandName}' with params:`, fullParams);
          
          // Execute the command directly with proper typing
          return await command.execute(fullParams);
        };
      }
    }) as Record<string, CommandFunction>;
  }

}