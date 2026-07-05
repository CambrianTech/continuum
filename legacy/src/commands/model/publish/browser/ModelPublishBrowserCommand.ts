/**
 * Model Publish Command - Browser Implementation
 *
 * Publish a forged model to HuggingFace — safetensors, config, tokenizer, model card, and alloy provenance. This is the Factory's shipping department: the forge produces the artifact on a grid node, this command pushes it to HuggingFace where anyone can download it. Supports publishing from a local forged directory (bigmama-style) or from a grid node's finished/ station via grid/send.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ModelPublishParams, ModelPublishResult } from '../shared/ModelPublishTypes';

export class ModelPublishBrowserCommand extends CommandBase<ModelPublishParams, ModelPublishResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('model/publish', context, subpath, commander);
  }

  async execute(params: ModelPublishParams): Promise<ModelPublishResult> {
    console.log('🌐 BROWSER: Delegating Model Publish to server');
    return await this.remoteExecute(params);
  }
}
