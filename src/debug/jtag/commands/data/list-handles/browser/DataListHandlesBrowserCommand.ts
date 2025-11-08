/**
 * Data List-Handles Command - Browser Implementation
 *
 * Forwards data/list-handles requests to server via CommandDaemon.
 * Browser cannot directly access handle registry - must go through server.
 *
 * See docs/MULTI-DATABASE-HANDLES.md for architecture
 */

import { Commands } from '../../../../system/core/shared/Commands';
import type { DataListHandlesParams, DataListHandlesResult } from '../shared/DataListHandlesTypes';

/**
 * Browser-side command for listing database handles
 *
 * This command forwards all requests to the server, where the actual
 * handle registry querying occurs.
 *
 * @example
 * ```typescript
 * // From browser code
 * const result = await Commands.execute<DataListHandlesResult>('data/list-handles', {});
 *
 * console.log(`Open handles: ${result.handles.length}`);
 * result.handles.forEach(h => {
 *   console.log(`  ${h.handle}: ${h.adapter} (${h.isDefault ? 'default' : 'custom'})`);
 * });
 * ```
 */
export class DataListHandlesBrowserCommand {
  /**
   * Execute data/list-handles command by forwarding to server
   *
   * @param params - No additional parameters needed
   * @returns Result from server with list of all open handles
   */
  async execute(params: DataListHandlesParams): Promise<DataListHandlesResult> {
    return await Commands.execute<DataListHandlesResult>('data/list-handles', params) as DataListHandlesResult;
  }
}
