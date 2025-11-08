/**
 * Data List-Handles Command - Server Implementation
 *
 * Lists all open database handles with metadata.
 * Routes requests to DatabaseHandleRegistry to retrieve handle information.
 *
 * See docs/MULTI-DATABASE-HANDLES.md for architecture
 */

import { DatabaseHandleRegistry } from '../../../../daemons/data-daemon/server/DatabaseHandleRegistry';
import type { DataListHandlesParams, DataListHandlesResult } from '../shared/DataListHandlesTypes';
import { createDataListHandlesResultFromParams } from '../shared/DataListHandlesTypes';

/**
 * Server-side command for listing database handles
 *
 * This command:
 * 1. Queries DatabaseHandleRegistry for all open handles
 * 2. Returns handle info (adapter type, config, timestamps, etc.)
 * 3. Useful for debugging and monitoring
 *
 * @example
 * ```typescript
 * // List all handles
 * const result = await Commands.execute('data/list-handles', {});
 *
 * // Output:
 * // {
 * //   success: true,
 * //   handles: [
 * //     {
 * //       handle: 'default',
 * //       adapter: 'sqlite',
 * //       config: { path: '.continuum/jtag/continuum.sqlite' },
 * //       isDefault: true,
 * //       openedAt: 1678901234567,
 * //       lastUsedAt: 1678901234999
 * //     },
 * //     {
 * //       handle: 'abc123...',
 * //       adapter: 'sqlite',
 * //       config: { path: '/tmp/training.sqlite', mode: 'readonly' },
 * //       isDefault: false,
 * //       openedAt: 1678901235000,
 * //       lastUsedAt: 1678901235500
 * //     }
 * //   ]
 * // }
 * ```
 */
export class DataListHandlesServerCommand {
  private registry: DatabaseHandleRegistry;

  constructor() {
    this.registry = DatabaseHandleRegistry.getInstance();
  }

  /**
   * Execute data/list-handles command
   *
   * @param params - No additional parameters needed
   * @returns Result with list of all open handles
   */
  async execute(params: DataListHandlesParams): Promise<DataListHandlesResult> {
    try {
      // Get all handles from registry
      const handles = this.registry.listHandles();

      // Return success with handles
      return createDataListHandlesResultFromParams(params, {
        success: true,
        handles
      });
    } catch (error) {
      // Return error
      return createDataListHandlesResultFromParams(params, {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      });
    }
  }
}
