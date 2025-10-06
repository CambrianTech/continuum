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

/**
 * Router result structure for better type safety
 */
interface RouterResult {
  success: boolean;
  handlerResult?: {
    commandResult?: unknown;
  } | unknown;
  response?: unknown;
}

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
 * Interface that enforces static commandName property
 * This ensures all command classes declare their command name
 */
export interface CommandConstructor {
  readonly commandName: string;
  executeIn<TParams extends CommandParams, TResult extends CommandResult>(
    environment: 'browser' | 'server',
    params: Omit<TParams, 'context' | 'sessionId'>
  ): Promise<TResult>;
}

/**
 * Base class for all commands with type-safe parameters and results
 *
 * REQUIRED: Subclasses MUST declare:
 *   static readonly commandName = 'command-name';
 *
 * This enables the elegant static execute() API
 */
export abstract class CommandBase<TParams extends CommandParams = CommandParams, TResult extends CommandResult = CommandResult> extends JTAGModule {
  protected commander: ICommandDaemon;
  protected subpath: string; // Command subpath (e.g., "screenshot")

  // Enforce that subclasses have static commandName
  static readonly commandName: string;

  constructor(name: string, context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(name, context);
    this.subpath = subpath;
    this.commander = commander;
  }

  /**
   * Protected: Subclasses can override to declare their natural environment
   *
   * Examples:
   *   - ScreenshotCommand: 'browser' (needs DOM)
   *   - FileSaveCommand: 'server' (needs filesystem)
   *   - DataReadCommand: 'auto' (works anywhere)
   */
  protected static get naturalEnvironment(): 'browser' | 'server' | 'auto' {
    return 'auto';
  }

  /**
   * Static execute - Universal command execution from anywhere
   *
   * Works in both browser and server with zero boilerplate:
   * ```typescript
   * const result = await ScreenshotCommand.execute({ querySelector: 'body' });
   * ```
   *
   * Subclasses inherit this automatically - just declare commandName
   */
  static async execute<TParams extends CommandParams, TResult extends CommandResult>(
    this: CommandConstructor,
    params: Omit<TParams, 'context' | 'sessionId'>
  ): Promise<TResult> {
    const { Commands } = await import('../../../system/core/shared/Commands');
    return await Commands.execute<TParams, TResult>(
      this.commandName,
      params
    );
  }

  /**
   * Static executeIn - Force execution in specific environment
   *
   * For cross-environment calls from within commands:
   * ```typescript
   * // From browser, call server version explicitly
   * const result = await FileSaveCommand.executeIn('server', { filepath, content });
   * ```
   */
  static async executeIn<TParams extends CommandParams, TResult extends CommandResult>(
    this: CommandConstructor,
    environment: 'browser' | 'server',
    params: Omit<TParams, 'context' | 'sessionId'>
  ): Promise<TResult> {
    const { Commands } = await import('../../../system/core/shared/Commands');
    return await Commands.execute<TParams, TResult>(
      this.commandName,
      { ...params, targetEnvironment: environment } as any
    );
  }

  /**
   * Static executeOnServer - Shorthand for server execution
   */
  static async executeOnServer<TParams extends CommandParams, TResult extends CommandResult>(
    this: CommandConstructor,
    params: Omit<TParams, 'context' | 'sessionId'>
  ): Promise<TResult> {
    return this.executeIn<TParams, TResult>('server', params);
  }

  /**
   * Static executeOnBrowser - Shorthand for browser execution
   */
  static async executeOnBrowser<TParams extends CommandParams, TResult extends CommandResult>(
    this: CommandConstructor,
    params: Omit<TParams, 'context' | 'sessionId'>
  ): Promise<TResult> {
    return this.executeIn<TParams, TResult>('browser', params);
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
  protected async remoteExecute<
    TParams extends CommandParams = CommandParams,
    TResult extends CommandResult = CommandResult
  >(
    params: TParams, 
    subpath: string = this.subpath,
    prefix?: string 
  ): Promise<TResult> {

    // Determine prefix based on current environment and subpath
    if (!prefix) {
      if (subpath === this.subpath) {
        // If subpath matches current command, use the opposite environment
        // This allows browser commands to call server commands and vice versa
        prefix = this.context.environment == JTAG_ENVIRONMENTS.BROWSER
          ? JTAG_ENVIRONMENTS.SERVER
          : JTAG_ENVIRONMENTS.BROWSER;
      }
      else {
        // If subpath is different, use the current environment
        // This allows commands to call other commands in the same environment
        prefix = this.context.environment;
      }
    }

    const origin = `${this.context.environment}/${this.commander.subpath}/${subpath}`;
    const endpoint = `${prefix}/${this.commander.subpath}/${subpath}`;

    // console.debug(`🔀 ${this.toString()}: ${subpath === this.subpath ? 'Remote' : 'Local'} executing from ${origin} to call ${endpoint}`);

    const message = JTAGMessageFactory.createRequest(
      this.context,
      origin,
      endpoint,
      params,
      JTAGMessageFactory.generateCorrelationId()
    );

    const routerResult = await this.commander.router.postMessage(message);

    // console.debug(`🔄 ${this.toString()}: Remote execution result:`, JSON.stringify(routerResult, null, 2));
    
    // Extract the actual command result from the router response
    const typedResult = routerResult as RouterResult;
    if (typedResult.success) {
      const handlerResult = typedResult.handlerResult;
      if (handlerResult && typeof handlerResult === 'object' && 'commandResult' in handlerResult) {
        // The actual command result is in handlerResult.commandResult for cross-context calls
        const commandResult = handlerResult.commandResult || handlerResult;
        return commandResult as unknown as TResult;
      } else if (handlerResult) {
        return handlerResult as unknown as TResult;
      }
      
      // Fallback to legacy response structure
      const response = typedResult.response;
      if (response) {
        return response as unknown as TResult;
      }
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