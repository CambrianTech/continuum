/**
 * Ai Key Test Command - Browser Implementation
 *
 * Test an API key before saving it. Makes a minimal API call to verify the key is valid and has sufficient permissions.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { AiKeyTestParams, AiKeyTestResult } from '../shared/AiKeyTestTypes';

export class AiKeyTestBrowserCommand extends CommandBase<AiKeyTestParams, AiKeyTestResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/key/test', context, subpath, commander);
  }

  async execute(params: AiKeyTestParams): Promise<AiKeyTestResult> {
    console.log('🌐 BROWSER: Delegating Ai Key Test to server');
    return await this.remoteExecute(params);
  }
}
