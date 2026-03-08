/**
 * JTAG Base - Common foundation for JTAGSystem and JTAGClient
 * 
 * Simple base class that extends JTAGModule to establish inheritance
 */

import { JTAGModule } from './JTAGModule';
import type { CommandParams, CommandResult } from '../types/JTAGTypes';
import type { UUID } from '../types/CrossPlatformUUID';
import type { CommandBase } from '../../../daemons/command-daemon/shared/CommandBase';
import { EventManager } from '../../events';

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

  // Abstract method for router access (needed for scoped events)
  protected abstract getRouter(): any; // JTAGRouter type causes circular imports

  protected abstract initialize(): Promise<void>;
  
  /**
   * Events interface - event system access via EventManager
   */
  get events() {
    return this.eventManager.events;
  }


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
          // Use sessionId from params if provided, otherwise fall back to system sessionId
          const sessionIdToUse = (params as any)?.sessionId ?? this.sessionId;
          const contextToUse = (params as any)?.context ?? this.context;

          const fullParams = command.withDefaults(params ?? {}, sessionIdToUse, contextToUse);

          // Reduce log spam - only log errors
          // console.log(`Executing command '${commandName}' with params:`, fullParams);

          // Execute the command directly with proper typing
          return await command.execute(fullParams);
        };
      }
    }) as Record<string, CommandFunction>;
  }

}