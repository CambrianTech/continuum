/**
 * Data List-Handles Command - Browser Implementation
 */

import { Commands } from '../../../../system/core/shared/Commands';
import type { DataListHandlesParams, DataListHandlesResult } from '../shared/DataListHandlesTypes';

export class DataListHandlesBrowserCommand {
  async execute(params: DataListHandlesParams): Promise<DataListHandlesResult> {
    return await Commands.execute<DataListHandlesResult>('data/list-handles', params) as DataListHandlesResult;
  }
}
