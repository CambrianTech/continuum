/**
 * Query Close Command - Shared Base Implementation
 *
 * Closes a paginated query and frees resources.
 * Should be called when done with pagination.
 */

import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import { BaseDataCommand } from '../../shared/BaseDataCommand';
import type { DataQueryCloseParams, DataQueryCloseResult } from './QueryCloseTypes';

/**
 * Abstract base for QueryClose commands
 * Handles backend routing, subclasses implement storage operations
 */
export abstract class QueryCloseCommand
  extends BaseDataCommand<DataQueryCloseParams, DataQueryCloseResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('data-query-close', context, subpath, commander);
  }

  /**
   * Abstract method for query close operation
   * Environment-specific implementations handle their appropriate backend
   */
  protected abstract executeDataCommand(params: DataQueryCloseParams): Promise<DataQueryCloseResult>;
}
