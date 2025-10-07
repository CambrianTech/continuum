/**
 * Model List Browser Command
 *
 * Browser stub - delegates to server
 */

import { ModelListCommand } from '../shared/ModelListCommand';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { ModelListParams, ModelListResult } from '../shared/ModelListTypes';

export class ModelListBrowserCommand extends ModelListCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('model/list', context, subpath, commander);
  }

  async execute(params: ModelListParams): Promise<ModelListResult> {
    // Browser delegates to server
    return await this.remoteExecute(params);
  }
}
