/**
 * Data Read Command - Shared Base Class
 *
 * Generic base class for data read operations with environment routing.
 * Follows ARCHITECTURE-RULES.md: BaseEntity-only generic programming.
 */

import { BaseDataCommand } from '../../shared/BaseDataCommand';
import type { DataReadParams, DataReadResult } from './DataReadTypes';
import type { BaseEntity } from '../../../../system/data/entities/BaseEntity';

/**
 * Base class for data read commands
 * Provides environment routing via BaseDataCommand
 */
export abstract class DataReadCommand<T extends BaseEntity = BaseEntity>
  extends BaseDataCommand<DataReadParams, DataReadResult<T>> {

  /**
   * Subclasses implement this for their specific environment
   */
  protected abstract executeDataCommand(params: DataReadParams): Promise<DataReadResult<T>>;
}