/**
 * GenomeJobStatusBrowserCommand - Browser-side job status query
 *
 * Delegates to server for actual database query.
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type {
  GenomeJobStatusParams,
  GenomeJobStatusResult
} from '../shared/GenomeJobStatusTypes';

export class GenomeJobStatusBrowserCommand extends CommandBase<
  GenomeJobStatusParams,
  GenomeJobStatusResult
> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome-job-status', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<GenomeJobStatusResult> {
    // Delegate to server — params has userId at runtime (injected by infrastructure)
    return this.remoteExecute(params as GenomeJobStatusParams);
  }
}
