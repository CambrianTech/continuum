/**
 * State Get Server Command - Delegate to browser for user context
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import type { StateGetParams, StateGetResult } from '../shared/StateGetTypes';
import type { BaseEntity } from '../../../../system/data/entities/BaseEntity';

export class StateGetServerCommand extends CommandBase<StateGetParams, StateGetResult<BaseEntity>> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('state/get', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<StateGetResult<BaseEntity>> {
    const stateParams = params as StateGetParams;

    console.log(`🔧 StateGetServer: Delegating state get to browser for user context`);

    // State operations need browser environment for session context
    return await this.remoteExecute(stateParams);
  }
}