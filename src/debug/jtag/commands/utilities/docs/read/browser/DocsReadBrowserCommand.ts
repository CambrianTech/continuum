import { DocsReadCommand } from '../shared/DocsReadCommand';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { DocsReadParams, DocsReadResult } from '../shared/DocsReadTypes';

export class DocsReadBrowserCommand extends DocsReadCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('utilities/docs/read', context, subpath, commander);
  }

  async execute(params: DocsReadParams): Promise<DocsReadResult> {
    return await this.remoteExecute({...params, context: params.context ?? this.context, sessionId: params.sessionId ?? ''});
  }
}
