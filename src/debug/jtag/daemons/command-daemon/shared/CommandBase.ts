/**
 * Command Base Class
 * 
 * Base class for all command implementations (ScreenshotCommand, etc.)
 * Provides remoteExecute() method for cross-context delegation.
 */

import { JTAGModule } from '../../../shared/JTAGModule';
import { JTAGContext, JTAGPayload, JTAGMessage, JTAG_ENVIRONMENTS, JTAGMessageFactory } from '../../../shared/JTAGTypes';
import { JTAGRouter } from '../../../shared/JTAGRouter';

// Forward declare CommandDaemon interface to avoid circular imports
export interface ICommandDaemon {
  readonly subpath: string;  
  readonly router: JTAGRouter;
  executeCommand(command: string, payload: any): Promise<any>;
  registerCommand(name: string, command: CommandBase): void;
}

/**
 * Base class for all commands with type-safe parameters and results
 */
export abstract class CommandBase<TParams extends JTAGPayload = JTAGPayload, TResult = unknown> extends JTAGModule {
  protected commander: ICommandDaemon;
  protected subpath: string; // Command subpath (e.g., "screenshot")

  constructor(name: string, context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(name, context);
    this.subpath = subpath;
    this.commander = commander;
  }

  /**
   * Execute this command with given parameters - now type-safe
   */
  abstract execute(params: TParams): Promise<TResult>;

  /**
   * Remote execution - delegates to another environment
   * This is the key method for cross-context communication
   */
  protected async remoteExecute(
    params: JTAGPayload, 
    subpath: string = this.subpath,
    remote: string = this.context.environment === JTAG_ENVIRONMENTS.BROWSER 
      ? JTAG_ENVIRONMENTS.SERVER 
      : JTAG_ENVIRONMENTS.BROWSER
  ): Promise<any> {
    
    console.log(`🔀 ${this.toString()}: Remote executing in ${remote} environment`);
    
    const message = JTAGMessageFactory.createRequest(
      this.context,
      `${this.context.environment}/${this.commander.subpath}/${subpath}`,
      `${remote}/${this.commander.subpath}/${subpath}`,
      params,
      JTAGMessageFactory.generateCorrelationId()
    );

    return await this.commander.router.postMessage(message);
  }

  /**
   * Get router instance from commander - guaranteed by constructor
   */
  protected getRouter(): JTAGRouter {
    return this.commander.router;
  }
}