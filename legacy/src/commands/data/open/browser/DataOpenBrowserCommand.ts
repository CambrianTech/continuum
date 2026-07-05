/**
 * Data Open Command - Browser Implementation
 *
 * Forwards data/open requests to server via CommandDaemon.
 * Browser cannot directly open database handles - must go through server.
 *
 * See docs/MULTI-DATABASE-HANDLES.md for architecture
 */

import { Commands } from '../../../../system/core/shared/Commands';
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';
import type { DataOpenParams, DataOpenResult } from '../shared/DataOpenTypes';

import { DataOpen } from '../shared/DataOpenTypes';
import { DataList } from '../../list/shared/DataListTypes';
/**
 * Browser-side command for opening database handles
 *
 * This command forwards all requests to the server, where the actual
 * database handle management occurs. The browser receives back an opaque
 * DbHandle identifier that can be used in subsequent data/* commands.
 *
 * @example
 * ```typescript
 * // From browser code
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
export class DataOpenBrowserCommand {
  /**
   * Execute data/open command by forwarding to server
   *
   * @param params - Adapter type and config
   * @returns Result from server with dbHandle on success
   */
  async execute(params: DataOpenParams): Promise<DataOpenResult> {
    return await DataOpen.execute(params) as DataOpenResult;
  }
}
