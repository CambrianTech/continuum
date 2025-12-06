import { DocsSearchCommand } from '../shared/DocsSearchCommand';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon} from '../../../../daemons/command-daemon/shared/CommandBase';
import type { DocsSearchParams, DocsSearchResult } from '../shared/DocsSearchTypes';

export class DocsSearchBrowserCommand extends DocsSearchCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('docs/search', context, subpath, commander);
  }

  async execute(params: DocsSearchParams): Promise<DocsSearchResult> {
    return await this.remoteExecute({...params, context: params.context ?? this.context, sessionId: params.sessionId ?? ''});
  }
}
