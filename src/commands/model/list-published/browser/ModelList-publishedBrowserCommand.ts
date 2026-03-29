/**
 * Model List Published Command - Browser Implementation
 *
 * List all published models from the continuum-ai HuggingFace org — download counts, likes, improvement scores, hardware targets, tags.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ModelListPublishedParams, ModelListPublishedResult } from '../shared/ModelList-publishedTypes';

export class ModelListPublishedBrowserCommand extends CommandBase<ModelListPublishedParams, ModelListPublishedResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('model/list-published', context, subpath, commander);
  }

  async execute(params: ModelListPublishedParams): Promise<ModelListPublishedResult> {
    console.log('🌐 BROWSER: Delegating Model List Published to server');
    return await this.remoteExecute(params);
  }
}
