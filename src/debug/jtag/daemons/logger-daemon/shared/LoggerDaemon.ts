/**
 * LoggerDaemon - Shared base class
 *
 * Thin wrapper that manages the Rust logger worker lifecycle.
 * This establishes the pattern for Rust-backed daemons:
 * - TypeScript handles daemon registration and lifecycle
 * - Rust worker does all heavy lifting (batching, file I/O, threading)
 * - Thin interface, no business logic in TypeScript
 */

import { DaemonBase } from '../../command-daemon/shared/DaemonBase';

export abstract class LoggerDaemon extends DaemonBase {
  public readonly subpath = 'logger';

  constructor(context: any, router: any) {
    super('LoggerDaemon', context, router);
  }
}
