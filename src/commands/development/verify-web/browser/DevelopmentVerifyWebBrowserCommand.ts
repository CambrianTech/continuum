/**
 * Development Verify Web Command - Browser Implementation
 *
 * Verify web output by opening in headless Playwright browser, capturing console errors + screenshot. Used by Academy teacher to grade coding output. No blind training.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { DevelopmentVerifyWebParams, DevelopmentVerifyWebResult } from '../shared/DevelopmentVerifyWebTypes';

export class DevelopmentVerifyWebBrowserCommand extends CommandBase<DevelopmentVerifyWebParams, DevelopmentVerifyWebResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('development/verify-web', context, subpath, commander);
  }

  async execute(params: DevelopmentVerifyWebParams): Promise<DevelopmentVerifyWebResult> {
    console.log('🌐 BROWSER: Delegating Development Verify Web to server');
    return await this.remoteExecute(params);
  }
}
