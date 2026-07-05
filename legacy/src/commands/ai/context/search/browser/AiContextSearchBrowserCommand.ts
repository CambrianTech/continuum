/**
 * Ai Context Search Command - Browser Implementation
 *
 * Semantic context navigation - search memories, messages, timeline across all entity types using cosine similarity via Rust embedding worker
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { AiContextSearchParams, AiContextSearchResult } from '../shared/AiContextSearchTypes';

export class AiContextSearchBrowserCommand extends CommandBase<AiContextSearchParams, AiContextSearchResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/context/search', context, subpath, commander);
  }

  async execute(params: AiContextSearchParams): Promise<AiContextSearchResult> {
    console.log('🌐 BROWSER: Delegating Ai Context Search to server');
    return await this.remoteExecute(params);
  }
}
