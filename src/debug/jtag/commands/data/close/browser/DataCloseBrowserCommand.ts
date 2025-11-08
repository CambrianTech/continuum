/**
 * Data Close Command - Browser Implementation
 *
 * Forwards data/close requests to server via CommandDaemon.
 * Browser cannot directly close database handles - must go through server.
 *
 * See docs/MULTI-DATABASE-HANDLES.md for architecture
 */

import { Commands } from '../../../../system/core/shared/Commands';
import type { DataCloseParams, DataCloseResult } from '../shared/DataCloseTypes';

/**
 * Browser-side command for closing database handles
 *
 * This command forwards all requests to the server, where the actual
 * database handle cleanup occurs.
 *
 * @example
 * ```typescript
 * // From browser code
 * const openResult = await Commands.execute<DataOpenResult>('data/open', {
 *   adapter: 'sqlite',
 *   config: { path: '/tmp/training.sqlite', mode: 'create' }
 * });
 *
 * // Use it...
 * await Commands.execute('data/create', {
 *   dbHandle: openResult.dbHandle,
 *   collection: 'examples',
 *   data: example
 * });
 *
 * // Close when done
 * const closeResult = await Commands.execute<DataCloseResult>('data/close', {
 *   dbHandle: openResult.dbHandle
 * });
 * ```
 */
export class DataCloseBrowserCommand {
  /**
   * Execute data/close command by forwarding to server
   *
   * @param params - Handle to close
   * @returns Result from server with success status
   */
  async execute(params: DataCloseParams): Promise<DataCloseResult> {
    return await Commands.execute<DataCloseResult>('data/close', params) as DataCloseResult;
  }
}
