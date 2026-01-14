/**
 * Ai Context Slice Command - Browser Implementation
 *
 * Retrieve full content of a context item by ID - companion to context/search for getting complete entity data
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { AiContextSliceParams, AiContextSliceResult } from '../shared/AiContextSliceTypes';

export class AiContextSliceBrowserCommand extends CommandBase<AiContextSliceParams, AiContextSliceResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/context/slice', context, subpath, commander);
  }

  async execute(params: AiContextSliceParams): Promise<AiContextSliceResult> {
    console.log('🌐 BROWSER: Delegating Ai Context Slice to server');
    return await this.remoteExecute(params);
  }
}
