/**
 * Query Open Command - Shared Base Implementation
 *
 * Opens a paginated query and returns a handle (UUID).
 * DataDaemon maintains pagination state internally.
 */

import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import { BaseDataCommand } from '../../shared/BaseDataCommand';
import type { DataQueryOpenParams, DataQueryOpenResult } from './QueryOpenTypes';

/**
 * Abstract base for QueryOpen commands
 * Handles backend routing, subclasses implement storage operations
 */
export abstract class QueryOpenCommand
  extends BaseDataCommand<DataQueryOpenParams, DataQueryOpenResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('data-query-open', context, subpath, commander);
  }

  /**
   * Abstract method for query open operation
   * Environment-specific implementations handle their appropriate backend
   */
  protected abstract executeDataCommand(params: DataQueryOpenParams): Promise<DataQueryOpenResult>;
}
