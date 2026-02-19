/**
 * State Update Server Command
 *
 * Simple delegation to browser for user context injection
 * Server command just passes through to browser environment
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import type { StateUpdateParams, StateUpdateResult } from '../shared/StateUpdateTypes';
import type { BaseEntity } from '../../../../system/data/entities/BaseEntity';

export class StateUpdateServerCommand extends CommandBase<StateUpdateParams, StateUpdateResult<BaseEntity>> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('state/update', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<StateUpdateResult<BaseEntity>> {
    const stateParams = params as StateUpdateParams;
    console.log(`🔧 StateUpdateServer: Delegating state update to browser for user context`);
    return await this.remoteExecute(stateParams);
  }
}