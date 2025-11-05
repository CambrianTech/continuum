/**
 * Data Update Command - Shared Base Class
 */

import { BaseDataCommand } from '../../shared/BaseDataCommand';
import type { DataUpdateParams, DataUpdateResult } from './DataUpdateTypes';
import type { BaseEntity } from '../../../../system/data/entities/BaseEntity';

export abstract class DataUpdateCommand<T extends BaseEntity = BaseEntity>
  extends BaseDataCommand<DataUpdateParams<T>, DataUpdateResult<T>> {

  protected abstract executeDataCommand(params: DataUpdateParams<T>): Promise<DataUpdateResult<T>>;
}