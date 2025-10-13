/**
 * AI Report Command Base
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { AIReportParams, AIReportResult } from './AIReportTypes';

export abstract class AIReportCommand extends CommandBase<AIReportParams, AIReportResult> {
  constructor(
    path: string,
    context: JTAGContext,
    subpath: string,
    commander: ICommandDaemon
  ) {
    super(path, context, subpath, commander);
  }

  abstract execute(params: AIReportParams): Promise<AIReportResult>;
}
