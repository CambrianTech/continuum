/**
 * Model Search Command - Browser Implementation
 *
 * Search HuggingFace for base models by name, architecture, or size. Used to find compaction targets (e.g., 'Qwen 3.5 27B'). Different from adapter/search which finds LoRA adapters.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ModelSearchParams, ModelSearchResult } from '../shared/ModelSearchTypes';

export class ModelSearchBrowserCommand extends CommandBase<ModelSearchParams, ModelSearchResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('model/search', context, subpath, commander);
  }

  async execute(params: ModelSearchParams): Promise<ModelSearchResult> {
    console.log('🌐 BROWSER: Delegating Model Search to server');
    return await this.remoteExecute(params);
  }
}
