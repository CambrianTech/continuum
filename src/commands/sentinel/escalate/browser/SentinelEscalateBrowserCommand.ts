/**
 * Sentinel Escalate — Browser (no-op, server-only command)
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { SentinelEscalateParams, SentinelEscalateResult } from '../shared/SentinelEscalateTypes';

export class SentinelEscalateBrowserCommand extends CommandBase<SentinelEscalateParams, SentinelEscalateResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('sentinel/escalate', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<SentinelEscalateResult> {
    return transformPayload(params, {
      success: false,
      processed: false,
      error: 'sentinel/escalate is server-only',
    });
  }
}
