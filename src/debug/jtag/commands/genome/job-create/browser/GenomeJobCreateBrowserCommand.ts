/**
 * GenomeJobCreateBrowserCommand - Browser-side placeholder for genome/job-create
 *
 * All logic is server-side (database operations, validation).
 * Browser command is a pass-through to server.
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type {
  GenomeJobCreateParams,
  GenomeJobCreateResult
} from '../shared/GenomeJobCreateTypes';

export class GenomeJobCreateBrowserCommand extends CommandBase<
  GenomeJobCreateParams,
  GenomeJobCreateResult
> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome-job-create', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<GenomeJobCreateResult> {
    // Browser-side: Always forward to server (database operations happen server-side)
    return transformPayload(params, {
      success: false,
      error: 'Job creation must happen on server'
    });
  }
}
