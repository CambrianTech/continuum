/**
 * Command Base Class
 * 
 * Base class for all command implementations (ScreenshotCommand, etc.)
 * Provides remoteExecute() method for cross-context delegation.
 */

import { JTAGModule } from '../../../system/core/shared/JTAGModule';
import type { JTAGContext, CommandParams, CommandResult } from '../../../system/core/types/JTAGTypes';
import { JTAG_ENVIRONMENTS, JTAGMessageFactory } from '../../../system/core/types/JTAGTypes';
import { type UUID } from '../../../system/core/types/CrossPlatformUUID';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import { isRequestResult } from '../../../system/core/router/shared/RouterTypes';

export interface CommandEntry {
  name: string;
  className: string;
  commandClass: new (context: JTAGContext, subpath: string, commander: ICommandDaemon) => any;
}

// Forward declare CommandDaemon interface to avoid circular imports
export interface ICommandDaemon {
  readonly subpath: string;  
  readonly router: JTAGRouter;
  readonly commands: Map<string, CommandBase<CommandParams, CommandResult>>;
}

/**
 * Base class for all commands with type-safe parameters and results
 */
export abstract class CommandBase<TParams extends CommandParams = CommandParams, TResult extends CommandResult = CommandResult> extends JTAGModule {
  protected commander: ICommandDaemon;
  protected subpath: string; // Command subpath (e.g., "screenshot")

  constructor(name: string, context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(name, context);
    this.subpath = subpath;
    this.commander = commander;
  }

  /**
   * Execute this command with given parameters from message payload
   */
  abstract execute(params: TParams): Promise<TResult>;

  /**
   * Generate default parameters - override in subclasses for command-specific defaults
   * @param sessionId - Current session ID from the active request
   */
  public getDefaultParams(sessionId: UUID, context: JTAGContext): TParams {
    return {sessionId, context} as TParams;
  }

  /**
   * Merge user params with defaults - used by proxy interface
   */
  public withDefaults(params: Partial<TParams>, sessionId: UUID, context: JTAGContext): TParams {
    return { ...this.getDefaultParams(sessionId, context), ...params } as TParams;
  }

  /**
   * Remote execution - delegates to another environment
   * This is the key method for cross-context communication
   */
  protected async remoteExecute(
    params: TParams, 
    subpath: string = this.subpath,
    remote: string = this.context.environment === JTAG_ENVIRONMENTS.BROWSER 
      ? JTAG_ENVIRONMENTS.SERVER 
      : JTAG_ENVIRONMENTS.BROWSER
  ): Promise<TResult> {
    
    console.log(`🔀 ${this.toString()}: Remote executing in ${remote} environment`);
    
    const message = JTAGMessageFactory.createRequest(
      this.context,
      `${this.context.environment}/${this.commander.subpath}/${subpath}`,
      `${remote}/${this.commander.subpath}/${subpath}`,
      params,
      JTAGMessageFactory.generateCorrelationId()
    );

    const routerResult = await this.commander.router.postMessage(message);
    
    // Extract the actual command result from the router response
    if (isRequestResult(routerResult) && routerResult.response) {
      return routerResult.response as unknown as TResult;
    }
    
    throw new Error(`Remote execution failed: ${JSON.stringify(routerResult)}`);
  }

  /**
   * Get router instance from commander - guaranteed by constructor
   */
  protected getRouter(): JTAGRouter {
    return this.commander.router;
  }
}