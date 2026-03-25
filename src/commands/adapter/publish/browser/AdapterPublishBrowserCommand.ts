/**
 * Adapter Publish Command - Browser Implementation
 *
 * Publish a trained LoRA adapter to HuggingFace with auto-generated model card and continuum:* tags. The adapter manifest metadata (role, skill, scores, base model) becomes discoverable via adapter/search. Every published adapter is an advertisement for the Continuum ecosystem.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { AdapterPublishParams, AdapterPublishResult } from '../shared/AdapterPublishTypes';

export class AdapterPublishBrowserCommand extends CommandBase<AdapterPublishParams, AdapterPublishResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('adapter/publish', context, subpath, commander);
  }

  async execute(params: AdapterPublishParams): Promise<AdapterPublishResult> {
    console.log('🌐 BROWSER: Delegating Adapter Publish to server');
    return await this.remoteExecute(params);
  }
}
