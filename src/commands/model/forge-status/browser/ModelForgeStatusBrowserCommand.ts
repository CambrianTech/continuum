/**
 * Model Forge Status Command - Browser Implementation
 *
 * Get the current status of active model forges — phase, step, loss, VRAM usage, ETA. Polls status.json from forge nodes on the grid.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ModelForgeStatusParams, ModelForgeStatusResult } from '../shared/ModelForgeStatusTypes';

export class ModelForgeStatusBrowserCommand extends CommandBase<ModelForgeStatusParams, ModelForgeStatusResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('model/forge-status', context, subpath, commander);
  }

  async execute(params: ModelForgeStatusParams): Promise<ModelForgeStatusResult> {
    console.log('🌐 BROWSER: Delegating Model Forge Status to server');
    return await this.remoteExecute(params);
  }
}
