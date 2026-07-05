/**
 * Data Close Command - Browser Implementation
 */

import { Commands } from '../../../../system/core/shared/Commands';
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';
import type { DataCloseParams, DataCloseResult } from '../shared/DataCloseTypes';

import { DataClose } from '../shared/DataCloseTypes';
export class DataCloseBrowserCommand {
  async execute(params: DataCloseParams): Promise<DataCloseResult> {
    return await DataClose.execute(params) as DataCloseResult;
  }
}
