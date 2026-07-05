import { DocsListCommand } from '../shared/DocsListCommand';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { DocsListParams, DocsListResult } from '../shared/DocsListTypes';

export class DocsListBrowserCommand extends DocsListCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('utilities/docs/list', context, subpath, commander);
  }

  async execute(params: DocsListParams): Promise<DocsListResult> {
    return await this.remoteExecute({...params, context: params.context ?? this.context, sessionId: params.sessionId ?? ''});
  }
}
