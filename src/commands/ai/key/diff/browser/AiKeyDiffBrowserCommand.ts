/**
 * Ai Key Diff Command - Browser Implementation
 *
 * Compare redacted AI key status entries and produce a value-free merge plan for trusted grid reconciliation.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { AiKeyDiffParams, AiKeyDiffResult } from '../shared/AiKeyDiffTypes';

export class AiKeyDiffBrowserCommand extends CommandBase<AiKeyDiffParams, AiKeyDiffResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/key/diff', context, subpath, commander);
  }

  async execute(params: AiKeyDiffParams): Promise<AiKeyDiffResult> {
    console.log('🌐 BROWSER: Delegating Ai Key Diff to server');
    return await this.remoteExecute(params);
  }
}
