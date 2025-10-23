/**
 * AI ThoughtStream Command (Shared)
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { ThoughtStreamParams, ThoughtStreamResult } from './ThoughtStreamTypes';

export abstract class ThoughtStreamCommand extends CommandBase<ThoughtStreamParams, ThoughtStreamResult> {
  constructor(
    path: string,
    context: JTAGContext,
    subpath: string,
    commander: ICommandDaemon
  ) {
    super(path, context, subpath, commander);
  }

  abstract execute(params: ThoughtStreamParams): Promise<ThoughtStreamResult>;
}
