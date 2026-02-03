/**
 * ServerCommands - Handle-Based Async Command Execution (Server-Only)
 *
 * Extends Commands with tracked execution modes using Handles + Events.
 * This file MUST NOT be imported from shared or browser code.
 *
 * Three tracked execution modes:
 *   1. Background:      execute({ background: true }) → returns handle immediately
 *   2. Timeout-tracked: execute({ timeout: 5000 }) → races against timeout
 *   3. Await:           ServerCommands.await('#abc123') → resolves when complete
 *
 * Architecture:
 *   - Commands (shared) delegates tracked execution to this class via TrackedCommandExecutor
 *   - ServerCommands.initialize() registers itself with Commands at server startup
 *   - All Handle/Events imports stay server-side, never pollute the browser bundle
 *
 * Event emission pattern:
 *   command:started:${handleId}  → { handle, command }
 *   command:complete:${handleId} → { handle, result }
 *   command:failed:${handleId}   → { handle, error }
 *   command:timeout:${handleId}  → { handle, behavior }
 */

import { Commands, CommandTimeoutError } from '../shared/Commands';
import type { TrackedCommandExecutor } from '../shared/Commands';
import type { CommandParams, CommandResult } from '../types/JTAGTypes';
import type { HandleRef, HandleRecord } from '../types/Handle';
import { Handles } from '../shared/Handles';
import { Events } from '../shared/Events';

export class ServerCommands implements TrackedCommandExecutor {
  private static _instance: ServerCommands | null = null;

  /**
   * Initialize server-side tracked execution.
   * Must be called once at server startup (e.g., in SystemOrchestrator).
   */
  static initialize(): void {
    if (this._instance) return; // Idempotent
    this._instance = new ServerCommands();
    Commands.registerTrackedExecutor(this._instance);
  }

  /**
   * Await a background command's result by handle reference.
   * Resolves when the handle reaches 'complete' or 'failed' status.
   *
   * @param ref - Handle reference (short ID "#abc123" or full UUID)
   * @param timeoutMs - Maximum time to wait for completion (default: 5 minutes)
   */
  static async await<U extends CommandResult = CommandResult>(
    ref: HandleRef,
    timeoutMs = 300_000,
  ): Promise<U> {
    const handle = await Handles.resolve(ref);
    if (!handle) {
      throw new Error(`Handle not found: ${ref}`);
    }

    // Already terminal?
    if (handle.status === 'complete') return handle.result as U;
    if (handle.status === 'failed') throw new Error(handle.error ?? 'Command failed');
    if (handle.status === 'cancelled') throw new Error('Command was cancelled');
    if (handle.status === 'expired') throw new Error('Handle expired before completion');

    // Still in progress — subscribe to completion events
    return new Promise<U>((resolve, reject) => {
      let timer: ReturnType<typeof setTimeout> | null = null;
      const unsubs: Array<() => void> = [];

      const cleanup = () => {
        if (timer) clearTimeout(timer);
        unsubs.forEach(fn => fn());
      };

      if (timeoutMs > 0) {
        timer = setTimeout(() => {
          cleanup();
          reject(new Error(`Await timeout: handle ${ref} did not complete within ${timeoutMs}ms`));
        }, timeoutMs);
      }

      unsubs.push(Events.subscribe<{ handle: string; result: unknown }>(
        `command:complete:${handle.id}`,
        (event) => {
          cleanup();
          resolve(event.result as U);
        }
      ));

      unsubs.push(Events.subscribe<{ handle: string; error: string }>(
        `command:failed:${handle.id}`,
        (event) => {
          cleanup();
          reject(new Error(event.error));
        }
      ));
    });
  }

  /**
   * Subscribe to events for a handle (progress, completion, failure).
   * Events follow the pattern: command:{event}:{handleId}
   *
   * @returns Unsubscribe function
   */
  static async subscribe(
    ref: HandleRef,
    listener: (event: { type: string; handle: string; [key: string]: unknown }) => void,
  ): Promise<() => void> {
    const handle = await Handles.resolve(ref);
    if (!handle) {
      throw new Error(`Handle not found: ${ref}`);
    }

    return Events.subscribe(
      `command:*:${handle.id}`,
      listener,
    );
  }

  // ──────────────────────────────────────────────
  // TrackedCommandExecutor implementation
  // ──────────────────────────────────────────────

