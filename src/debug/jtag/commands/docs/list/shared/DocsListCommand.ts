import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { DocsListParams, DocsListResult } from './DocsListTypes';

export abstract class DocsListCommand extends CommandBase<DocsListParams, DocsListResult> {
  constructor(path: string, context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(path, context, subpath, commander);
  }

  abstract execute(params: DocsListParams): Promise<DocsListResult>;
}
