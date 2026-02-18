/**
 * Continuum Set Command - Shared Base
 *
 * Universal control of the Continuum widget status
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { ContinuumSetParams, ContinuumSetResult } from './ContinuumSetTypes';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';

export abstract class ContinuumSetCommand extends CommandBase<ContinuumSetParams, ContinuumSetResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('continuum/set', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<ContinuumSetResult> {
    return this.executeContinuumSet(params as ContinuumSetParams);
  }

  protected abstract executeContinuumSet(params: ContinuumSetParams): Promise<ContinuumSetResult>;
}
