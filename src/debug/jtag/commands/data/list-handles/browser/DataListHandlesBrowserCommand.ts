/**
 * Data List-Handles Command - Browser Implementation
 */

import { Commands } from '../../../../system/core/shared/Commands';
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';
import type { DataListHandlesParams, DataListHandlesResult } from '../shared/DataListHandlesTypes';

import { DataListHandles } from '../shared/DataListHandlesTypes';
export class DataListHandlesBrowserCommand {
  async execute(params: DataListHandlesParams): Promise<DataListHandlesResult> {
    return await DataListHandles.execute(params) as DataListHandlesResult;
  }
}
