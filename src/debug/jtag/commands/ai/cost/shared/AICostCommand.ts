/**
 * AI Cost Command - Base class
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { AICostParams, AICostResult } from './AICostTypes';

export abstract class AICostCommand extends CommandBase<AICostParams, AICostResult> {
  constructor(
    path: string,
    context: JTAGContext,
    subpath: string,
    commander: ICommandDaemon
  ) {
    super(path, context, subpath, commander);
  }

  abstract execute(params: AICostParams): Promise<AICostResult>;
}
