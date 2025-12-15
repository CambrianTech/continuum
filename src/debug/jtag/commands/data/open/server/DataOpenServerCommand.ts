/**
 * Data Open Command - Server Implementation
 *
 * Opens a new database handle for multi-database operations.
 * Routes requests to DatabaseHandleRegistry for handle management.
 *
 * See docs/MULTI-DATABASE-HANDLES.md for architecture
 */

import { DatabaseHandleRegistry } from '../../../../daemons/data-daemon/server/DatabaseHandleRegistry';
import type { DataOpenParams, DataOpenResult } from '../shared/DataOpenTypes';
import { createDataOpenResultFromParams } from '../shared/DataOpenTypes';

/**
 * Server-side command for opening database handles
 *
 * This command:
 * 1. Receives adapter type and config from client
 * 2. Delegates to DatabaseHandleRegistry to create handle
 * 3. Returns opaque DbHandle identifier for use in other commands
 *
 * @example
 * ```typescript
 * // Open training database
 * const result = await Commands.execute(DATA_COMMANDS.OPEN, {
 *   adapter: 'sqlite',
 *   config: {
 *     path: '/datasets/prepared/continuum-git.sqlite',
 *     mode: 'readonly'
 *   }
 * });
 *
 * // Use handle in other commands
 * const examples = await Commands.execute(DATA_COMMANDS.LIST, {
 *   dbHandle: result.dbHandle,
 *   collection: 'training_examples'
 * });
 * ```
 */
export class DataOpenServerCommand {
  private registry: DatabaseHandleRegistry;

  constructor() {
    this.registry = DatabaseHandleRegistry.getInstance();
  }

  /**
   * Execute data/open command
   *
   * @param params - Adapter type and config
   * @returns Result with dbHandle on success, error message on failure
   */
  async execute(params: DataOpenParams): Promise<DataOpenResult> {
    try {
      // Validate adapter type
      if (!params.adapter) {
        return createDataOpenResultFromParams(params, {
          success: false,
          error: 'Missing required parameter: adapter'
        });
      }

      // Validate config
      if (!params.config) {
        return createDataOpenResultFromParams(params, {
          success: false,
          error: 'Missing required parameter: config'
        });
      }

      // Delegate to registry
      const handle = await this.registry.open(params.adapter, params.config);

      // Return success with handle
      return createDataOpenResultFromParams(params, {
        success: true,
        dbHandle: handle,
        adapter: params.adapter
      });
    } catch (error) {
      // Return error
      return createDataOpenResultFromParams(params, {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      });
    }
  }
}
