/**
 * Adapter Adopt Command - Browser Implementation
 *
 * Add an adapter to a persona's genome, making it a permanent trait
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { AdapterAdoptParams, AdapterAdoptResult } from '../shared/AdapterAdoptTypes';

export class AdapterAdoptBrowserCommand extends CommandBase<AdapterAdoptParams, AdapterAdoptResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('adapter/adopt', context, subpath, commander);
  }

  async execute(params: AdapterAdoptParams): Promise<AdapterAdoptResult> {
    console.log('🌐 BROWSER: Delegating Adapter Adopt to server');
    return await this.remoteExecute(params);
  }
}
