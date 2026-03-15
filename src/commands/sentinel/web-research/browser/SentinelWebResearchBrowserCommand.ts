/**
 * Sentinel Web Research — Browser (no-op, server-only command)
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { SentinelWebResearchParams, SentinelWebResearchResult } from '../shared/SentinelWebResearchTypes';

export class SentinelWebResearchBrowserCommand extends CommandBase<SentinelWebResearchParams, SentinelWebResearchResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('sentinel/web-research', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<SentinelWebResearchResult> {
    return transformPayload(params, {
      success: false,
      summary: '',
      pages: [],
      pagesFetched: 0,
      query: '',
      error: 'sentinel/web-research is server-only',
    });
  }
}
