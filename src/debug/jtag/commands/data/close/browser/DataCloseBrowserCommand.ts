/**
 * Data Close Command - Browser Implementation
 */

import { Commands } from '../../../../system/core/shared/Commands';
import type { DataCloseParams, DataCloseResult } from '../shared/DataCloseTypes';

export class DataCloseBrowserCommand {
  async execute(params: DataCloseParams): Promise<DataCloseResult> {
    return await Commands.execute<DataCloseResult>('data/close', params) as DataCloseResult;
  }
}
