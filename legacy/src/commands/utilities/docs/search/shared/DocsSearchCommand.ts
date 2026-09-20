import { CommandBase } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { DocsSearchParams, DocsSearchResult } from './DocsSearchTypes';

export abstract class DocsSearchCommand extends CommandBase<DocsSearchParams, DocsSearchResult> {
  constructor(path: string, context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(path, context, subpath, commander);
  }

  abstract execute(params: DocsSearchParams): Promise<DocsSearchResult>;
}
