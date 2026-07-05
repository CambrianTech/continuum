/**
 * Query Next Command - Shared Base Implementation
 *
 * Gets the next page from a paginated query using the handle.
 * DataDaemon manages cursor position internally.
 */

import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import { BaseDataCommand } from '../../shared/BaseDataCommand';
import type { DataQueryNextParams, DataQueryNextResult } from './QueryNextTypes';

/**
 * Abstract base for QueryNext commands
 * Handles backend routing, subclasses implement storage operations
 */
export abstract class QueryNextCommand
  extends BaseDataCommand<DataQueryNextParams, DataQueryNextResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('data-query-next', context, subpath, commander);
  }

  /**
   * Abstract method for query next operation
   * Environment-specific implementations handle their appropriate backend
   */
  protected abstract executeDataCommand(params: DataQueryNextParams): Promise<DataQueryNextResult>;
}
