/**
 * State Create Server Command - Delegate to browser for user context
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import type { StateCreateParams, StateCreateResult } from '../shared/StateCreateTypes';
import type { BaseEntity } from '../../../../system/data/entities/BaseEntity';

export class StateCreateServerCommand extends CommandBase<StateCreateParams, StateCreateResult<BaseEntity>> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('state/create', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<StateCreateResult<BaseEntity>> {
    const stateParams = params as StateCreateParams;

    console.log(`🔧 StateCreateServer: Delegating state create to browser for user context`);

    // State operations need browser environment for session context
    return await this.remoteExecute(stateParams);
  }
}