  /**
   * Execute a command with handle-based tracking.
   * Called by Commands.execute() when params.background or params.timeout is set.
   */
  async executeTracked<T extends CommandParams, U extends CommandResult>(
    command: string,
    params: Partial<T> | undefined,
    executeDirect: (command: string, params?: Partial<T>) => Promise<U>,
  ): Promise<U> {
    const requesterId = params?.userId ?? '00000000-0000-0000-0000-000000000000';

    // Create handle for tracking
    const handle = await Handles.create(
      command,
      params ?? {},
      requesterId,
      params?.timeout ? params.timeout * 2 : undefined, // TTL = 2x timeout, or default
    );
    await Handles.markProcessing(handle.id);

    // Emit started event
    await Events.emit(`command:started:${handle.id}`, {
      handle: handle.shortId,
      command,
    });

    if (params?.background) {
      // Background: fire and forget — execute async, emit events on completion
      this._executeAsync<T, U>(command, params, handle, executeDirect);
      return { handle: `#${handle.shortId}`, handleId: handle.id } as U;
    }

    // Timeout-based execution
    return this._executeWithTimeout<T, U>(command, params, handle, executeDirect);
  }

  // ──────────────────────────────────────────────
  // Private: Execution Strategies
  // ──────────────────────────────────────────────

  /**
   * Background async execution — returns immediately, emits events on completion.
   */
  private async _executeAsync<T extends CommandParams, U extends CommandResult>(
    command: string,
    params: Partial<T> | undefined,
    handle: HandleRecord,
    executeDirect: (command: string, params?: Partial<T>) => Promise<U>,
  ): Promise<void> {
    try {
      const result = await executeDirect(command, params);
      await Handles.markComplete(handle.id, result);
      await Events.emit(`command:complete:${handle.id}`, {
        handle: `#${handle.shortId}`,
        result,
      });
    } catch (error) {
      const msg = error instanceof Error ? error.message : String(error);
      await Handles.markFailed(handle.id, msg);
      await Events.emit(`command:failed:${handle.id}`, {
        handle: `#${handle.shortId}`,
        error: msg,
      });
    }
  }

  /**
   * Timeout-tracked execution — races command against timeout.
   * Behavior on timeout controlled by params.onTimeout.
   */
  private async _executeWithTimeout<T extends CommandParams, U extends CommandResult>(
    command: string,
    params: Partial<T> | undefined,
    handle: HandleRecord,
    executeDirect: (command: string, params?: Partial<T>) => Promise<U>,
  ): Promise<U> {
    const timeoutMs = params?.timeout ?? 30_000;
    const onTimeout = params?.onTimeout ?? 'fail';

    // Sentinel to detect timeout
    const TIMEOUT_SENTINEL = Symbol('timeout');

    // Capture the execution promise so we can attach handlers if it survives the timeout
    const executionPromise = executeDirect(command, params);

    try {
      const result = await Promise.race([
        executionPromise,
        new Promise<typeof TIMEOUT_SENTINEL>((resolve) =>
          setTimeout(() => resolve(TIMEOUT_SENTINEL), timeoutMs)
        ),
      ]);

      if (result === TIMEOUT_SENTINEL) {
        // Timeout fired
        await Events.emit(`command:timeout:${handle.id}`, {
          handle: `#${handle.shortId}`,
          behavior: onTimeout,
        });

        switch (onTimeout) {
          case 'cancel':
            await Handles.markCancelled(handle.id);
            throw new CommandTimeoutError(command, timeoutMs, `#${handle.shortId}`, handle.id);

          case 'continue':
            // Original executionPromise is still running — attach completion handlers
            executionPromise
              .then(async (r) => {
                await Handles.markComplete(handle.id, r);
                await Events.emit(`command:complete:${handle.id}`, { handle: `#${handle.shortId}`, result: r });
              })
              .catch(async (e) => {
                const msg = e instanceof Error ? e.message : String(e);
                await Handles.markFailed(handle.id, msg);
                await Events.emit(`command:failed:${handle.id}`, { handle: `#${handle.shortId}`, error: msg });
              });
            return { handle: `#${handle.shortId}`, handleId: handle.id, timedOut: true } as U;

          case 'fail':
          default:
            await Handles.markFailed(handle.id, `Execution timeout after ${timeoutMs}ms`);
            throw new CommandTimeoutError(command, timeoutMs, `#${handle.shortId}`, handle.id);
        }
      }

      // Completed within timeout
      await Handles.markComplete(handle.id, result);
      await Events.emit(`command:complete:${handle.id}`, {
        handle: `#${handle.shortId}`,
        result,
      });
      return result as U;

    } catch (error) {
      if (error instanceof CommandTimeoutError) throw error;

      const msg = error instanceof Error ? error.message : String(error);
      await Handles.markFailed(handle.id, msg);
      await Events.emit(`command:failed:${handle.id}`, {
        handle: `#${handle.shortId}`,
        error: msg,
      });
      throw error;
    }
  }
}
