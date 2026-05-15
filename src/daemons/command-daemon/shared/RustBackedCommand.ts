/**
 * RustBackedCommand — base class for the standard "validate → call mixin →
 * wrap result" envelope shared by every TS command that exists ONLY to
 * route into a Rust IPC handler (#1198).
 *
 * # Why this exists
 *
 * Per Joel's "TS moves DOWN into rust… if not UI/UX it is rust" rule
 * (2026-05-14), every Rust-backed TS command in `src/commands/*` does
 * the same five things in the same order:
 *
 *   1. Validate the required params (throw `ValidationError` with a
 *      consistent message + missing-field name)
 *   2. Resolve the Rust IPC client singleton
 *   3. Call the typed mixin method on the client
 *   4. Translate the snake_case Rust response into the camelCase
 *      `Result` shape via `createXResultFromParams`
 *   5. Return the wrapped result
 *
 * Steps 1, 2, and 5 are pure boilerplate. Steps 3 and 4 are the only
 * variable bits per command. The pre-#1198 status quo was every command
 * re-writing the boilerplate inline, ~30 LOC of envelope around ~5 LOC
 * of actual call. That's uncompressed redundancy → drift target (the
 * specific drift the compression principle in CLAUDE.md exists to
 * prevent).
 *
 * # How to use
 *
 * Subclass declares: `requiredParams` (which fields must be non-empty),
 * `callRust(params, client)` (the variable mixin call), and
 * `toResult(raw, params)` (the variable result wrapping). Base class
 * owns: validation loop, client resolution, error consistency.
 *
 * See `commands/cognition/admit-inbox-message/server/CognitionAdmitInboxMessageServerCommand.ts`
 * for the canonical example refactored under #1198.
 *
 * # Why TRest is generic (not `unknown`)
 *
 * Each subclass knows the exact mixin response shape (it's a typed
 * ts-rs export). Threading it through `TRest` lets `toResult` be
 * type-safe instead of carrying an `unknown` cast. Subclasses that
 * don't care can use `unknown` explicitly.
 *
 * # Custom validation
 *
 * Subclasses that need richer per-field validation than non-empty
 * (e.g., shape constraints like `typeof params.message === 'object'`)
 * override `validateParams(params)` and call `super.validateParams(params)`
 * BEFORE adding their custom checks. This preserves the consistent
 * required-field behavior.
 */

import { CommandBase, type ICommandDaemon } from './CommandBase';
import type {
  CommandParams,
  CommandResult,
  JTAGContext,
} from '../../../system/core/types/JTAGTypes';
import { ValidationError } from '../../../system/core/types/ErrorTypes';
import { RustCoreIPCClient } from '../../../workers/continuum-core/bindings/RustCoreIPC';

export abstract class RustBackedCommand<
  TParams extends CommandParams,
  TResult extends CommandResult,
  TRest = unknown,
> extends CommandBase<TParams, TResult> {
  /**
   * Names of params this command requires to be present + non-empty.
   * The base class throws `ValidationError` with a consistent message
   * that names the offending field and points at the command's README.
   */
  protected abstract readonly requiredParams: ReadonlyArray<keyof TParams>;

  constructor(
    name: string,
    context: JTAGContext,
    subpath: string,
    commander: ICommandDaemon,
  ) {
    super(name, context, subpath, commander);
  }

  /**
   * Subclass implements the actual mixin invocation. The base class
   * has already validated `requiredParams` and resolved `client`.
   */
  protected abstract callRust(
    params: TParams,
    client: RustCoreIPCClient,
  ): Promise<TRest>;

  /**
   * Subclass translates the raw Rust response (snake_case) into the
   * camelCase `Result` type, typically via the per-command
   * `createXResultFromParams(...)` factory.
   */
  protected abstract toResult(raw: TRest, params: TParams): TResult;

  /**
   * Common required-param check. Subclasses with richer needs override
   * and call `super.validateParams(params)` first.
   */
  protected validateParams(params: TParams): void {
    for (const key of this.requiredParams) {
      const value = (params as Record<string, unknown>)[key as string];
      const missing =
        value === undefined ||
        value === null ||
        (typeof value === 'string' && value.trim() === '');
      if (missing) {
        throw new ValidationError(
          String(key),
          `Missing required parameter '${String(key)}'. ` +
            `See the ${this.name} README for usage.`,
        );
      }
    }
  }

  override async execute(params: TParams): Promise<TResult> {
    this.validateParams(params);
    const client = await RustCoreIPCClient.getInstanceAsync();
    const raw = await this.callRust(params, client);
    return this.toResult(raw, params);
  }
}
