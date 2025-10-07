/**
 * Model Find Browser Command
 *
 * Browser stub - delegates to server
 */

import { ModelFindCommand } from '../shared/ModelFindCommand';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { ModelFindParams, ModelFindResult } from '../shared/ModelFindTypes';

export class ModelFindBrowserCommand extends ModelFindCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('model/find', context, subpath, commander);
  }

  async execute(params: ModelFindParams): Promise<ModelFindResult> {
    // Browser delegates to server
    return await this.remoteExecute(params);
  }
}
