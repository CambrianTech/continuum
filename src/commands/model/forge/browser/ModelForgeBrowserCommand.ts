/**
 * Model Forge Command - Browser Implementation
 *
 * Start a model forge job — sends forge parameters to a grid node with GPU for training. Returns job ID for status tracking.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ModelForgeParams, ModelForgeResult } from '../shared/ModelForgeTypes';

export class ModelForgeBrowserCommand extends CommandBase<ModelForgeParams, ModelForgeResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('model/forge', context, subpath, commander);
  }

  async execute(params: ModelForgeParams): Promise<ModelForgeResult> {
    console.log('🌐 BROWSER: Delegating Model Forge to server');
    return await this.remoteExecute(params);
  }
}
