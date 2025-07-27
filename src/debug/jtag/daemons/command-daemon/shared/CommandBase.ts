/**
 * Command Base Class
 * 
 * Base class for all command implementations (ScreenshotCommand, etc.)
 * Provides remoteExecute() method for cross-context delegation.
 */

import { JTAGModule } from '@shared/JTAGModule';
import type { JTAGContext, CommandParams, CommandResult } from '@shared/JTAGTypes';
import { JTAG_ENVIRONMENTS, JTAGMessageFactory } from '@shared/JTAGTypes';
import { type UUID } from '@shared/CrossPlatformUUID';
import type { JTAGRouter } from '@shared/JTAGRouter';
import { isRequestResult } from '@shared/RouterTypes';

export interface CommandEntry {
  name: string;
  className: string;
  commandClass: new (context: JTAGContext, subpath: string, commander: ICommandDaemon) => any;
}

// Forward declare CommandDaemon interface to avoid circular imports
export interface ICommandDaemon {
  readonly subpath: string;  
  readonly router: JTAGRouter;
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
  public getDefaultParams(sessionId: UUID): TParams {
    return {} as TParams;
  }

  /**
   * Merge user params with defaults - used by proxy interface
   */
  public withDefaults(params: Partial<TParams>, sessionId: UUID): TParams {
    return { ...this.getDefaultParams(sessionId), ...params } as TParams;
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
      return routerResult.response as TResult;
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