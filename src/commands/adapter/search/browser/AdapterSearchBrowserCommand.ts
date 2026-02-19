/**
 * Adapter Search Command - Browser Implementation
 *
 * Search for LoRA adapters across registries (HuggingFace, local, mesh)
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { AdapterSearchParams, AdapterSearchResult } from '../shared/AdapterSearchTypes';

export class AdapterSearchBrowserCommand extends CommandBase<AdapterSearchParams, AdapterSearchResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('adapter/search', context, subpath, commander);
  }

  async execute(params: AdapterSearchParams): Promise<AdapterSearchResult> {
    console.log('🌐 BROWSER: Delegating Adapter Search to server');
    return await this.remoteExecute(params);
  }
}
