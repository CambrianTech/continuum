/**
 * Commands - Clean Client Interface with Type-Safe Command Registry
 *
 * Provides elegant static interface for command execution with full type inference.
 * No more manual generic parameters - types are inferred from command name!
 *
 * Three execution modes:
 *   1. Direct (default): execute → await result (fast path, no Handle overhead)
 *   2. Background:       execute({ background: true }) → returns handle immediately
 *   3. Timeout-tracked:  execute({ timeout: 5000 }) → races execution vs timeout
 *
 * Modes 2 & 3 require ServerCommands to be initialized (server-side only).
 * The shared Commands class stays environment-agnostic — all Handle/Events
 * logic lives in system/core/server/ServerCommands.ts.
 *
 * Usage:
 *   // Direct (fast path — works in browser + server)
 *   const result = await Commands.execute('ping', {});
 *
 *   // Background — server only, returns handle immediately
 *   const { handle, handleId } = await Commands.execute('ai/generate', { background: true });
 *
 *   // Await a handle — server only
 *   const result = await ServerCommands.await(handle);
 */

import { JTAGClient } from '../client/shared/JTAGClient';
import type { CommandParams, CommandResult } from '../types/JTAGTypes';

import { Screenshot } from '../../../commands/interface/screenshot/shared/ScreenshotTypes';
import { FileSave } from '../../../commands/file/save/shared/FileSaveTypes';

/** Error thrown when a command exceeds its timeout */
export class CommandTimeoutError extends Error {
  constructor(
    public readonly command: string,
    public readonly timeoutMs: number,
    public readonly handle?: string,
    public readonly handleId?: string,
  ) {
    super(`Command '${command}' timed out after ${timeoutMs}ms`);
    this.name = 'CommandTimeoutError';
  }
}

/**
 * Interface for tracked (background/timeout) command execution.
 * Implemented by ServerCommands, registered at server startup.
 */
export interface TrackedCommandExecutor {
  executeTracked<T extends CommandParams, U extends CommandResult>(
    command: string,
    params: Partial<T> | undefined,
    executeDirect: (command: string, params?: Partial<T>) => Promise<U>,
  ): Promise<U>;
}

export class Commands {
  /** Server-side tracked executor (registered by ServerCommands at startup) */
  private static _trackedExecutor: TrackedCommandExecutor | null = null;

  /** Register the server-side tracked executor (called by ServerCommands.initialize()) */
  static registerTrackedExecutor(executor: TrackedCommandExecutor): void {
    this._trackedExecutor = executor;
  }

  /**
   * Execute a command with full type safety.
   * Context and sessionId are auto-injected, all other params required/optional as defined.
   *
   * When params.background is true, returns { handle, handleId } immediately (server only).
   * When params.timeout is set, races execution against timeout (server only).
   * Otherwise, executes directly with no Handle overhead (fast path).
   */
  static execute<T extends CommandParams = CommandParams, U extends CommandResult = CommandResult>(
    command: string,
    params?: Partial<T>
  ): Promise<U>;

  // Implementation
  static async execute<T extends CommandParams = CommandParams, U extends CommandResult = CommandResult>(
    command: string,
    params?: Partial<T>
  ): Promise<U> {
    const isTracked = params?.background || params?.timeout;

    if (!isTracked) {
      // Fast path: direct execution (no Handle overhead)
      return this._executeDirect<T, U>(command, params);
    }

    // Tracked path: delegate to ServerCommands (server-side only)
    if (!this._trackedExecutor) {
      throw new Error(
        'Tracked execution (background/timeout) requires server environment. ' +
        'Ensure ServerCommands.initialize() is called at server startup.'
      );
    }

    return this._trackedExecutor.executeTracked<T, U>(
      command,
      params,
      (cmd, p) => this._executeDirect<T, U>(cmd, p),
    );
  }

  /**
   * Execute command and extract rich content (markdown, images, audio)
   *
   * Automatically unwraps common content fields from command results.
   * Perfect for PersonaUsers who want clean content without parsing structures.
   */
  static async content<T extends CommandParams = CommandParams>(
    command: string,
    params?: Partial<T>
  ): Promise<string | Buffer | Uint8Array> {
    const result = await Commands.execute<T, CommandResult>(command, params);

    // Try common content field names in priority order
    if ('content' in result && result.content) return result.content as string;
    if ('markdown' in result && result.markdown) return result.markdown as string;
    if ('text' in result && result.text) return result.text as string;
    if ('data' in result && result.data) return result.data as Buffer | Uint8Array;
    if ('buffer' in result && result.buffer) return result.buffer as Buffer | Uint8Array;
    if ('audio' in result && result.audio) return result.audio as Buffer | Uint8Array;
    if ('image' in result && result.image) return result.image as Buffer | Uint8Array;
    if ('media' in result && (result.media as { data?: Buffer | Uint8Array })?.data) {
      return (result.media as { data: Buffer | Uint8Array }).data;
    }

    // Fallback: JSON stringify the result
    return JSON.stringify(result, null, 2);
  }

  // ──────────────────────────────────────────────
  // Internal: Direct Execution (environment-agnostic)
  // ──────────────────────────────────────────────

  /**
   * Direct execution — the fast path. No Handle overhead.
   * Works in both browser and server environments.
   * Exposed internally so ServerCommands can delegate to it.
   */
  static async _executeDirect<T extends CommandParams, U extends CommandResult>(
    command: string,
    params?: Partial<T>,
  ): Promise<U> {
    // Server-side optimization: If we're already in a server context with a CommandDaemon,
    // route internally instead of creating a new client connection
    interface GlobalWithJTAG {
      __JTAG_COMMAND_DAEMON__?: unknown;
      __JTAG_CONTEXT__?: string;
      __JTAG_SESSION_ID__?: string;
    }
    const globalWithJTAG = globalThis as GlobalWithJTAG;

    if (typeof process !== 'undefined' && globalWithJTAG.__JTAG_COMMAND_DAEMON__) {
      interface CommandDaemonWithCommands {
        commands: Map<string, { execute(params: CommandParams): Promise<CommandResult> }>;
      }
      const commandDaemon = globalWithJTAG.__JTAG_COMMAND_DAEMON__ as CommandDaemonWithCommands;

      const finalParams: CommandParams = {
        context: params?.context || globalWithJTAG.__JTAG_CONTEXT__ || 'unknown',
        sessionId: params?.sessionId || globalWithJTAG.__JTAG_SESSION_ID__ || 'unknown',
        userId: params?.userId,
        ...(params || {})
      } as T;

      const commandInstance = commandDaemon.commands.get(command);
      if (commandInstance) {
        return await commandInstance.execute(finalParams) as U;
      }
    }

    // Client-side or fallback: Use JTAGClient
    const jtagClient = await JTAGClient.sharedInstance;

    const finalParams: T = {
      context: jtagClient.context,
      sessionId: jtagClient.sessionId,
      userId: jtagClient.userId,
      ...(params || {})
    } as T;

    return await jtagClient.daemons.commands.execute<T, U>(command, finalParams);
  }
}
