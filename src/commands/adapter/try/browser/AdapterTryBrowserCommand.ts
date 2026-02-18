/**
 * Adapter Try Command - Browser Implementation
 *
 * Temporarily load a LoRA adapter and run A/B comparison test
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { AdapterTryParams, AdapterTryResult } from '../shared/AdapterTryTypes';

export class AdapterTryBrowserCommand extends CommandBase<AdapterTryParams, AdapterTryResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('adapter/try', context, subpath, commander);
  }

  async execute(params: AdapterTryParams): Promise<AdapterTryResult> {
    console.log('🌐 BROWSER: Delegating Adapter Try to server');
    return await this.remoteExecute(params);
  }
}
