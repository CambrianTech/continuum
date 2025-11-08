/**
 * Data Close Command - Server Implementation
 *
 * Closes a database handle, releasing resources.
 * Routes requests to DatabaseHandleRegistry for handle cleanup.
 *
 * See docs/MULTI-DATABASE-HANDLES.md for architecture
 */

import { DatabaseHandleRegistry, DEFAULT_HANDLE } from '../../../../daemons/data-daemon/server/DatabaseHandleRegistry';
import type { DataCloseParams, DataCloseResult } from '../shared/DataCloseTypes';
import { createDataCloseResultFromParams } from '../shared/DataCloseTypes';

/**
 * Server-side command for closing database handles
 *
 * This command:
 * 1. Validates handle (cannot close 'default')
 * 2. Delegates to DatabaseHandleRegistry to close handle
 * 3. Returns success or error
 *
 * @example
 * ```typescript
 * // Open training database
 * const openResult = await Commands.execute('data/open', {
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
 * await Commands.execute('data/close', {
 *   dbHandle: openResult.dbHandle
 * });
 * ```
 */
export class DataCloseServerCommand {
  private registry: DatabaseHandleRegistry;

  constructor() {
    this.registry = DatabaseHandleRegistry.getInstance();
  }

  /**
   * Execute data/close command
   *
   * @param params - Handle to close
   * @returns Result with success status
   */
  async execute(params: DataCloseParams): Promise<DataCloseResult> {
    try {
      // Validate handle provided
      if (!params.dbHandle) {
        return createDataCloseResultFromParams(params, {
          success: false,
          error: 'Missing required parameter: dbHandle'
        });
      }

      // Prevent closing default handle
      if (params.dbHandle === DEFAULT_HANDLE) {
        return createDataCloseResultFromParams(params, {
          success: false,
          error: 'Cannot close default database handle - it remains open for process lifetime'
        });
      }

      // Delegate to registry
      await this.registry.close(params.dbHandle);

      // Return success
      return createDataCloseResultFromParams(params, {
        success: true
      });
    } catch (error) {
      // Return error
      return createDataCloseResultFromParams(params, {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      });
    }
  }
}
