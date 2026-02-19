/**
 * Data Create Command - Shared Base Implementation
 *
 * Following ARCHITECTURE-RULES.md:
 * ✅ Generic programming with BaseEntity only
 * ✅ Uses BaseDataCommand for backend routing
 * ✅ Abstract methods for environment-specific operations
 */

import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import { BaseDataCommand } from '../../shared/BaseDataCommand';
import type { DataCreateParams, DataCreateResult } from './DataCreateTypes';
import type { BaseEntity } from '../../../../system/data/entities/BaseEntity';

/**
 * Abstract base for DataCreate commands
 * Handles backend routing, subclasses implement storage operations
 */
export abstract class DataCreateCommand<T extends BaseEntity = BaseEntity>
  extends BaseDataCommand<DataCreateParams, DataCreateResult<T>> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('data-create', context, subpath, commander);
  }

  /**
   * Abstract method for data create operation
   * Environment-specific implementations handle their appropriate backend
   */
  protected abstract executeDataCommand(params: DataCreateParams): Promise<DataCreateResult<T>>;
}