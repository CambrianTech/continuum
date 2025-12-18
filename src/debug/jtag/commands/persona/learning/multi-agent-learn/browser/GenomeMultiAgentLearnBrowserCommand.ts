/**
 * GenomeMultiAgentLearnBrowserCommand - Browser stub
 *
 * All multi-agent learning happens server-side.
 */

import { CommandBase } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '@system/core/types/JTAGTypes';
import { transformPayload } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type {
  GenomeMultiAgentLearnParams,
  GenomeMultiAgentLearnResult
} from '../shared/GenomeMultiAgentLearnTypes';

export class GenomeMultiAgentLearnBrowserCommand extends CommandBase<
  GenomeMultiAgentLearnParams,
  GenomeMultiAgentLearnResult
> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('persona/learning/multi-agent-learn', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<GenomeMultiAgentLearnResult> {
    // Browser doesn't do multi-agent learning - forward to server
    return transformPayload(params, {
      success: false,
      error: 'Multi-agent learning must happen on server'
    });
  }
}
