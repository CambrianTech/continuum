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

import { DataOpen } from '../shared/DataOpenTypes';
import { DataList } from '../../list/shared/DataListTypes';
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
 * const result = await DataOpen.execute({
 *   adapter: 'sqlite',
 *   config: {
 *     path: '/datasets/prepared/continuum-git.sqlite',
 *     mode: 'readonly'
 *   }
 * });
 *
 * // Use handle in other commands
 * const examples = await DataList.execute({
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
  /** Valid adapter types for helpful error messages */
  private static readonly VALID_ADAPTERS = ['sqlite', 'json', 'vector', 'graph', 'rust'] as const;

  async execute(params: DataOpenParams): Promise<DataOpenResult> {
    try {
      // Validate adapter type
      if (!params.adapter) {
        return createDataOpenResultFromParams(params, {
          success: false,
          error: `Missing required parameter: adapter. ` +
            `Valid adapters: ${DataOpenServerCommand.VALID_ADAPTERS.join(', ')}. ` +
            `NOTE: Most commands use the default database automatically - ` +
            `you probably want data/list or data/read instead of data/open.`
        });
      }

      // Validate adapter is a known type
      if (!DataOpenServerCommand.VALID_ADAPTERS.includes(params.adapter as any)) {
        return createDataOpenResultFromParams(params, {
          success: false,
          error: `Unknown adapter type: '${params.adapter}'. ` +
            `Valid adapters: ${DataOpenServerCommand.VALID_ADAPTERS.join(', ')}. ` +
            `Example: data/open --adapter="sqlite" --config='{"path":"/tmp/my.db"}'`
        });
      }

      // Validate config
      if (!params.config) {
        return createDataOpenResultFromParams(params, {
          success: false,
          error: `Missing required parameter: config. ` +
            `For ${params.adapter}, use: --config='{"path":"/path/to/database"}'. ` +
            `NOTE: Most commands use the default database automatically - ` +
            `you probably want data/list or data/read instead.`
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
