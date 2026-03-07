/**
 * Codebase Index Browser Command — routes to server
 */

import { CodebaseIndexCommand } from '../shared/CodebaseIndexCommand';
import type { JTAGContext } from '../../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../../daemons/command-daemon/shared/CommandBase';
import type { CodebaseIndexParams, CodebaseIndexResult } from '../shared/CodebaseIndexTypes';

export class CodebaseIndexBrowserCommand extends CodebaseIndexCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/rag/index-codebase', context, subpath, commander);
  }

  async execute(params: CodebaseIndexParams): Promise<CodebaseIndexResult> {
    return this.remoteExecute(params) as Promise<CodebaseIndexResult>;
  }
}
